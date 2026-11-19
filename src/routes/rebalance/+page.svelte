<script lang="ts">
	import { t, fmtNumber, errorText } from '$lib/i18n/index.svelte';
	import { settings } from '$lib/settings.svelte';
	import { invoke, type Mode, type RebalanceResult } from '$lib/tauri';
	import Icon from '$lib/components/Icon.svelte';
	import RebalanceResultView from '$lib/components/RebalanceResultView.svelte';

	interface Row {
		key: number;
		id: string;
		quantity: string;
		price: string;
		targetPercent: string;
		fractional: boolean;
	}

	const DRAFT_KEY = 'ami.rebalance.draft.v1';
	let nextKey = 1;
	const row = (id = '', quantity = '0', price = '', targetPercent = '', fractional = false): Row => ({
		key: nextKey++,
		id,
		quantity,
		price,
		targetPercent,
		fractional
	});

	function loadDraft() {
		try {
			const d = JSON.parse(localStorage.getItem(DRAFT_KEY) ?? 'null');
			if (d?.rows?.length) return d;
		} catch {
			/* ignore */
		}
		return {
			rows: [
				{ id: 'ETF akcje świat', quantity: '10', price: '120', targetPercent: '60', fractional: false },
				{ id: 'Obligacje', quantity: '30', price: '100', targetPercent: '30', fractional: false },
				{ id: 'BTC', quantity: '0', price: '400000', targetPercent: '10', fractional: true }
			],
			mode: 'buyOnly',
			contribution: '1000',
			tolerance: '5'
		};
	}

	const draft = loadDraft();
	let rows: Row[] = $state(
		draft.rows.map((r: Omit<Row, 'key'>) => row(r.id, r.quantity, r.price, r.targetPercent, r.fractional))
	);
	let mode: Mode = $state(draft.mode);
	let contribution = $state(draft.contribution);
	let tolerance = $state(draft.tolerance);
	let result: RebalanceResult | null = $state(null);
	let error = $state('');

	const num = (s: string) => s.replace(/\s/g, '').replace(',', '.') || '0';
	const targetSum = $derived(rows.reduce((acc, r) => acc + (Number(num(r.targetPercent)) || 0), 0));

	$effect(() => {
		const snapshot = {
			rows: rows.map(({ key: _key, ...r }) => r),
			mode,
			contribution,
			tolerance
		};
		try {
			localStorage.setItem(DRAFT_KEY, JSON.stringify(snapshot));
		} catch {
			/* ignore */
		}
	});

	async function calculate() {
		error = '';
		try {
			result = await invoke<RebalanceResult>('rebalance', {
				input: {
					positions: rows.map((r, i) => ({
						id: r.id || `#${i + 1}`,
						quantity: num(r.quantity),
						price: num(r.price),
						targetPercent: num(r.targetPercent),
						fractional: r.fractional
					})),
					contribution: num(contribution),
					mode,
					tolerancePercent: num(tolerance)
				}
			});
		} catch (e) {
			result = null;
			error = errorText(e);
		}
	}

</script>

