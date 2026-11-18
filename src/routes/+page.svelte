<script lang="ts">
	import { t, fmtMoney, fmtNumber, numberLocale, errorText } from '$lib/i18n/index.svelte';
	import { settings } from '$lib/settings.svelte';
	import { invoke, isTauri } from '$lib/tauri';
	import { quotes } from '$lib/quotes.svelte';
	import AllocationBar from '$lib/components/AllocationBar.svelte';
	import CategoryBars from '$lib/components/CategoryBars.svelte';
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

	const monthLabel = $derived(
		data ? new Intl.DateTimeFormat(numberLocale(), { month: 'long', year: 'numeric' }).format(new Date(data.month.month + 'T00:00:00')) : ''
	);
	const spending = $derived(
		(data?.month.categories ?? [])
			.filter((c) => c.category.kind === 'expense')
			.map((c) => ({ id: c.category.id, label: c.category.name, value: Number(c.amount), limit: c.limit === null ? null : Number(c.limit) }))
	);
	// One color slot per instrument across all portfolios (sorted by symbol).
	const slots = $derived.by(() => {
		const symbols = new Map<string, string>();
		for (const p of data?.portfolios ?? []) for (const a of p.allocation) symbols.set(a.instrumentId, a.symbol);
		const ordered = [...symbols.entries()].sort((a, b) => a[1].localeCompare(b[1]));
		return new Map(ordered.map(([id], i) => [id, i]));
	});
	const colorOf = (id: string) => slots.get(id) ?? 8;
	const empty = $derived(data !== null && data.portfolios.length === 0 && Number(data.accountsTotal) === 0 && Number(data.month.expense) === 0);
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

		<!-- Net worth -->
		<section class="card hero">
			<div>
				<span class="muted">{t('dashboard.netWorth')}</span>
				<div class="hero-num">{fmtMoney(data.netWorth, data.currency)}</div>
				<span class="muted small">{t('dashboard.netWorthHint', { currency: data.currency })}</span>
			</div>
			<dl class="split">
				<div>
					<dt>{t('dashboard.accounts')}</dt>
					<dd>{fmtMoney(data.accountsTotal, data.currency)}</dd>
				</div>
				<div>
					<dt>{t('dashboard.portfolios')}</dt>
					<dd>{fmtMoney(data.portfoliosTotal, data.currency)}</dd>
				</div>
			</dl>
		</section>

		<div class="grid">
			<!-- Month -->
			<section class="card stack">
				<div class="section-head">
					<h2 class="cap">{t('dashboard.month', { month: monthLabel })}</h2>
					<a href="/budget" class="small">{t('dashboard.goBudget')}</a>
				</div>
				<dl class="split three">
					<div>
						<dt>{t('budget.income')}</dt>
						<dd>{fmtMoney(data.month.income, data.currency)}</dd>
					</div>
					<div>
						<dt>{t('budget.expense')}</dt>
						<dd>{fmtMoney(data.month.expense, data.currency)}</dd>
					</div>
					<div>
						<dt>{t('budget.net')}</dt>
						<dd class:pos={Number(data.month.net) > 0} class:neg={Number(data.month.net) < 0}>
							{fmtMoney(data.month.net, data.currency)}
						</dd>
					</div>
				</dl>
				<h3>{t('dashboard.spending')}</h3>
				{#if spending.some((s) => s.value > 0)}
					<CategoryBars rows={spending} currency={data.currency} otherLabel={t('dashboard.other')} />
				{:else}
					<p class="muted">{t('dashboard.noSpending')}</p>
				{/if}
			</section>

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
								<span class="status warn-text">⚠ {t('dashboard.needsRebalance')} · {fmtNumber(p.maxDeviation ?? 0)} p.p.</span>
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
		</div>
	{/if}
</div>

<style>
	p {
		margin: 0;
	}
	h3 {
		font-size: 0.95rem;
		margin: 0;
	}
	.small {
		font-size: 0.85rem;
	}
	.cap::first-letter {
		text-transform: uppercase;
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
		font-size: 2.4rem;
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
	.grid {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(min(100%, 420px), 1fr));
		gap: var(--space);
		align-items: start;
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
