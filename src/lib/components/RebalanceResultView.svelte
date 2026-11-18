<script lang="ts">
	import { t, fmtMoney, fmtNumber } from '$lib/i18n/index.svelte';
	import type { RebalanceResult } from '$lib/tauri';

	let {
		result,
		currency,
		label = (id: string) => id
	}: { result: RebalanceResult; currency?: string; label?: (id: string) => string } = $props();

	function action(units: string) {
		const n = Number(units);
		if (n > 0) return { text: t('rebalance.buy', { units: fmtNumber(n, 8) }), cls: 'pos' };
		if (n < 0) return { text: t('rebalance.sell', { units: fmtNumber(-n, 8) }), cls: 'neg' };
		return { text: t('rebalance.hold'), cls: 'muted' };
	}
</script>

<section class="card stack">
	<h2>{t('rebalance.result')}</h2>
	<div class="notice" class:warn={result.needsRebalance}>
		{t(result.needsRebalance ? 'rebalance.needs' : 'rebalance.ok', { dev: fmtNumber(result.maxDeviationBefore) })}
	</div>
	<div class="table-wrap">
		<table>
			<thead>
				<tr>
					<th>{t('rebalance.name')}</th>
					<th class="num">{t('rebalance.current')}</th>
					<th class="num">%</th>
					<th class="num">{t('rebalance.target')}</th>
					<th class="num">{t('rebalance.deviation')}</th>
					<th>{t('rebalance.action')}</th>
					<th class="num">{t('rebalance.after')}</th>
					<th class="num">{t('rebalance.after')} %</th>
				</tr>
			</thead>
			<tbody>
				{#each result.positions as p, i (i)}
					{@const a = action(p.unitsToTrade)}
					<tr>
						<td>{label(p.id)}</td>
						<td class="num">{fmtMoney(p.currentValue, currency)}</td>
						<td class="num">{fmtNumber(p.currentPercent)}</td>
						<td class="num">{fmtNumber(p.targetPercent)}</td>
						<td class="num">{fmtNumber(p.deviationBefore)}</td>
						<td class={a.cls}>
							{a.text} <span class="muted">({fmtMoney(Math.abs(Number(p.tradeValue)), currency)})</span>
						</td>
						<td class="num">{fmtMoney(p.valueAfter, currency)}</td>
						<td class="num">{fmtNumber(p.percentAfter)}</td>
					</tr>
				{/each}
			</tbody>
		</table>
	</div>
	<dl class="totals">
		<div><dt>{t('rebalance.totalBefore')}</dt><dd>{fmtMoney(result.totalBefore, currency)}</dd></div>
		<div><dt>{t('rebalance.totalAfter')}</dt><dd>{fmtMoney(result.totalAfter, currency)}</dd></div>
		<div><dt>{t('rebalance.cashLeft')}</dt><dd>{fmtMoney(result.cashLeft, currency)}</dd></div>
	</dl>
</section>

<style>
	.totals {
		display: flex;
		flex-wrap: wrap;
		gap: calc(var(--space) * 2);
		margin: 0;
	}
	.totals dt {
		font-size: 0.8rem;
		color: var(--text-muted);
	}
	.totals dd {
		margin: 0;
		font-size: 1.15rem;
		font-weight: 600;
		font-variant-numeric: tabular-nums;
	}
</style>