<div class="stack">
	<h1>{t('rebalance.title')}</h1>
	<p class="muted hint">{t('rebalance.hint', { currency: settings.baseCurrency })}</p>

	<section class="card stack">
		<div class="table-wrap">
			<table class="inputs">
				<thead>
					<tr>
						<th>{t('rebalance.name')}</th>
						<th class="num">{t('rebalance.quantity')}</th>
						<th class="num">{t('rebalance.price')}</th>
						<th class="num">{t('rebalance.target')}</th>
						<th>{t('rebalance.fractional')}</th>
						<th></th>
					</tr>
				</thead>
				<tbody>
					{#each rows as r (r.key)}
						<tr>
							<td class="name" data-label={t('rebalance.name')}><input bind:value={r.id} aria-label={t('rebalance.name')} /></td>
							<td data-label={t('rebalance.quantity')}><input class="n" inputmode="decimal" bind:value={r.quantity} aria-label={t('rebalance.quantity')} /></td>
							<td data-label={t('rebalance.price')}><input class="n" inputmode="decimal" bind:value={r.price} aria-label={t('rebalance.price')} /></td>
							<td data-label={t('rebalance.target')}><input class="n" inputmode="decimal" bind:value={r.targetPercent} aria-label={t('rebalance.target')} /></td>
							<td class="frac" data-label={t('rebalance.fractional')}><input type="checkbox" bind:checked={r.fractional} aria-label={t('rebalance.fractional')} /></td>
							<td class="del">
								<button
									class="btn ghost"
									title={t('common.remove')}
									aria-label={t('common.remove')}
									onclick={() => (rows = rows.filter((x) => x.key !== r.key))}><Icon name="trash" size={16} /></button
								>
							</td>
						</tr>
					{/each}
				</tbody>
			</table>
		</div>

		<div class="row between">
			<button class="btn" onclick={() => rows.push(row())}><Icon name="plus" size={16} />{t('rebalance.addPosition')}</button>
			<span class:neg={Math.abs(targetSum - 100) > 0.01} class="sum">{t('rebalance.sum', { sum: fmtNumber(targetSum) })}</span>
		</div>

		<div class="row">
			<label class="field">
				{t('rebalance.mode')}
				<select bind:value={mode}>
					<option value="buyOnly">{t('rebalance.mode.buyOnly')}</option>
					<option value="full">{t('rebalance.mode.full')}</option>
				</select>
			</label>
			<label class="field">
				{t('rebalance.contribution')} ({settings.baseCurrency})
				<input inputmode="decimal" bind:value={contribution} />
			</label>
			<label class="field">
				{t('rebalance.tolerance')}
				<input inputmode="decimal" bind:value={tolerance} class="short" />
			</label>
			<button class="btn primary" onclick={calculate}>{t('common.calculate')}</button>
		</div>
	</section>

	{#if error}
		<div class="notice error">{error}</div>
	{/if}

	{#if result}
		<RebalanceResultView {result} />
	{/if}
</div>

<style>
	.hint {
		margin: 0;
		max-width: 70ch;
	}
	.inputs td {
		padding: 0.25rem;
		border-bottom: none;
	}
	.inputs input:not([type='checkbox']) {
		width: 100%;
		min-width: 6rem;
	}
	.inputs input.n {
		text-align: right;
	}
	.between {
		justify-content: space-between;
		align-items: center;
	}
	.short {
		width: 6rem;
	}
	.sum {
		font-variant-numeric: tabular-nums;
	}
	/* Phones: every position becomes a small card with labelled fields. */
	@media (max-width: 640px) {
		.inputs,
		.inputs tbody {
			display: block;
		}
		.inputs thead {
			display: none;
		}
		.inputs tr {
			display: grid;
			grid-template-columns: 1fr 1fr;
			gap: 0.5rem;
			padding: 0.7rem 0;
			border-bottom: 1px solid var(--border);
		}
		.inputs td {
			display: block;
			padding: 0;
			min-width: 0;
		}
		.inputs td::before {
			content: attr(data-label);
			display: block;
			font-size: 0.8rem;
			color: var(--text-muted);
			margin-bottom: 0.2rem;
		}
		.inputs td.name {
			grid-column: 1;
			grid-row: 1;
		}
		.inputs td.del {
			grid-column: 2;
			grid-row: 1;
			justify-self: end;
			align-self: end;
		}
		.inputs td.del::before {
			display: none;
		}
		.inputs td.frac {
			display: flex;
			align-items: center;
			gap: 0.5rem;
			align-self: end;
			min-height: 2.4rem;
		}
		.inputs td.frac::before {
			order: 2;
			margin: 0;
			font-size: 0.9rem;
		}
	}
</style>
