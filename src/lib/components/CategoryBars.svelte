<script lang="ts">
	import { fmtMoney } from '$lib/i18n/index.svelte';

	interface Row {
		id: string;
		label: string;
		value: number;
		limit: number | null;
	}

	let { rows, currency, max = 6, otherLabel }: { rows: Row[]; currency: string; max?: number; otherLabel: string } = $props();

	// Top N by value; the rest folds into one "other" row.
	const shown = $derived.by(() => {
		const sorted = rows.filter((r) => r.value > 0).sort((a, b) => b.value - a.value);
		if (sorted.length <= max) return sorted;
		const rest = sorted.slice(max - 1);
		return [
			...sorted.slice(0, max - 1),
			{ id: '__other', label: otherLabel, value: rest.reduce((s, r) => s + r.value, 0), limit: null }
		];
	});
	const scale = $derived(Math.max(1, ...shown.map((r) => Math.max(r.value, r.limit ?? 0))));
</script>

<div class="bars">
	{#each shown as r (r.id)}
		{@const over = r.limit !== null && r.value > r.limit}
		<div class="row" title={`${r.label}: ${fmtMoney(r.value, currency)}${r.limit !== null ? ` / ${fmtMoney(r.limit, currency)}` : ''}`}>
			<span class="label">{r.label}</span>
			<div class="track">
				<div class="fill" class:over style:width={`${(r.value / scale) * 100}%`}></div>
				{#if r.limit !== null}
					<div class="limit" style:left={`${(r.limit / scale) * 100}%`}></div>
				{/if}
			</div>
			<span class="val" class:neg={over}>{fmtMoney(r.value, currency)}</span>
		</div>
	{/each}
</div>

<style>
	.bars {
		display: flex;
		flex-direction: column;
		gap: 0.45rem;
	}
	.row {
		display: grid;
		grid-template-columns: minmax(6rem, 9rem) 1fr auto;
		align-items: center;
		gap: 0.6rem;
		font-size: 0.85rem;
	}
	.label {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.track {
		position: relative;
		height: 10px;
	}
	.fill {
		height: 100%;
		background: var(--series-1);
		border-radius: 0 4px 4px 0;
		min-width: 2px;
	}
	.fill.over {
		background: var(--negative);
	}
	.limit {
		position: absolute;
		top: -3px;
		bottom: -3px;
		width: 2px;
		background: var(--text-muted);
		border-radius: 1px;
	}
	.val {
		font-variant-numeric: tabular-nums;
		min-width: 6.5rem;
		text-align: right;
	}
</style>
