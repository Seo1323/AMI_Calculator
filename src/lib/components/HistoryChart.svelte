<script lang="ts">
	// Line chart: portfolio value (recorded daily) against the money put in (from the trades). The two
	// lines differ by color and by dash, and the legend carries the latest numbers, so nothing relies on
	// color alone or on reading the plot by eye.
	import { t, fmtMoney, fmtDay } from '$lib/i18n/index.svelte';

	interface Point {
		day: string;
		value: string;
	}
	interface Props {
		values: Point[];
		invested: Point[];
		currency: string;
	}
	let { values, invested, currency }: Props = $props();

	const W = 560;
	const H = 200;
	const PAD = { l: 8, r: 8, t: 8, b: 8 };
	const ms = (d: string) => Date.parse(`${d}T00:00:00Z`);

	// The invested line is a step function: it holds its level until the next trade, and runs up to today.
	const steps = $derived.by(() => {
		const pts = invested.map((p) => ({ x: ms(p.day), y: Number(p.value) }));
		const end = Math.max(...values.map((p) => ms(p.day)), ...pts.map((p) => p.x));
		if (pts.length && pts[pts.length - 1].x < end) pts.push({ x: end, y: pts[pts.length - 1].y });
		return pts;
	});
	const line = $derived(values.map((p) => ({ x: ms(p.day), y: Number(p.value) })));
	const all = $derived([...steps, ...line]);
	const xMin = $derived(Math.min(...all.map((p) => p.x)));
	const xMax = $derived(Math.max(...all.map((p) => p.x)));
	const yLo = $derived(Math.min(...all.map((p) => p.y)));
	const yHi = $derived(Math.max(...all.map((p) => p.y)));
	const pad = $derived((yHi - yLo || Math.abs(yHi) || 1) * 0.1);
	const sx = (x: number) => PAD.l + (xMax === xMin ? 0.5 : (x - xMin) / (xMax - xMin)) * (W - PAD.l - PAD.r);
	const sy = (y: number) => H - PAD.b - ((y - (yLo - pad)) / (yHi + pad - (yLo - pad))) * (H - PAD.t - PAD.b);

	const stepPath = $derived(
		steps.map((p, i) => (i === 0 ? `M${sx(p.x)},${sy(p.y)}` : `H${sx(p.x)}V${sy(p.y)}`)).join('')
	);
	const linePath = $derived(line.map((p, i) => `${i ? 'L' : 'M'}${sx(p.x)},${sy(p.y)}`).join(''));
	const last = (a: Point[]) => a[a.length - 1];
</script>

<div class="hist">
	<svg viewBox="0 0 {W} {H}" role="img" aria-label={t('history.caption')}>
		{#if steps.length}
			<path d={stepPath} fill="none" stroke="var(--series-2)" stroke-width="2" stroke-dasharray="6 4" />
		{/if}
		{#if line.length > 1}
			<path d={linePath} fill="none" stroke="var(--series-1)" stroke-width="2.5" stroke-linejoin="round" />
		{/if}
		{#if line.length}
			<circle cx={sx(line[line.length - 1].x)} cy={sy(line[line.length - 1].y)} r="4" fill="var(--series-1)" />
		{/if}
	</svg>
	<div class="axis small muted">
		<span>{fmtDay(new Date(xMin).toISOString().slice(0, 10))}</span>
		<span>{fmtDay(new Date(xMax).toISOString().slice(0, 10))}</span>
	</div>
	<ul class="legend">
		<li>
			<span class="sw solid"></span>{t('history.value')}:
			<strong>{values.length ? fmtMoney(last(values).value, currency) : '–'}</strong>
		</li>
		{#if invested.length}
			<li>
				<span class="sw dashed"></span>{t('history.invested')}: <strong>{fmtMoney(last(invested).value, currency)}</strong>
			</li>
		{/if}
	</ul>
	{#if values.length < 2}
		<p class="muted small">{t('history.young')}</p>
	{/if}
</div>

<style>
	.hist svg {
		width: 100%;
		height: auto;
		display: block;
	}
	.axis {
		display: flex;
		justify-content: space-between;
	}
	.legend {
		display: flex;
		flex-wrap: wrap;
		gap: 0.25rem 1.25rem;
		list-style: none;
		padding: 0;
		margin: 0.5rem 0 0;
	}
	.sw {
		display: inline-block;
		width: 1.5rem;
		height: 0;
		border-top: 3px solid var(--series-1);
		margin-right: 0.4rem;
		vertical-align: middle;
	}
	.sw.dashed {
		border-top: 3px dashed var(--series-2);
	}
</style>
