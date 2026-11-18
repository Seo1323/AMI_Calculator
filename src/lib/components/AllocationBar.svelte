<script lang="ts">
	import { t, fmtNumber } from '$lib/i18n/index.svelte';

	interface Slice {
		id: string;
		label: string;
		current: number;
		target: number;
	}

	let { slices, colorOf }: { slices: Slice[]; colorOf?: (id: string) => number } = $props();

	// Color follows the entity: callers pass one stable slot per instrument so the same
	// instrument has the same color everywhere; slots past 8 fold to "other".
	const slot = (s: Slice, i: number) => (colorOf ? colorOf(s.id) : i);
	const color = (n: number) => (n < 8 ? `var(--series-${n + 1})` : 'var(--series-other)');
	let hovered: string | null = $state(null);

	const rows = $derived([
		{ key: 'current', label: t('rebalance.current'), value: (s: Slice) => s.current },
		{ key: 'target', label: t('pf.target'), value: (s: Slice) => s.target }
	]);
</script>

<div class="alloc" role="figure" aria-label={t('dashboard.allocation')}>
	{#each rows as row (row.key)}
		<div class="bar-row">
			<span class="bar-label muted">{row.label}</span>
			<div class="bar">
				{#each slices as s, i (s.id)}
					{@const v = row.value(s)}
					{#if v > 0}
						<div
							class="seg"
							class:dim={hovered !== null && hovered !== s.id}
							style:width={`${v}%`}
							style:background={color(slot(s, i))}
							title={`${s.label}: ${fmtNumber(v)}%`}
							role="presentation"
							onmouseenter={() => (hovered = s.id)}
							onmouseleave={() => (hovered = null)}
						></div>
					{/if}
				{/each}
			</div>
		</div>
	{/each}
	<ul class="legend">
		{#each slices as s, i (s.id)}
			<li
				class:dim={hovered !== null && hovered !== s.id}
				onmouseenter={() => (hovered = s.id)}
				onmouseleave={() => (hovered = null)}
			>
				<span class="sw" style:background={color(slot(s, i))}></span>
				<span class="name">{s.label}</span>
				<span class="val">{fmtNumber(s.current)}% → {fmtNumber(s.target)}%</span>
			</li>
		{/each}
	</ul>
</div>

<style>
	.alloc {
		display: flex;
		flex-direction: column;
		gap: 0.4rem;
	}
	.bar-row {
		display: grid;
		grid-template-columns: 4.5rem 1fr;
		align-items: center;
		gap: 0.5rem;
		font-size: 0.8rem;
	}
	.bar {
		display: flex;
		gap: 2px; /* surface gap between fills */
		height: 12px;
	}
	.seg {
		height: 100%;
		min-width: 2px;
		transition: opacity 0.15s;
	}
	.seg:first-child {
		border-radius: 4px 0 0 4px;
	}
	.seg:last-child {
		border-radius: 0 4px 4px 0;
	}
	.seg:only-child {
		border-radius: 4px;
	}
	.dim {
		opacity: 0.35;
	}
	.legend {
		list-style: none;
		margin: 0.2rem 0 0;
		padding: 0;
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(14rem, 1fr));
		gap: 0.15rem 1rem;
		font-size: 0.8rem;
	}
	.legend li {
		display: flex;
		align-items: center;
		gap: 0.4rem;
		transition: opacity 0.15s;
	}
	.sw {
		width: 10px;
		height: 10px;
		border-radius: 3px;
		flex: none;
	}
	.name {
		color: var(--text);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.val {
		margin-left: auto;
		color: var(--text-muted);
		font-variant-numeric: tabular-nums;
		white-space: nowrap;
	}
</style>
