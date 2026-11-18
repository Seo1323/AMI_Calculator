<script lang="ts">
	import { save } from '@tauri-apps/plugin-dialog';
	import { t, fmtMoney, fmtNumber, numberLocale, errorText } from '$lib/i18n/index.svelte';
	import { settings } from '$lib/settings.svelte';
	import { invoke, isTauri } from '$lib/tauri';
	import MonthlyBars from '$lib/components/MonthlyBars.svelte';
	import CategoryBars from '$lib/components/CategoryBars.svelte';
	import { monthKey, type PeriodReport } from '$lib/types';

	type Preset = 'last3' | 'last6' | 'last12' | 'thisYear' | 'custom';
	let preset: Preset = $state('last6');
	const now = new Date();
	let fromMonth = $state(monthKey(now, -5).slice(0, 7));
	let toMonth = $state(monthKey(now).slice(0, 7));
	let report = $state<PeriodReport | null>(null);
	let showTable = $state(false);
	let error = $state('');
	let info = $state('');

	function applyPreset(p: Preset) {
		preset = p;
		const back = { last3: -2, last6: -5, last12: -11 } as const;
		if (p === 'thisYear') fromMonth = `${now.getFullYear()}-01`;
		else if (p !== 'custom') fromMonth = monthKey(now, back[p]).slice(0, 7);
		if (p !== 'custom') toMonth = monthKey(now).slice(0, 7);
	}

	$effect(() => {
		const from = `${fromMonth}-01`;
		const to = `${toMonth}-01`;
		const currency = settings.baseCurrency;
		if (!isTauri() || !fromMonth || !toMonth) return;
		invoke<PeriodReport>('budget_report', { from, to, currency })
			.then((r) => {
				report = r;
				error = '';
			})
			.catch((e) => (error = errorText(e)));
	});

	const months = $derived(report?.months.length ?? 1);
	const points = $derived(
		(report?.months ?? []).map((m) => ({ month: m.month, income: Number(m.income), expense: Number(m.expense), net: Number(m.net) }))
	);
	const expenseLines = $derived((report?.categories ?? []).filter((c) => c.category.kind === 'expense' && Number(c.amount) > 0));
	const incomeLines = $derived((report?.categories ?? []).filter((c) => c.category.kind === 'income' && Number(c.amount) > 0));
	const hasData = $derived(Number(report?.income ?? 0) > 0 || Number(report?.expense ?? 0) > 0);
	const monthLong = $derived(new Intl.DateTimeFormat(numberLocale(), { month: 'long', year: 'numeric' }));

	async function exportCsv() {
		if (!report) return;
		error = info = '';
		try {
			const path = await save({ defaultPath: `ami-raport-${fromMonth}_${toMonth}.csv`, filters: [{ name: 'CSV', extensions: ['csv'] }] });
			if (!path) return;
			const q = (s: string) => `"${s.replaceAll('"', '""')}"`;
			const rows = [
				[t('reports.month'), t('reports.income'), t('reports.expense'), t('reports.net')].map(q).join(';'),
				...report.months.map((m) => [m.month.slice(0, 7), m.income, m.expense, m.net].join(';')),
				'',
				[t('reports.category'), t('reports.total'), t('reports.limits')].map(q).join(';'),
				...report.categories.map((c) => [q(c.category.name), c.amount, c.limit ?? ''].join(';'))
			];
			// BOM so Excel opens Polish characters correctly; ";" works with comma-decimal locales.
			await invoke('save_text_file', { path, contents: '﻿' + rows.join('\r\n') });
			info = t('reports.exported', { path });
		} catch (e) {
			error = errorText(e);
		}
	}
</script>

