<script lang="ts">
	// Donut chart of the portfolio value by asset kind. The legend is a table with the exact shares and
	// amounts, so the chart never depends on telling close slices apart by eye; it is also the text
	// alternative (screen readers and keyboard users read the table). Colors follow the kind, never the rank.
	import { t, fmtMoney, fmtNumber } from '$lib/i18n/index.svelte';
	import { layout, arcPath } from '$lib/pie';

	interface Share {
		kind: string;
		value: string;
		percent: string;
	}
	interface Props {
		shares: Share[];
		total: string;
		currency: string;
		/** Positions that have units but no value yet, so they are not in the chart. */
		unvalued?: number;
	}
	let { shares, total, currency, unvalued = 0 }: Props = $props();

	const COLOR: Record<string, string> = {
		etf: 'var(--series-1)',
		stock: 'var(--series-2)',
		bond: 'var(--series-3)',
		crypto: 'var(--series-4)',
		cash: 'var(--series-5)',
		deposit: 'var(--series-6)',
		other: 'var(--series-other)'
	};
	const SIZE = 200;
	const C = SIZE / 2;
	const OUTER = 92;
	const INNER = 58;

	let active: string | null = $state(null);

	const slices = $derived(layout(shares.map((s) => ({ id: s.kind, value: Number(s.value) }))));
	const byKind = $derived(Object.fromEntries(shares.map((s) => [s.kind, s])));
	const pct = (s: Share) => `${fmtNumber(Number(s.percent), 1)}%`;
	const name = (kind: string) => t(`kind.${kind}` as Parameters<typeof t>[0]);
	const describe = (s: Share) => `${name(s.kind)}: ${pct(s)}, ${fmtMoney(s.value, currency)}`;
	const focused = $derived(active ? byKind[active] : undefined);
</script>

{#if slices.length === 0}
	<p class="muted">{t('pf.allocation.empty')}</p>
{:else}
	<div class="pie-wrap">
		<svg class="pie" viewBox="0 0 {SIZE} {SIZE}" role="group" aria-label={t('pf.allocation.caption')}>
			{#each slices as s (s.id)}
				{#if slices.length === 1}
					<!-- One slice is the whole ring: a plain circle, so no seam is drawn where two halves would meet. -->
					<circle
						cx={C}
						cy={C}
						r={(OUTER + INNER) / 2}
						fill="none"
						stroke={COLOR[s.id] ?? 'var(--series-other)'}
						stroke-width={OUTER - INNER}
						class="whole"
						role="img"
						aria-label={describe(byKind[s.id])}
					/>
				{:else}
					<path
						d={arcPath(C, C, OUTER, INNER, s.start, s.end)}
						fill={COLOR[s.id] ?? 'var(--series-other)'}
						class="slice"
						class:dim={active !== null && active !== s.id}
						role="img"
						aria-label={describe(byKind[s.id])}
						onpointerenter={() => (active = s.id)}
						onpointerleave={() => (active = null)}
					></path>
				{/if}
			{/each}
			<!-- Readout in the hole: the total, or the slice under the pointer -->
			<g class="readout" pointer-events="none">
				{#if focused}
					<text x={C} y={C - 12} class="r-label">{name(focused.kind)}</text>
					<text x={C} y={C + 10} class="r-big">{pct(focused)}</text>
					<text x={C} y={C + 28} class="r-sub">{fmtMoney(focused.value, currency)}</text>
				{:else}
					<text x={C} y={C - 6} class="r-label">{t('pf.allocation.total')}</text>
					<text x={C} y={C + 16} class="r-total">{fmtMoney(total, currency)}</text>
				{/if}
			</g>
		</svg>

		<table class="legend">
			<caption class="sr-only">{t('pf.allocation.caption')}</caption>
			<thead>
				<tr>
					<th scope="col">{t('instruments.kind')}</th>
					<th scope="col" class="num">{t('pf.allocation.share')}</th>
					<th scope="col" class="num">{t('pf.value')}</th>
				</tr>
			</thead>
			<tbody>
				{#each shares as s (s.kind)}
					<tr
						class:on={active === s.kind}
						onpointerenter={() => (active = s.kind)}
						onpointerleave={() => (active = null)}
					>
						<th scope="row">
							<span class="swatch" style:background={COLOR[s.kind] ?? 'var(--series-other)'}></span>{name(s.kind)}
						</th>
						<td class="num">{pct(s)}</td>
						<td class="num">{fmtMoney(s.value, currency)}</td>
					</tr>
				{/each}
			</tbody>
		</table>
	</div>
	{#if unvalued > 0}
		<p class="muted small">{t('pf.allocation.unvalued', { n: unvalued })}</p>
	{/if}
{/if}

<style>
	.pie-wrap {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		/* When the legend wraps under the chart, the chart sits in the middle. */
		justify-content: center;
		gap: 1.5rem 2rem;
	}
	.pie {
		width: min(15rem, 100%);
		flex: 0 0 auto;
	}
	.slice {
		/* A surface-colored edge is the 2px gap between neighbouring slices. */
		stroke: var(--surface);
		stroke-width: 2;
		stroke-linejoin: round;
		cursor: pointer;
		transition: opacity 0.15s;
	}
	.slice.dim {
		opacity: 0.4;
	}
	.readout text {
		text-anchor: middle;
		fill: var(--text);
	}
	.r-label {
		font-size: 0.6rem;
		fill: var(--text-muted);
	}
	.r-total {
		font-size: 0.78rem;
		font-weight: 700;
	}
	.r-big {
		font-size: 1.35rem;
		font-weight: 700;
	}
	.r-sub {
		font-size: 0.7rem;
		fill: var(--text-muted);
	}
	.legend {
		flex: 1 1 14rem;
		border-collapse: collapse;
		min-width: 0;
	}
	.legend th,
	.legend td {
		padding: 0.4rem 0.5rem;
		text-align: left;
		font-weight: 400;
		border-bottom: 1px solid var(--border);
	}
	.legend thead th {
		font-size: 0.75rem;
		color: var(--text-muted);
		text-transform: uppercase;
		letter-spacing: 0.03em;
	}
	/* Row headers are the kind names: plain text, not the small uppercase style of column headers. */
	.legend tbody th {
		text-transform: none;
		letter-spacing: normal;
		font-size: inherit;
		color: var(--text);
	}
	.legend .num {
		text-align: right;
		white-space: nowrap;
	}
	.legend tr.on {
		background: var(--accent-soft);
	}
	.swatch {
		display: inline-block;
		width: 0.7rem;
		height: 0.7rem;
		border-radius: 50%;
		margin-right: 0.5rem;
	}
	.sr-only {
		position: absolute;
		width: 1px;
		height: 1px;
		overflow: hidden;
		clip: rect(0 0 0 0);
		white-space: nowrap;
	}
</style>
