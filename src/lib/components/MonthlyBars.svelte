<script lang="ts">
	import { fmtMoney, numberLocale } from '$lib/i18n/index.svelte';

	interface Point {
		month: string;
		income: number;
		expense: number;
		net: number;
	}

	let {
		points,
		currency,
		labels
	}: { points: Point[]; currency: string; labels: { income: string; expense: string; net: string } } = $props();

	let width = $state(600);
	const height = 240;
	const pad = { top: 12, right: 8, bottom: 26, left: 64 };
	let hovered: number | null = $state(null);

	const max = $derived(Math.max(1, ...points.flatMap((p) => [p.income, p.expense])));
	// "Nice" axis maximum and 4 gridlines.
	const niceMax = $derived.by(() => {
		const raw = max / 4;
		const mag = 10 ** Math.floor(Math.log10(raw));
		const step = [1, 2, 2.5, 5, 10].map((m) => m * mag).find((s) => s >= raw) ?? raw;
		return step * 4;
	});
	const ticks = $derived([0, 1, 2, 3, 4].map((i) => (niceMax / 4) * i));
	const plotW = $derived(Math.max(1, width - pad.left - pad.right));
	const plotH = height - pad.top - pad.bottom;
	const slot = $derived(plotW / Math.max(1, points.length));
	const barW = $derived(Math.max(3, Math.min(22, (slot - 10) / 2)));
	const y = (v: number) => pad.top + plotH - (v / niceMax) * plotH;
	const monthFmt = $derived(new Intl.DateTimeFormat(numberLocale(), { month: 'short' }));
	const monthLong = $derived(new Intl.DateTimeFormat(numberLocale(), { month: 'long', year: 'numeric' }));
	const date = (m: string) => new Date(m + 'T00:00:00');
	const compact = $derived(new Intl.NumberFormat(numberLocale(), { notation: 'compact', maximumFractionDigits: 1 }));

	/** Bar path with 4px rounded top corners, anchored to the baseline. */
	function bar(x: number, v: number) {
		const top = y(v);
		const base = y(0);
		const h = base - top;
		if (h <= 0) return '';
		const r = Math.min(4, h, barW / 2);
		return `M${x},${base} V${top + r} Q${x},${top} ${x + r},${top} H${x + barW - r} Q${x + barW},${top} ${x + barW},${top + r} V${base} Z`;
	}
</script>

<div class="chart" bind:clientWidth={width}>
	<ul class="legend">
		<li><span class="sw" style:background="var(--series-1)"></span>{labels.income}</li>
		<li><span class="sw" style:background="var(--series-2)"></span>{labels.expense}</li>
	</ul>
	<svg {width} {height} role="img" aria-label={`${labels.income} / ${labels.expense}`}>
		{#each ticks as tk (tk)}
			<line class="grid" x1={pad.left} x2={width - pad.right} y1={y(tk)} y2={y(tk)} />
			<text class="axis" x={pad.left - 8} y={y(tk) + 4} text-anchor="end">{compact.format(tk)}</text>
		{/each}
		{#each points as p, i (p.month)}
			{@const cx = pad.left + slot * i + slot / 2}
			<g class:dim={hovered !== null && hovered !== i}>
				<path d={bar(cx - barW - 1, p.income)} fill="var(--series-1)" />
				<path d={bar(cx + 1, p.expense)} fill="var(--series-2)" />
			</g>
			{#if points.length <= 18 || i % Math.ceil(points.length / 12) === 0}
				<text class="axis" x={cx} y={height - 8} text-anchor="middle">{monthFmt.format(date(p.month))}</text>
			{/if}
			<!-- Hit target wider than the marks -->
			<rect
				x={pad.left + slot * i}
				y={pad.top}
				width={slot}
				height={plotH}
				fill="transparent"
				role="presentation"
				onmouseenter={() => (hovered = i)}
				onmouseleave={() => (hovered = null)}
			/>
		{/each}
		<line class="baseline" x1={pad.left} x2={width - pad.right} y1={y(0)} y2={y(0)} />
	</svg>
	{#if hovered !== null}
		{@const p = points[hovered]}
		{@const left = Math.min(width - 190, Math.max(0, pad.left + slot * hovered + slot / 2 - 95))}
		<div class="tip" style:left={`${left}px`}>
			<strong>{monthLong.format(date(p.month))}</strong>
			<div><span class="sw" style:background="var(--series-1)"></span>{labels.income}<span>{fmtMoney(p.income, currency)}</span></div>
			<div><span class="sw" style:background="var(--series-2)"></span>{labels.expense}<span>{fmtMoney(p.expense, currency)}</span></div>
			<div class="net">{labels.net}<span>{fmtMoney(p.net, currency)}</span></div>
		</div>
	{/if}
</div>

<style>
	.chart {
		position: relative;
		width: 100%;
	}
	svg {
		display: block;
		overflow: visible;
	}
	.grid {
		stroke: var(--border);
		stroke-width: 1;
	}
	.baseline {
		stroke: var(--text-muted);
		stroke-width: 1;
	}
	.axis {
		fill: var(--text-muted);
		font-size: 11px;
		font-variant-numeric: tabular-nums;
	}
	g {
		transition: opacity 0.15s;
	}
	.dim {
		opacity: 0.35;
	}
	.legend {
		list-style: none;
		display: flex;
		gap: 1rem;
		margin: 0 0 0.5rem;
		padding: 0;
		font-size: 0.85rem;
	}
	.legend li,
	.tip div {
		display: flex;
		align-items: center;
		gap: 0.4rem;
	}
	.sw {
		width: 10px;
		height: 10px;
		border-radius: 3px;
		flex: none;
	}
	.tip {
		position: absolute;
		top: 2rem;
		width: 190px;
		padding: 0.5rem 0.65rem;
		background: var(--surface);
		border: 1px solid var(--border);
		border-radius: calc(var(--radius) * 0.7);
		box-shadow: var(--shadow);
		font-size: 0.8rem;
		pointer-events: none;
		display: flex;
		flex-direction: column;
		gap: 0.2rem;
	}
	.tip strong {
		text-transform: capitalize;
	}
	.tip span:last-child {
		margin-left: auto;
		font-variant-numeric: tabular-nums;
	}
	.tip .net {
		border-top: 1px solid var(--border);
		padding-top: 0.2rem;
		font-weight: 600;
	}
</style>
