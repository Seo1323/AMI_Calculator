<script lang="ts">
	import { t, fmtMoney, fmtNumber, errorText } from '$lib/i18n/index.svelte';
	import { settings, widgetList } from '$lib/settings.svelte';
	import { invoke, isTauri } from '$lib/tauri';
	import { quotes } from '$lib/quotes.svelte';
	import AllocationBar from '$lib/components/AllocationBar.svelte';
	import RefreshQuotes from '$lib/components/RefreshQuotes.svelte';
	import type { Dashboard } from '$lib/types';

	let data = $state<Dashboard | null>(null);
	let error = $state('');

	$effect(() => {
		void quotes.version; // reload after quote refreshes
		const currency = settings.baseCurrency;
		if (!isTauri()) return;
		invoke<Dashboard>('dashboard', { currency })
			.then((d) => (data = d))
			.catch((e) => (error = errorText(e)));
	});

	// One color slot per instrument across all portfolios (sorted by symbol).
	const slots = $derived.by(() => {
		const symbols = new Map<string, string>();
		for (const p of data?.portfolios ?? []) for (const a of p.allocation) symbols.set(a.instrumentId, a.symbol);
		const ordered = [...symbols.entries()].sort((a, b) => a[1].localeCompare(b[1]));
		return new Map(ordered.map(([id], i) => [id, i]));
	});
	const colorOf = (id: string) => slots.get(id) ?? 8;
	const widgets = $derived(widgetList().filter((w) => w.visible));
	const empty = $derived(data !== null && data.portfolios.length === 0);
</script>

<div class="stack">
	<header class="head">
		<h1>{t('dashboard.title')}</h1>
		<RefreshQuotes />
	</header>

	{#if !isTauri()}
		<div class="notice">{t('common.notInTauri')}</div>
	{/if}
	{#if error}<div class="notice error">{error}</div>{/if}

	{#if data}
		{#if empty}
			<section class="card">
				<h2>{t('dashboard.welcome')}</h2>
				<p class="muted">{t('dashboard.start')}</p>
			</section>
		{/if}

		{#if data.missingRates.length}
			<div class="notice warn">{t('dashboard.missingRates', { list: data.missingRates.join(', ') })}</div>
		{/if}

		<div class="grid">
			{#each widgets as w (w.id)}
				{#if w.id === 'netWorth'}
		<!-- Total value -->
		<section class="card hero wide">
			<div>
				<span class="muted">{t('dashboard.netWorth')}</span>
				<div class="hero-num">{fmtMoney(data.totalValue, data.currency)}</div>
				<span class="muted small">{t('dashboard.netWorthHint', { currency: data.currency })}</span>
			</div>
			<dl class="split">
				<div>
					<dt>{t('dashboard.gain')}</dt>
					<dd class:pos={Number(data.totalGain) > 0} class:neg={Number(data.totalGain) < 0}>{fmtMoney(data.totalGain, data.currency)}</dd>
				</div>
			</dl>
		</section>

				{:else if w.id === 'portfolios'}
			<!-- Portfolios -->
			<section class="card stack">
				<div class="section-head">
					<h2>{t('dashboard.portfoliosTitle')}</h2>
					<a href="/portfolio" class="small">
						{data.portfolios.length ? t('dashboard.allPortfolios') : t('dashboard.createPortfolio')}
					</a>
				</div>
				{#if data.portfolios.length === 0}
					<p class="muted">{t('dashboard.noPortfolios')}</p>
				{/if}
				{#each data.portfolios as p (p.id)}
					<a class="pf" href={`/portfolio/${p.id}`}>
						<div class="pf-head">
							<strong>{p.name}</strong>
							<span class="num">{p.value === null ? '—' : fmtMoney(p.value, data.currency)}</span>
						</div>
						<div class="pf-sub small">
							{#if !p.complete}
								<span class="muted">{t('dashboard.incomplete')}</span>
							{:else if p.needsRebalance}
								<span class="status warn-text">⚠ {t('dashboard.needsRebalance')} · {fmtNumber(p.maxDeviation ?? 0)} {t('unit.pp')}</span>
							{:else}
								<span class="status ok-text">✓ {t('dashboard.inTolerance')}</span>
							{/if}
							{#if p.gain !== null}
								<span class:pos={Number(p.gain) > 0} class:neg={Number(p.gain) < 0}>{fmtMoney(p.gain, data.currency)}</span>
							{/if}
						</div>
						{#if p.allocation.length}
							<AllocationBar
								{colorOf}
								slices={p.allocation.map((a) => ({
									id: a.instrumentId,
									label: a.symbol,
									current: Number(a.currentPercent),
									target: Number(a.targetPercent)
								}))}
							/>
						{/if}
					</a>
				{/each}
			</section>
				{/if}
			{/each}
		</div>
	{/if}
</div>

<style>
	p {
		margin: 0;
	}
	.small {
		font-size: 0.85rem;
	}
	.head,
	.section-head {
		display: flex;
		flex-wrap: wrap;
		justify-content: space-between;
		align-items: center;
		gap: var(--space);
	}
	.hero {
		display: flex;
		flex-wrap: wrap;
		justify-content: space-between;
		align-items: end;
		gap: var(--space);
	}
	.hero-num {
		font-size: clamp(1.8rem, 8vw, 2.4rem);
		font-weight: 700;
		font-variant-numeric: tabular-nums;
		line-height: 1.15;
	}
	.split {
		display: flex;
		flex-wrap: wrap;
		gap: calc(var(--space) * 2);
		margin: 0;
	}
	.split dt {
		font-size: 0.8rem;
		color: var(--text-muted);
	}
	.split dd {
		margin: 0;
		font-size: 1.15rem;
		font-weight: 600;
		font-variant-numeric: tabular-nums;
	}
	.wide {
		grid-column: 1 / -1;
	}
	.grid {
		display: grid;
		grid-template-columns: repeat(2, minmax(0, 1fr));
		gap: var(--space);
		align-items: start;
	}
	@media (max-width: 980px) {
		.grid {
			grid-template-columns: minmax(0, 1fr);
		}
	}
	.pf {
		display: flex;
		flex-direction: column;
		gap: 0.4rem;
		padding: calc(var(--space) * 0.8);
		border: 1px solid var(--border);
		border-radius: calc(var(--radius) * 0.8);
		color: inherit;
		text-decoration: none;
	}
	.pf:hover {
		border-color: var(--accent);
	}
	.pf-head,
	.pf-sub {
		display: flex;
		justify-content: space-between;
		gap: 0.5rem;
		font-variant-numeric: tabular-nums;
	}
	.warn-text {
		color: var(--warning);
		font-weight: 600;
	}
	.ok-text {
		color: var(--positive);
	}
</style>
