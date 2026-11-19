<script lang="ts">
	import '../app.css';
	import { page } from '$app/state';
	import { goto } from '$app/navigation';
	import { onMount, tick } from 'svelte';
	import { fade } from 'svelte/transition';
	import { settings, saveSettings, hydrateSettings } from '$lib/settings.svelte';
	import { t, type MessageKey } from '$lib/i18n/index.svelte';
	import Icon from '$lib/components/Icon.svelte';
	import { startAutoRefresh } from '$lib/quotes.svelte';
	import { applyUiScale } from '$lib/uiScale';

	let { children } = $props();

	const media = typeof window !== 'undefined' ? window.matchMedia('(prefers-color-scheme: dark)') : null;
	const motionQuery = typeof window !== 'undefined' ? window.matchMedia('(prefers-reduced-motion: reduce)') : null;
	let systemDark = $state(media?.matches ?? false);
	let systemReducedMotion = $state(motionQuery?.matches ?? false);
	let ready = $state(false);
	// Medium windows collapse the sidebar to icons so the content keeps its room.
	// The width is divided by the text size, because larger text leaves less room.
	let viewportWidth = $state(typeof window !== 'undefined' ? window.innerWidth : 1280);
	// CSS zoom of the fallback in uiScale.ts; the viewport width does not shrink with it.
	let cssZoom = $state(1);
	const narrow = $derived(viewportWidth / (settings.fontScale * cssZoom) <= 1024);

	$effect(() => {
		const wanted = settings.uiScale;
		applyUiScale(wanted).then((z) => {
			if (wanted === settings.uiScale) cssZoom = z;
		});
	});
	const collapsed = $derived(settings.sidebarCollapsed || narrow);
	const animate = $derived(!settings.reduceMotion && !systemReducedMotion);

	// (Re)start automatic quote downloads once settings are loaded and whenever they change.
	$effect(() => {
		void settings.autoRefreshQuotes;
		void settings.refreshMinutes;
		if (ready) startAutoRefresh();
	});

	type IconName = 'dashboard' | 'portfolio' | 'rebalance' | 'settings';
	const nav: { href: string; key: MessageKey; icon: IconName }[] = [
		{ href: '/', key: 'nav.dashboard', icon: 'dashboard' },
		{ href: '/portfolio', key: 'nav.portfolio', icon: 'portfolio' },
		{ href: '/rebalance', key: 'nav.rebalance', icon: 'rebalance' },
		{ href: '/settings', key: 'nav.settings', icon: 'settings' }
	];

	const isActive = (href: string) => (href === '/' ? page.url.pathname === '/' : page.url.pathname.startsWith(href));

	function onKey(e: KeyboardEvent) {
		// Alt+1..4 jumps between sections; Alt+H hides amounts; Alt+B toggles the sidebar.
		if (!e.altKey || e.ctrlKey || e.metaKey) return;
		const n = Number(e.key);
		if (n >= 1 && n <= nav.length) {
			e.preventDefault();
			goto(nav[n - 1].href);
		} else if (e.key.toLowerCase() === 'h') {
			e.preventDefault();
			settings.hideAmounts = !settings.hideAmounts;
		} else if (e.key.toLowerCase() === 'b') {
			e.preventDefault();
			settings.sidebarCollapsed = !settings.sidebarCollapsed;
		}
	}

	onMount(() => {
		const onDark = (e: MediaQueryListEvent) => (systemDark = e.matches);
		const onMotion = (e: MediaQueryListEvent) => (systemReducedMotion = e.matches);
		media?.addEventListener('change', onDark);
		motionQuery?.addEventListener('change', onMotion);
		hydrateSettings().then(() => {
			ready = true;
			if (page.url.pathname === '/' && settings.startPage !== '/') goto(settings.startPage, { replaceState: true });
		});
		return () => {
			media?.removeEventListener('change', onDark);
			motionQuery?.removeEventListener('change', onMotion);
		};
	});

	$effect(() => {
		const root = document.documentElement;
		const dark = settings.theme === 'dark' || (settings.theme === 'system' && systemDark);
		root.dataset.theme = dark ? 'dark' : 'light';
		root.dataset.density = settings.density;
		root.style.setProperty('--accent', settings.accent);
		root.style.setProperty('--font-scale', String(settings.fontScale));
		root.style.setProperty('--radius', `${settings.radius}px`);
		root.toggleAttribute('data-reduce-motion', settings.reduceMotion);
		// Breakpoints for hiding table columns, counted after text scaling (see `narrow`).
		const effective = viewportWidth / (settings.fontScale * cssZoom);
		for (const w of [820, 480, 400]) root.toggleAttribute(`data-lt${w}`, effective <= w);
		root.toggleAttribute('data-hide-amounts', settings.hideAmounts);
		root.lang = settings.locale;
		saveSettings();
	});

	// ----- Sliding highlight behind the active item -----
	let linksEl: HTMLElement | undefined = $state();
	const linkEls: Record<string, HTMLElement> = {};
	let indicator = $state({ x: 0, y: 0, w: 0, h: 0, visible: false });

	async function placeIndicator() {
		await tick();
		const active = nav.find((n) => isActive(n.href));
		const el = active ? linkEls[active.href] : undefined;
		if (!el) {
			indicator.visible = false;
			return;
		}
		indicator = { x: el.offsetLeft, y: el.offsetTop, w: el.offsetWidth, h: el.offsetHeight, visible: true };
	}

	$effect(() => {
		void page.url.pathname;
		void collapsed;
		placeIndicator();
	});

	$effect(() => {
		if (!linksEl) return;
		// Follows width changes while the sidebar animates and on phone/desktop layout switches.
		const ro = new ResizeObserver(() => placeIndicator());
		ro.observe(linksEl);
		return () => ro.disconnect();
	});
