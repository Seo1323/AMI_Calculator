<script lang="ts">
	// Target allocation as linked sliders: moving one redistributes the rest so the total stays 100 %.
	// A lock keeps a row where it is. The maths lives in $lib/targets (pure, tested).
	import Icon from '$lib/components/Icon.svelte';
	import { t, fmtNumber } from '$lib/i18n/index.svelte';
	import {
		WHOLE,
		type TargetRow,
		fromUnits,
		toUnits,
		total,
		setUnits,
		addRow,
		removeRow,
		toggleLock,
		equalSplit
	} from '$lib/targets';

	interface Option {
		id: string;
		label: string;
	}
	interface Props {
		rows: TargetRow[];
		/** Everything that can be given a target; rows are picked from here. */
		options: Option[];
		onchange?: () => void;
	}
	let { rows = $bindable(), options, onchange }: Props = $props();

	let pick = $state('');
	const label = (id: string) => options.find((o) => o.id === id)?.label ?? id;
	const available = $derived(options.filter((o) => !rows.some((r) => r.id === o.id)));
	const sum = $derived(total(rows));
	const color = (i: number) => (i < 8 ? `var(--series-${i + 1})` : 'var(--series-other)');
	const show = (units: number) => String(fromUnits(units));

	function apply(next: TargetRow[]) {
		rows = next;
		onchange?.();
	}

	function slide(e: Event & { currentTarget: HTMLInputElement }, id: string) {
		const next = setUnits(rows, id, Math.round(Number(e.currentTarget.value) * 100));
		// Show what was actually applied (the value can be limited by locked rows).
		e.currentTarget.value = show(next.find((r) => r.id === id)?.units ?? 0);
		apply(next);
	}

	function typed(e: Event & { currentTarget: HTMLInputElement }, id: string) {
		const units = toUnits(e.currentTarget.value);
		const next = units === null ? rows : setUnits(rows, id, units);
		e.currentTarget.value = show(next.find((r) => r.id === id)?.units ?? 0);
		if (next !== rows) apply(next);
	}
</script>

<div class="targets stack">
	{#if rows.length === 0}
		<p class="muted small">{t('targets.empty')}</p>
	{:else}
		<div class="bar" role="img" aria-label={rows.map((r) => `${label(r.id)} ${fmtNumber(fromUnits(r.units))}%`).join(', ')}>
			{#each rows as r, i (r.id)}
				<span style:width="{r.units / 100}%" style:background={color(i)}></span>
			{/each}
		</div>
		{#each rows as r, i (r.id)}
			<div class="trow">
				<span class="dot" style:background={color(i)}></span>
				<span class="name">{label(r.id)}</span>
				<input
					class="slider"
					type="range"
					min="0"
					max="100"
					step="0.5"
					value={show(r.units)}
					disabled={r.locked}
					aria-label="{label(r.id)}: {t('pf.target')}"
					aria-valuetext="{fmtNumber(fromUnits(r.units))}%"
					oninput={(e) => slide(e, r.id)}
				/>
				<input
					class="short n"
					inputmode="decimal"
					value={show(r.units)}
					disabled={r.locked}
					aria-label="{label(r.id)}: {t('pf.target')} (%)"
					onchange={(e) => typed(e, r.id)}
				/>
				<span>%</span>
				<button
					class="btn ghost"
					type="button"
					aria-pressed={r.locked}
					aria-label={r.locked ? t('targets.unlock') : t('targets.lock')}
					title={r.locked ? t('targets.unlock') : t('targets.lock')}
					onclick={() => apply(toggleLock(rows, r.id))}><Icon name={r.locked ? 'lock' : 'unlock'} size={16} /></button
				>
				<button
					class="btn ghost"
					type="button"
					aria-label={t('common.remove')}
					title={t('common.remove')}
					onclick={() => apply(removeRow(rows, r.id))}><Icon name="trash" size={16} /></button
				>
			</div>
		{/each}
		<div class="total" class:ok={sum === WHOLE} class:neg={sum !== WHOLE}>
			{t('targets.total', { sum: fmtNumber(fromUnits(sum)) })}
			{#if rows.length >= 2}
				<button class="btn ghost small" type="button" onclick={() => apply(equalSplit(rows))}>{t('targets.equal')}</button>
			{/if}
		</div>
		<p class="muted small">{t('targets.hint')}</p>
	{/if}
	{#if available.length}
		<div class="row">
			<select bind:value={pick} aria-label={t('pf.addToTargets')}>
				<option value="">{t('pf.addToTargets')}</option>
				{#each available as o (o.id)}<option value={o.id}>{o.label}</option>{/each}
			</select>
			<button
				class="btn"
				type="button"
				disabled={!pick}
				onclick={() => {
					apply(addRow(rows, pick));
					pick = '';
				}}><Icon name="plus" size={16} />{t('common.add')}</button
			>
		</div>
	{/if}
</div>

<style>
	.bar {
		display: flex;
		height: 0.6rem;
		gap: 2px;
		border-radius: 999px;
		overflow: hidden;
	}
	.bar span {
		min-width: 2px;
		transition: width 0.1s;
	}
	.trow {
		display: grid;
		grid-template-columns: auto minmax(5rem, 11rem) 1fr 4.5rem auto auto auto;
		align-items: center;
		gap: 0.5rem;
	}
	.dot {
		width: 0.7rem;
		height: 0.7rem;
		border-radius: 50%;
	}
	.name {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.slider {
		width: 100%;
		min-width: 6rem;
	}
	.n {
		text-align: right;
	}
	.total {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		font-weight: 600;
	}
	.total.neg {
		color: var(--negative);
	}
	@media (max-width: 640px) {
		.trow {
			grid-template-columns: auto 1fr auto auto auto;
		}
		.slider {
			grid-column: 1 / -1;
			order: 5;
		}
	}
</style>
