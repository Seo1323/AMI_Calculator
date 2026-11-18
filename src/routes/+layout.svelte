<script lang="ts">
	import '../app.css';
	import { page } from '$app/state';
	import { goto } from '$app/navigation';
	import { onMount } from 'svelte';
	import { settings, saveSettings, hydrateSettings } from '$lib/settings.svelte';
	import { t, type MessageKey } from '$lib/i18n/index.svelte';
	import Icon from '$lib/components/Icon.svelte';

	let { children } = $props();

	const media = typeof window !== 'undefined' ? window.matchMedia('(prefers-color-scheme: dark)') : null;
	let systemDark = $state(media?.matches ?? false);

	onMount(() => {
		const onChange = (e: MediaQueryListEvent) => (systemDark = e.matches);
		media?.addEventListener('change', onChange);
		hydrateSettings().then(() => {
			if (page.url.pathname === '/' && settings.startPage !== '/') goto(settings.startPage, { replaceState: true });
		});
		return () => media?.removeEventListener('change', onChange);
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
		root.lang = settings.locale;
		saveSettings();
	});

	const nav: { href: string; key: MessageKey; icon: 'dashboard' | 'budget' | 'portfolio' | 'rebalance' | 'settings' }[] = [
		{ href: '/', key: 'nav.dashboard', icon: 'dashboard' },
		{ href: '/budget', key: 'nav.budget', icon: 'budget' },
		{ href: '/portfolio', key: 'nav.portfolio', icon: 'portfolio' },
		{ href: '/rebalance', key: 'nav.rebalance', icon: 'rebalance' },
		{ href: '/settings', key: 'nav.settings', icon: 'settings' }
	];

	const isActive = (href: string) =>
		href === '/' ? page.url.pathname === '/' : page.url.pathname.startsWith(href);
</script>

<div class="shell" class:collapsed={settings.sidebarCollapsed}>
	<nav class="sidebar" aria-label="main">
		<div class="brand">
			<span class="logo" aria-hidden="true">⚖</span>
			<span class="label">{t('app.name')}</span>
		</div>
		{#each nav as item (item.href)}
			<a href={item.href} class:active={isActive(item.href)} title={t(item.key)}>
				<Icon name={item.icon} />
				<span class="label">{t(item.key)}</span>
			</a>
		{/each}
		<button
			class="btn ghost collapse"
			title={t('nav.collapse')}
			aria-label={t('nav.collapse')}
			onclick={() => (settings.sidebarCollapsed = !settings.sidebarCollapsed)}
		>
			<span class="chev"><Icon name="collapse" /></span>
		</button>
	</nav>

	<main>
		{@render children()}
	</main>
</div>

<style>
	.shell {
		display: grid;
		grid-template-columns: 220px 1fr;
		min-height: 100vh;
	}
	.shell.collapsed {
		grid-template-columns: 64px 1fr;
	}
	.sidebar {
		position: sticky;
		top: 0;
		height: 100vh;
		display: flex;
		flex-direction: column;
		gap: 0.25rem;
		padding: var(--space) 0.6rem;
		background: var(--surface);
		border-right: 1px solid var(--border);
	}
	.brand {
		display: flex;
		align-items: center;
		gap: 0.6rem;
		font-weight: 700;
		font-size: 1.15rem;
		padding: 0.3rem 0.6rem calc(var(--space) * 1.2);
	}
	.logo {
		color: var(--accent);
		font-size: 1.4rem;
		width: 24px;
		text-align: center;
	}
	.sidebar a {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		padding: calc(var(--space) * 0.55) 0.7rem;
		border-radius: calc(var(--radius) * 0.7);
		color: var(--text-muted);
		text-decoration: none;
		white-space: nowrap;
	}
	.sidebar a:hover {
		background: var(--surface-2);
		color: var(--text);
	}
	.sidebar a.active {
		background: var(--accent-soft);
		color: var(--accent-text);
		font-weight: 600;
	}
	.collapse {
		margin-top: auto;
		align-self: flex-end;
	}
	.collapsed .collapse {
		align-self: center;
	}
	.collapsed .chev {
		transform: rotate(180deg);
	}
	.chev {
		display: inline-flex;
	}
	.collapsed .label {
		display: none;
	}
	main {
		padding: calc(var(--space) * 1.5);
		min-width: 0;
		max-width: 1400px;
		width: 100%;
	}

	/* Phones: bottom navigation bar */
	@media (max-width: 720px) {
		.shell,
		.shell.collapsed {
			grid-template-columns: 1fr;
		}
		.sidebar {
			position: fixed;
			top: auto;
			bottom: 0;
			left: 0;
			right: 0;
			height: auto;
			z-index: 10;
			flex-direction: row;
			justify-content: space-around;
			padding: 0.3rem 0.3rem calc(0.3rem + env(safe-area-inset-bottom));
			border-right: none;
			border-top: 1px solid var(--border);
		}
		.brand,
		.collapse {
			display: none;
		}
		.sidebar a {
			flex-direction: column;
			gap: 0.15rem;
			font-size: 0.7rem;
			padding: 0.35rem 0.4rem;
			flex: 1;
			min-width: 0;
		}
		.sidebar a .label,
		.collapsed .sidebar a .label {
			display: block;
			overflow: hidden;
			text-overflow: ellipsis;
			max-width: 100%;
		}
		main {
			padding: 1rem 1rem 5.5rem;
		}
	}
</style>