</script>

<svelte:window onkeydown={onKey} bind:innerWidth={viewportWidth} />

<div class="shell" class:collapsed class:animate>
	<nav class="sidebar" aria-label="main">
		<div class="brand">
			<span class="logo" aria-hidden="true">⚖</span>
			<span class="label">{t('app.name')}</span>
		</div>

		<div class="links" bind:this={linksEl}>
			<span
				class="indicator"
				class:visible={indicator.visible}
				style:transform={`translate(${indicator.x}px, ${indicator.y}px)`}
				style:width={`${indicator.w}px`}
				style:height={`${indicator.h}px`}
				aria-hidden="true"
			></span>
			{#each nav as item, i (item.href)}
				<a
					href={item.href}
					class:active={isActive(item.href)}
					title={`${t(item.key)} (Alt+${i + 1})`}
					bind:this={linkEls[item.href]}
					aria-current={isActive(item.href) ? 'page' : undefined}
				>
					<span class="icon"><Icon name={item.icon} /></span>
					<span class="label">{t(item.key)}</span>
				</a>
			{/each}
		</div>

		<div class="bottom">
			<button
				class="btn ghost tool"
				title={`${t('nav.hideAmounts')} (Alt+H)`}
				aria-label={t('nav.hideAmounts')}
				aria-pressed={settings.hideAmounts}
				onclick={() => (settings.hideAmounts = !settings.hideAmounts)}
			>
				<Icon name={settings.hideAmounts ? 'eyeOff' : 'eye'} />
				<span class="label">{t('nav.hideAmounts')}</span>
			</button>
			<button
				class="btn ghost tool"
				title={`${t('nav.collapse')} (Alt+B)`}
				aria-label={t('nav.collapse')}
				aria-expanded={!collapsed}
				onclick={() => (settings.sidebarCollapsed = !settings.sidebarCollapsed)}
			>
				<span class="chev"><Icon name="collapse" /></span>
				<span class="label">{t('nav.collapse')}</span>
			</button>
		</div>
	</nav>

	<main>
		{#key page.url.pathname}
			<div class="page" in:fade={{ duration: animate ? 160 : 0 }}>
				{@render children()}
			</div>
		{/key}
	</main>
</div>

<style>
	.shell {
		--sidebar-w: 14.7rem;
		--ease: cubic-bezier(0.2, 0.8, 0.2, 1);
		display: flex;
		min-height: 100vh;
	}
	.shell.collapsed {
		--sidebar-w: 4.3rem;
	}
	.sidebar {
		position: sticky;
		top: 0;
		height: 100vh;
		flex: none;
		width: var(--sidebar-w);
		display: flex;
		flex-direction: column;
		gap: 0.25rem;
		padding: var(--space) 0.6rem;
		background: var(--surface);
		border-right: 1px solid var(--border);
		overflow: hidden;
	}
	.animate .sidebar {
		transition: width 0.28s var(--ease);
	}
	.brand {
		display: flex;
		align-items: center;
		gap: 0.6rem;
		font-weight: 700;
		font-size: 1.15rem;
		padding: 0.3rem 0.6rem calc(var(--space) * 1.2);
		white-space: nowrap;
	}
	.logo {
		color: var(--accent);
		font-size: 1.4rem;
		width: 24px;
		flex: none;
		text-align: center;
	}
	.animate .logo {
		transition: transform 0.4s var(--ease);
	}
	.brand:hover .logo {
		transform: rotate(-12deg);
	}

	.links {
		position: relative;
		display: flex;
		flex-direction: column;
		gap: 0.25rem;
	}
	.indicator {
		position: absolute;
		left: 0;
		top: 0;
		border-radius: calc(var(--radius) * 0.7);
		background: var(--accent-soft);
		opacity: 0;
		pointer-events: none;
	}
	.indicator.visible {
		opacity: 1;
	}
	.animate .indicator {
		transition:
			transform 0.3s var(--ease),
			width 0.28s var(--ease),
			height 0.28s var(--ease),
			opacity 0.2s;
	}

	.sidebar a,
	.tool {
		position: relative;
		display: flex;
		align-items: center;
		gap: 0.75rem;
		padding: calc(var(--space) * 0.55) 0.7rem;
		border-radius: calc(var(--radius) * 0.7);
		color: var(--text-muted);
		text-decoration: none;
		white-space: nowrap;
	}
	.animate .sidebar a,
	.animate .tool {
		transition:
			color 0.15s,
			background 0.15s;
	}
	.sidebar a:hover,
	.tool:hover {
		background: var(--surface-2);
		color: var(--text);
	}
	.sidebar a.active {
		color: var(--accent-text);
		font-weight: 600;
		background: transparent;
	}
	.icon {
		display: inline-flex;
		flex: none;
	}
	.animate .sidebar a:hover .icon {
		transform: translateX(2px);
	}
	.animate .icon {
		transition: transform 0.2s var(--ease);
	}
	.animate .sidebar a:active .icon {
		transform: scale(0.9);
	}

	.label {
		display: inline-block;
		opacity: 1;
		transform: translateX(0);
		max-width: 12rem;
	}
	.animate .label {
		transition:
			opacity 0.2s 0.06s,
			transform 0.28s var(--ease),
			max-width 0.28s var(--ease);
	}
	.collapsed .label {
		opacity: 0;
		transform: translateX(-6px);
		max-width: 0;
		pointer-events: none;
	}
	.animate.collapsed .label {
		transition:
			opacity 0.12s,
			transform 0.2s var(--ease),
			max-width 0.28s var(--ease);
	}

	.bottom {
		margin-top: auto;
		display: flex;
		flex-direction: column;
		gap: 0.25rem;
	}
	.tool {
		border: none;
		background: transparent;
		width: 100%;
		justify-content: flex-start;
		font-size: 0.9rem;
	}
	.chev {
		display: inline-flex;
		flex: none;
	}
	.animate .chev {
		transition: transform 0.3s var(--ease);
	}
	.collapsed .chev {
		transform: rotate(180deg);
	}

	main {
		flex: 1;
		min-width: 0;
		padding: calc(var(--space) * 1.5);
		max-width: 94rem;
		margin-inline: auto;
	}

	/* Phones: bottom navigation bar */
	@media (max-width: 720px) {
		.shell,
		.shell.collapsed {
			--sidebar-w: 100%;
			display: block;
		}
		.sidebar {
			position: fixed;
			top: auto;
			bottom: 0;
			left: 0;
			right: 0;
			height: auto;
			z-index: 10;
			padding: 0.3rem 0.3rem calc(0.3rem + env(safe-area-inset-bottom));
			border-right: none;
			border-top: 1px solid var(--border);
		}
		.animate .sidebar {
			transition: none;
		}
		.brand,
		.bottom {
			display: none;
		}
		.links {
			flex-direction: row;
			justify-content: space-around;
		}
		.sidebar a {
			flex-direction: column;
			gap: 0.15rem;
			font-size: 0.7rem;
			padding: 0.35rem 0.3rem;
			flex: 1;
			min-width: 0;
		}
		.sidebar a .label,
		.collapsed .sidebar a .label {
			opacity: 1;
			transform: none;
			max-width: 100%;
			overflow: hidden;
			text-overflow: ellipsis;
		}
		@media (max-width: 400px) {
			.sidebar a {
				font-size: 0.62rem;
			}
		}
		main {
			padding: calc(1rem + env(safe-area-inset-top)) calc(1rem + env(safe-area-inset-right)) calc(5.5rem + env(safe-area-inset-bottom))
				calc(1rem + env(safe-area-inset-left));
		}
	}
</style>
