<script lang="ts">
	import '@fontsource-variable/bricolage-grotesque';
	import '../app.css';
	import { page } from '$app/state';
	import { goto } from '$app/navigation';
	import { onMount, tick } from 'svelte';
	import { fade } from 'svelte/transition';
	import { settings, saveSettings, hydrateSettings } from '$lib/settings.svelte';
	import { t, type MessageKey } from '$lib/i18n/index.svelte';
	import Icon from '$lib/components/Icon.svelte';
	import { startAutoRefresh } from '$lib/quotes.svelte';
	import { sync, startSync } from '$lib/sync.svelte';
	import { isTauri } from '$lib/tauri';
	import { applyUiScale } from '$lib/uiScale';
	import { perf, clearSlow, startWatching } from '$lib/perf.svelte';
	import { perfActive } from '$lib/perf';
	import { isActivePath, isRoot } from '$lib/route';

	let { children } = $props();

	const media = typeof window !== 'undefined' ? window.matchMedia('(prefers-color-scheme: dark)') : null;
	const motionQuery = typeof window !== 'undefined' ? window.matchMedia('(prefers-reduced-motion: reduce)') : null;
	let systemDark = $state(media?.matches ?? false);
	let systemReducedMotion = $state(motionQuery?.matches ?? false);
	let ready = $state(false);
	// Medium windows collapse the sidebar to icons so the content keeps its room.
	// The width is divided by the text size, because larger text leaves less room.
	// "AMI Calculator": the first word is set in bold.
	const brand = $derived.by(() => {
		const [first, ...rest] = t('app.name').split(' ');
		return { first, rest: rest.join(' ') };
	});
	let viewportWidth = $state(typeof window !== 'undefined' ? window.innerWidth : 1280);
	// CSS zoom of the fallback in uiScale.ts; the viewport width does not shrink with it.
	let cssZoom = $state(1);
	const effective = $derived(viewportWidth / (settings.fontScale * cssZoom));
	const narrow = $derived(effective <= 1024);
	// Breakpoints for hiding table columns. Booleans, so the effect below re-runs only when one is
	// crossed, not on every pixel of a window drag.
	const lt820 = $derived(effective <= 820);
	const lt480 = $derived(effective <= 480);
	const lt400 = $derived(effective <= 400);

	// Performance mode: chosen, or in auto mode once slow painting was seen while scrolling.
	$effect(() => {
		document.documentElement.dataset.perf = perfActive(settings.perfMode, perf.slowSeen) ? 'on' : 'off';
	});
	$effect(() => {
		if (settings.perfMode === 'off' && perf.slowSeen) clearSlow(); // choosing "off" resets the verdict
		if (settings.perfMode !== 'auto' || perf.slowSeen) return;
		return startWatching();
	});

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

	const isActive = (href: string) => isActivePath(href, page.url.pathname);

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
		let stopSync = () => {};
		hydrateSettings().then(() => {
			ready = true;
			if (isTauri()) stopSync = startSync();
			// isRoot: the root page has an empty pathname under tauri://localhost (Linux, macOS).
			if (isRoot(page.url.pathname) && settings.startPage !== '/') goto(settings.startPage, { replaceState: true });
		});
		return () => {
			media?.removeEventListener('change', onDark);
			motionQuery?.removeEventListener('change', onMotion);
			stopSync();
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
		root.dataset.font = settings.fontFamily;
		root.dataset.shadows = settings.shadows;
		root.dataset.darkStyle = settings.darkStyle;
		root.dataset.width = settings.contentWidth;
		root.toggleAttribute('data-nav-top', settings.navTop);
		root.toggleAttribute('data-no-glow', !settings.glow);
		root.toggleAttribute('data-colorblind', settings.colorblind);
		root.toggleAttribute('data-lt820', lt820);
		root.toggleAttribute('data-lt480', lt480);
		root.toggleAttribute('data-lt400', lt400);
		root.toggleAttribute('data-hide-amounts', settings.hideAmounts);
		root.lang = settings.locale;
	});
	$effect(() => saveSettings()); // separate, so dragging the window does not rewrite the settings

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
			<span class="logo" aria-hidden="true"><Icon name="rebalance" size={18} /></span>
			<span class="label wordmark"><b>{brand.first}</b>{brand.rest ? ' ' + brand.rest : ''}</span>
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
		{#key `${page.url.pathname}:${sync.version}`}
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
		background: linear-gradient(180deg, var(--surface), color-mix(in srgb, var(--surface) 94%, var(--accent)));
		border-right: 1px solid color-mix(in srgb, var(--border) 80%, transparent);
		overflow: hidden;
	}
	.animate .sidebar {
		transition: width 0.28s var(--ease);
	}
	.brand {
		display: flex;
		align-items: center;
		gap: 0.6rem;
		font-size: 1.05rem;
		padding: 0.3rem 0.4rem calc(var(--space) * 1.2);
		white-space: nowrap;
	}
	.wordmark {
		font-weight: 500;
		letter-spacing: -0.01em;
	}
	.wordmark b {
		font-weight: 800;
		color: var(--accent-text);
	}
	.logo {
		display: grid;
		place-items: center;
		flex: none;
		width: 2.1rem;
		height: 2.1rem;
		border-radius: calc(var(--radius) * 0.75);
		color: white;
		background: linear-gradient(
			140deg,
			color-mix(in srgb, var(--accent) 72%, white),
			var(--accent) 52%,
			color-mix(in srgb, var(--accent) 72%, black)
		);
		box-shadow:
			inset 0 1px 0 rgb(255 255 255 / 0.3),
			0 6px 14px -6px color-mix(in srgb, var(--accent) 70%, transparent);
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
		box-shadow: inset 3px 0 0 var(--accent);
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
		padding: calc(var(--space) * 1.75) calc(var(--space) * 2);
		max-width: var(--content-max, 94rem);
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
		/* The same bar at the top: the page leaves room above instead of below. */
		:global(:root[data-nav-top]) .sidebar {
			top: 0;
			bottom: auto;
			padding: calc(0.3rem + env(safe-area-inset-top)) 0.3rem 0.3rem;
			border-top: none;
			border-bottom: 1px solid var(--border);
		}
		:global(:root[data-nav-top]) main {
			padding: calc(5.5rem + env(safe-area-inset-top)) calc(1rem + env(safe-area-inset-right)) calc(1rem + env(safe-area-inset-bottom))
				calc(1rem + env(safe-area-inset-left));
		}
	}
</style>