<div class="stack">
	<header class="head">
		<h1>{t('reports.title')}</h1>
		<button class="btn" disabled={!report} onclick={exportCsv}>{t('reports.exportCsv')}</button>
	</header>

	{#if error}<div class="notice error">{error}</div>{/if}
	{#if info}<div class="notice">{info}</div>{/if}

	<section class="card row filters">
		<div class="segmented" role="radiogroup" aria-label={t('reports.period')}>
			{#each ['last3', 'last6', 'last12', 'thisYear', 'custom'] as const as p (p)}
				<button role="radio" aria-checked={preset === p} class:on={preset === p} onclick={() => applyPreset(p)}>
					{t(`reports.${p}`)}
				</button>
			{/each}
		</div>
		{#if preset === 'custom'}
			<label class="field">
				{t('reports.from')}
				<input type="month" bind:value={fromMonth} />
			</label>
			<label class="field">
				{t('reports.to')}
				<input type="month" bind:value={toMonth} />
			</label>
		{/if}
	</section>

	{#if report?.missingRates.length}
		<div class="notice warn">{t('budget.missingRates', { list: report.missingRates.join(', ') })}</div>
	{/if}

	{#if report}
		<div class="kpis">
			<section class="card"><span class="muted">{t('reports.income')}</span><strong>{fmtMoney(report.income, report.currency)}</strong></section>
			<section class="card"><span class="muted">{t('reports.expense')}</span><strong>{fmtMoney(report.expense, report.currency)}</strong></section>
			<section class="card">
				<span class="muted">{t('reports.net')}</span>
				<strong class:pos={Number(report.net) > 0} class:neg={Number(report.net) < 0}>{fmtMoney(report.net, report.currency)}</strong>
			</section>
			<section class="card">
				<span class="muted">{t('reports.savingsRate')}</span>
				<strong>{report.savingsRate === null ? '—' : `${fmtNumber(report.savingsRate, 1)}%`}</strong>
			</section>
			<section class="card"><span class="muted">{t('reports.avgExpense')}</span><strong>{fmtMoney(report.avgExpense, report.currency)}</strong></section>
		</div>

		{#if !hasData}
			<p class="muted">{t('reports.empty')}</p>
		{:else}
			<section class="card stack">
				<div class="section-head">
					<h2>{t('reports.chart')}</h2>
					<button class="btn ghost" onclick={() => (showTable = !showTable)}>
						{showTable ? t('reports.hideTable') : t('reports.showTable')}
					</button>
				</div>
				<MonthlyBars
					{points}
					currency={report.currency}
					labels={{ income: t('reports.income'), expense: t('reports.expense'), net: t('reports.net') }}
				/>
				{#if showTable}
					<div class="table-wrap">
						<table>
							<thead>
								<tr>
									<th>{t('reports.month')}</th>
									<th class="num">{t('reports.income')}</th>
									<th class="num">{t('reports.expense')}</th>
									<th class="num">{t('reports.net')}</th>
								</tr>
							</thead>
							<tbody>
								{#each report.months as m (m.month)}
									<tr>
										<td class="cap">{monthLong.format(new Date(m.month + 'T00:00:00'))}</td>
										<td class="num">{fmtMoney(m.income, report.currency)}</td>
										<td class="num">{fmtMoney(m.expense, report.currency)}</td>
										<td class="num" class:pos={Number(m.net) > 0} class:neg={Number(m.net) < 0}>{fmtMoney(m.net, report.currency)}</td>
									</tr>
								{/each}
							</tbody>
						</table>
					</div>
				{/if}
			</section>

			<div class="grid">
				<section class="card stack">
					<h2>{t('reports.categories')}</h2>
					<CategoryBars
						rows={expenseLines.map((c) => ({ id: c.category.id, label: c.category.name, value: Number(c.amount), limit: null }))}
						currency={report.currency}
						max={10}
						otherLabel={t('dashboard.other')}
					/>
					<div class="table-wrap">
						<table>
							<thead>
								<tr>
									<th>{t('reports.category')}</th>
									<th class="num">{t('reports.total')}</th>
									<th class="num">{t('reports.perMonth')}</th>
									<th class="num">{t('reports.share')}</th>
								</tr>
							</thead>
							<tbody>
								{#each expenseLines as c (c.category.id)}
									<tr>
										<td>{c.category.name}</td>
										<td class="num">{fmtMoney(c.amount, report.currency)}</td>
										<td class="num">{fmtMoney(Number(c.amount) / months, report.currency)}</td>
										<td class="num">{fmtNumber((Number(c.amount) / Math.max(1, Number(report.expense))) * 100, 1)}%</td>
									</tr>
								{/each}
								{#if Number(report.uncategorizedExpense) > 0}
									<tr class="muted">
										<td>{t('budget.uncategorized')}</td>
										<td class="num">{fmtMoney(report.uncategorizedExpense, report.currency)}</td>
										<td class="num">{fmtMoney(Number(report.uncategorizedExpense) / months, report.currency)}</td>
										<td class="num">{fmtNumber((Number(report.uncategorizedExpense) / Math.max(1, Number(report.expense))) * 100, 1)}%</td>
									</tr>
								{/if}
							</tbody>
						</table>
					</div>
				</section>

				{#if incomeLines.length || Number(report.uncategorizedIncome) > 0}
					<section class="card stack">
						<h2>{t('reports.incomeCategories')}</h2>
						<div class="table-wrap">
							<table>
								<tbody>
									{#each incomeLines as c (c.category.id)}
										<tr>
											<td>{c.category.name}</td>
											<td class="num">{fmtMoney(c.amount, report.currency)}</td>
										</tr>
									{/each}
									{#if Number(report.uncategorizedIncome) > 0}
										<tr class="muted">
											<td>{t('budget.uncategorized')}</td>
											<td class="num">{fmtMoney(report.uncategorizedIncome, report.currency)}</td>
										</tr>
									{/if}
								</tbody>
							</table>
						</div>
					</section>
				{/if}
			</div>
		{/if}
	{/if}
</div>

<style>
	p {
		margin: 0;
	}
	.head,
	.section-head {
		display: flex;
		flex-wrap: wrap;
		justify-content: space-between;
		align-items: center;
		gap: var(--space);
	}
	.filters {
		align-items: end;
	}
	.segmented {
		display: inline-flex;
		flex-wrap: wrap;
		border: 1px solid var(--border);
		border-radius: calc(var(--radius) * 0.7);
		overflow: hidden;
		width: fit-content;
	}
	.segmented button {
		border: none;
		background: var(--surface-2);
		padding: calc(var(--space) * 0.4) calc(var(--space) * 0.9);
		cursor: pointer;
		color: var(--text);
	}
	.segmented button.on {
		background: var(--accent);
		color: white;
	}
	.kpis {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(170px, 1fr));
		gap: var(--space);
	}
	.kpis .card {
		display: flex;
		flex-direction: column;
		gap: 0.2rem;
	}
	.kpis span {
		font-size: 0.85rem;
	}
	.kpis strong {
		font-size: 1.3rem;
		font-variant-numeric: tabular-nums;
	}
	.grid {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(min(100%, 420px), 1fr));
		gap: var(--space);
		align-items: start;
	}
	.cap {
		text-transform: capitalize;
	}
</style>
