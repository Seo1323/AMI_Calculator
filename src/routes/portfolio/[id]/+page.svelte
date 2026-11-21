<script lang="ts">
	import { page } from '$app/state';
	import { ask } from '$lib/confirm';
	import { t, fmtMoney, fmtNumber, errorText } from '$lib/i18n/index.svelte';
	import { db } from '$lib/db';
	import { invoke, isTauri, type Mode, type RebalanceResult } from '$lib/tauri';
	import DateInput from '$lib/components/DateInput.svelte';
	import Icon from '$lib/components/Icon.svelte';
	import HistoryChart from '$lib/components/HistoryChart.svelte';
	import AllocationPie from '$lib/components/AllocationPie.svelte';
	import TargetSliders from '$lib/components/TargetSliders.svelte';
	import { fromSaved, fromUnits, type TargetRow } from '$lib/targets';
	import RefreshQuotes from '$lib/components/RefreshQuotes.svelte';
	import { quotes as quoteState } from '$lib/quotes.svelte';
	import RebalanceResultView from '$lib/components/RebalanceResultView.svelte';
	import { parseAmounts, summarize, type Summary } from '$lib/whatif';
	import { toDecimal, today, type AccountType, type History, type Instrument, type Portfolio, type PortfolioView, type Trade } from '$lib/types';

	const id = $derived(page.params.id ?? '');

	let view = $state<PortfolioView | null>(null);
	let history = $state<History | null>(null);
	let instruments: Instrument[] = $state([]);
	let trades: Trade[] = $state([]);
	let error = $state('');

	// Target editing
	let editing = $state(false);
	let draftRows: TargetRow[] = $state([]);

	// New trade
	const emptyTrade = () => ({ date: today(), instrumentId: '', side: 'buy' as Trade['side'], quantity: '', price: '', fee: '0' });
	let tradeForm = $state(emptyTrade());
	let editingTradeId = $state<string | null>(null);

	// Portfolio name / tolerance
	let editingDetails = $state(false);
	let detailsForm = $state({ name: '', tolerance: '', accountType: 'regular' as AccountType });

	// Rebalance
	let mode: Mode = $state('buyOnly');
	let contribution = $state(page.url.searchParams.get('contribution') ?? '0');
	let result = $state<RebalanceResult | null>(null);

	const currency = $derived(view?.portfolio.baseCurrency ?? 'PLN');
	const bySymbol = $derived(Object.fromEntries(instruments.map((i) => [i.id, i])));
	const symbol = (instrumentId: string) => bySymbol[instrumentId]?.symbol ?? '?';
	const moneyKind = $derived(bySymbol[tradeForm.instrumentId]?.kind);
	const isCash = $derived(moneyKind === 'cash' || moneyKind === 'deposit');
	// A cash deposit is quantity = amount at price 1.
	$effect(() => {
		if (isCash && tradeForm.price.trim() === '') tradeForm.price = '1';
	});

	async function run(fn: () => Promise<unknown>) {
		error = '';
		try {
			if (!isTauri()) throw { code: 'notInTauri' };
			await fn();
		} catch (e) {
			error = errorText(e);
		}
	}

	async function load() {
		const wanted = id;
		// The view is awaited first: it records today's value, which the history then includes.
		const v = await invoke<PortfolioView>('portfolio_view', { id });
		const [h, all, ownTrades] = await Promise.all([
			invoke<History>('portfolio_history', { id }),
			db.list<Instrument>('instrument'),
			invoke<Trade[]>('portfolio_trades', { id })
		]);
		if (wanted !== id) return; // the user moved to another portfolio while this was loading
		view = v;
		history = h;
		instruments = all.sort((a, b) => a.symbol.localeCompare(b.symbol));
		trades = ownTrades.sort((a, b) => b.date.localeCompare(a.date) || b.createdAt.localeCompare(a.createdAt));
		if (!tradeForm.instrumentId && instruments.length) tradeForm.instrumentId = instruments[0].id;
	}

	$effect(() => {
		void quoteState.version; // reload after every quote refresh
		if (id) run(load);
	});

	function startEditing() {
		draftRows = fromSaved((view?.positions ?? []).map((p) => ({ id: p.instrument.id, percent: p.targetPercent })));
		editing = true;
	}

	const saveTargets = () =>
		run(async () => {
			await invoke('portfolio_set_targets', {
				id,
				targets: draftRows
					.filter((r) => r.units > 0)
					.map((r) => ({ instrumentId: r.id, targetPercent: String(fromUnits(r.units)) }))
			});
			editing = false;
			result = null;
			comparison = null;
			await load();
		});

	function startEditTrade(tr: Trade) {
		editingTradeId = tr.id;
		tradeForm = { date: tr.date, instrumentId: tr.instrumentId, side: tr.side, quantity: tr.quantity, price: tr.price, fee: tr.fee };
	}

	function cancelEditTrade() {
		editingTradeId = null;
		tradeForm = { ...emptyTrade(), instrumentId: tradeForm.instrumentId };
	}

	function startEditDetails() {
		if (!view) return;
		detailsForm = { name: view.portfolio.name, tolerance: view.portfolio.tolerancePercent, accountType: view.portfolio.accountType };
		editingDetails = true;
	}

	const saveDetails = () =>
		run(async () => {
			if (!view) return;
			await db.save<Portfolio>('portfolio', {
				...view.portfolio,
				name: detailsForm.name.trim(),
				tolerancePercent: toDecimal(detailsForm.tolerance),
				accountType: detailsForm.accountType
			});
			editingDetails = false;
			result = null;
			comparison = null;
			await load();
		});

	const addTrade = () =>
		run(async () => {
			await db.save<Trade>('trade', {
				...(editingTradeId ? { id: editingTradeId } : {}),
				portfolioId: id,
				instrumentId: tradeForm.instrumentId,
				date: tradeForm.date,
				side: tradeForm.side,
				quantity: toDecimal(tradeForm.quantity),
				price: toDecimal(tradeForm.price),
				fee: toDecimal(tradeForm.fee)
			});
			tradeForm = { ...emptyTrade(), instrumentId: tradeForm.instrumentId };
			editingTradeId = null;
			result = null;
			comparison = null;
			await load();
		});

	const deleteTrade = (tr: Trade) =>
		run(async () => {
			if (!(await ask(`${tr.date} ${symbol(tr.instrumentId)} ${tr.quantity}?`))) return;
			await db.remove('trade', tr.id);
			if (editingTradeId === tr.id) cancelEditTrade();
			result = null;
			comparison = null;
			await load();
		});

	const calculate = () =>
		run(async () => {
			result = await invoke<RebalanceResult>('portfolio_rebalance', { id, contribution: toDecimal(contribution), mode });
		});

	// What-if: the same read-only rebalance for several deposits at once, nothing is saved.
	let compareText = $state('500; 1000; 2000; 5000');
	let comparison = $state<{ amount: string; s: Summary }[] | null>(null);
	let compareError = $state('');
	const compare = () =>
		run(async () => {
			const amounts = parseAmounts(compareText);
			compareError = amounts.length ? '' : t('pf.compareNone');
			if (!amounts.length) return;
			const results = await Promise.all(
				amounts.map((a) => invoke<RebalanceResult>('portfolio_rebalance', { id, contribution: a, mode }))
			);
			comparison = amounts.map((amount, i) => ({ amount, s: summarize(results[i]) }));
		});

	const tradeTotal = $derived(
		(Number(toDecimal(tradeForm.quantity)) || 0) * (Number(toDecimal(tradeForm.price)) || 0) +
			(tradeForm.side === 'buy' ? 1 : -1) * (Number(toDecimal(tradeForm.fee)) || 0)
	);
</script>

<div class="stack">
	<a href="/portfolio" class="back">{t('pf.back')}</a>
	<RefreshQuotes />

	{#if error}<div class="notice error">{error}</div>{/if}

	{#if view}
		<header class="head">
			<div>
				{#if editingDetails}
					<div class="row">
						<label class="field">
							{t('portfolio.name')}
							<input bind:value={detailsForm.name} />
						</label>
						<label class="field">
							{t('account.type')}
							<select bind:value={detailsForm.accountType}>
								<option value="regular">{t('account.regular')}</option>
								<option value="ike">{t('account.ike')}</option>
								<option value="ikze">{t('account.ikze')}</option>
							</select>
						</label>
						
						<label class="field">
							{t('portfolio.tolerance')}: {fmtNumber(Number(detailsForm.tolerance) || 0)}
							<input
								type="range"
								min="0"
								max="20"
								step="0.5"
								value={Number(detailsForm.tolerance) || 0}
								oninput={(e) => (detailsForm.tolerance = e.currentTarget.value)}
							/>
						</label>
						<button class="btn primary" disabled={!detailsForm.name.trim()} onclick={saveDetails}>{t('common.save')}</button>
						<button class="btn" onclick={() => (editingDetails = false)}>{t('common.cancel')}</button>
					</div>
				{:else}
					<h1>
						{view.portfolio.name}
						<button class="btn ghost" aria-label={t('common.edit')} title={t('common.edit')} onclick={startEditDetails}
							><Icon name="edit" size={16} /></button
						>
					</h1>
					<div class="muted small">
						{t(`account.${view.portfolio.accountType}` as Parameters<typeof t>[0])} · {t('portfolio.tolerance')}: {fmtNumber(view.portfolio.tolerancePercent)}
					</div>
				{/if}
				<div class="big">{fmtMoney(view.totalValue, currency)}</div>
			</div>
			<div class="kpis">
				<div>
					<span class="muted">{t('portfolio.gain')}</span>
					<strong class:pos={Number(view.totalGain) > 0} class:neg={Number(view.totalGain) < 0}>
						{fmtMoney(view.totalGain, currency)}
					</strong>
				</div>
				{#if view.avgInterestPercent !== null}
					<div>
						<span class="muted">{t('portfolio.avgInterest')}</span>
						<strong>{fmtNumber(view.avgInterestPercent)}%</strong>
					</div>
				{/if}
				{#if view.maxDeviation !== null}
					<div>
						<span class="muted">{t('rebalance.deviation')}</span>
						<strong class:warn-text={view.needsRebalance}>{fmtNumber(view.maxDeviation)} {t('unit.pp')}</strong>
					</div>
				{/if}
			</div>
		</header>

		{#if view.problem}<div class="notice error">{errorText(view.problem)} {t('portfolio.dataProblem')}</div>{/if}
		{#if view.missingPrices.length}
			<div class="notice warn">{t('pf.missingPrices', { list: view.missingPrices.join(', ') })}</div>
		{/if}
		{#if view.estimatedPrices.length}
			<div class="notice">{t('pf.estimatedPrices', { list: view.estimatedPrices.join(', ') })}</div>
		{/if}
		{#if view.missingRates.length}
			<div class="notice warn">{t('pf.missingRates', { list: view.missingRates.join(', ') })}</div>
		{/if}

		<!-- Split of the value by asset kind -->
		<section class="card stack">
			<h2>{t('pf.allocation.title')}</h2>
			<AllocationPie
				shares={view.kindShares}
				total={view.totalValue}
				{currency}
				unvalued={view.positions.filter((p) => p.value === null && Number(p.quantity) > 0).length}
			/>
		</section>

		{#if history && (history.values.length || history.invested.length)}
			<section class="card stack">
				<h2>{t('history.title')}</h2>
				<HistoryChart values={history.values} invested={history.invested} {currency} />
			</section>
		{/if}

		<!-- Positions & targets -->
		<section class="card stack">
			<div class="section-head">
				<h2>{t('pf.positions')}</h2>
				{#if !editing}
					<button class="btn" onclick={startEditing}>{t('pf.editTargets')}</button>
				{/if}
			</div>

			{#if editing}
				<div class="stack">
					<TargetSliders bind:rows={draftRows} options={instruments.map((i) => ({ id: i.id, label: `${i.symbol} · ${i.name}` }))} />
					<div class="row">
						<button class="btn primary" onclick={saveTargets}>{t('pf.saveTargets')}</button>
						<button class="btn" onclick={() => (editing = false)}>{t('common.cancel')}</button>
					</div>
				</div>
			{:else if view.positions.length === 0}
				<p class="muted">{t('pf.noPositions')}</p>
			{:else}
				<div class="table-wrap">
					<table>
						<thead>
							<tr>
								<th>{t('instruments.symbol')}</th>
								<th class="num hide-xxs">{t('pf.quantity')}</th>
								<th class="num hide-sm">{t('pf.avgCost')}</th>
								<th class="num hide-xs">{t('pf.price')}</th>
								<th class="num">{t('pf.value')}</th>
								<th class="num hide-xs">{t('pf.gain')}</th>
								<th class="num hide-sm">{t('pf.share')}</th>
								<th class="num">{t('pf.target')}</th>
							</tr>
						</thead>
						<tbody>
							{#each view.positions as p (p.instrument.id)}
								{@const dev = p.currentPercent === null ? 0 : Number(p.currentPercent) - Number(p.targetPercent)}
								<tr>
									<td class="instr"><strong>{p.instrument.symbol}</strong><span class="muted">{p.instrument.name}</span></td>
									<td class="num hide-xxs">{fmtNumber(p.quantity, 8)}</td>
									<td class="num hide-sm">{p.avgCost === null ? '—' : fmtMoney(p.avgCost, p.instrument.currency)}</td>
									<td class="num hide-xs">{p.price === null ? '—' : fmtMoney(p.price, p.instrument.currency)}</td>
									<td class="num">{p.value === null ? '—' : fmtMoney(p.value, currency)}</td>
									<td class="num hide-xs" class:pos={Number(p.gain) > 0} class:neg={Number(p.gain) < 0}>
										{p.gain === null ? '—' : fmtMoney(p.gain, currency)}
									</td>
									<td class="num hide-sm">{p.currentPercent === null ? '—' : `${fmtNumber(p.currentPercent)}%`}</td>
									<td class="num" class:warn-text={Math.abs(dev) > Number(view.portfolio.tolerancePercent)}>
										{fmtNumber(p.targetPercent)}%
									</td>
								</tr>
							{/each}
						</tbody>
					</table>
				</div>
				{#if Math.abs(Number(view.targetsSum) - 100) > 0.01}
					<div class="notice warn">{t('pf.targetsSum', { sum: fmtNumber(view.targetsSum) })}</div>
				{/if}
			{/if}
		</section>

		<!-- Rebalance -->
		<section class="card stack">
			<h2>{t('pf.rebalance')}</h2>
			<p class="muted small">{t('pf.rebalanceHint')}</p>
			<div class="row">
				<label class="field">
					{t('rebalance.mode')}
					<select bind:value={mode}>
						<option value="buyOnly">{t('rebalance.mode.buyOnly')}</option>
						<option value="full">{t('rebalance.mode.full')}</option>
					</select>
				</label>
				<label class="field">
					{t('rebalance.contribution')} ({currency})
					<input inputmode="decimal" bind:value={contribution} />
				</label>
				<button class="btn primary" onclick={calculate}>{t('common.calculate')}</button>
			</div>
		</section>
		{#if result}
			<RebalanceResultView {result} {currency} label={symbol} />
		{/if}

		<!-- What if -->
		<section class="card stack">
			<h2>{t('pf.compare')}</h2>
			<p class="muted small">{t('pf.compareHint')}</p>
			<div class="row">
				<label class="field">
					{t('pf.compareAmounts')} ({currency})
					<input bind:value={compareText} />
				</label>
				<button class="btn primary" onclick={compare}>{t('common.calculate')}</button>
			</div>
			{#if compareError}
				<div class="notice error">{compareError}</div>
			{/if}
			{#if comparison}
				<div class="table-wrap">
					<table>
						<thead>
							<tr>
								<th class="num">{t('rebalance.contribution')}</th>
								<th class="num">{t('pf.compareAfter')}</th>
								<th class="num">{t('trades.buy')}</th>
								<th class="num">{t('trades.sell')}</th>
								<th class="num">{t('rebalance.cashLeft')}</th>
							</tr>
						</thead>
						<tbody>
							{#each comparison as c (c.amount)}
								<tr>
									<td class="num">{fmtMoney(c.amount, currency)}</td>
									<td class="num">{fmtNumber(c.s.maxDeviationAfter)} {t('unit.pp')}</td>
									<td class="num">{fmtMoney(c.s.bought, currency)}</td>
									<td class="num">{fmtMoney(c.s.sold, currency)}</td>
									<td class="num">{fmtMoney(c.s.cashLeft, currency)}</td>
								</tr>
							{/each}
						</tbody>
					</table>
				</div>
			{/if}
		</section>

		<!-- Trades -->
		<section class="card stack">
			<h2>{editingTradeId ? t('trades.editing') : t('trades.title')}</h2>
			{#if instruments.length === 0}
				<p class="muted">{t('trades.needInstrument')}</p>
			{:else}
				<div class="row">
					<label class="field">
						{t('trades.date')}
						<DateInput bind:value={tradeForm.date} />
					</label>
					<label class="field">
						{t('trades.instrument')}
						<select bind:value={tradeForm.instrumentId}>
							{#each instruments as i (i.id)}<option value={i.id}>{i.symbol}</option>{/each}
						</select>
					</label>
					<label class="field">
						{t('trades.side')}
						<select bind:value={tradeForm.side}>
							<option value="buy">{t('trades.buy')}</option>
							<option value="sell">{t('trades.sell')}</option>
						</select>
					</label>
					<label class="field">
						{t('trades.quantity')}
						<input class="short" inputmode="decimal" bind:value={tradeForm.quantity} />
					</label>
					<label class="field">
						{t('trades.price')} ({bySymbol[tradeForm.instrumentId]?.currency ?? ''})
						<input class="short" inputmode="decimal" bind:value={tradeForm.price} />
					</label>
					<label class="field">
						{t('trades.fee')}
						<input class="short" inputmode="decimal" bind:value={tradeForm.fee} />
					</label>
					<button
						class="btn primary"
						disabled={!tradeForm.instrumentId || !tradeForm.quantity.trim() || !tradeForm.price.trim()}
						onclick={addTrade}>{editingTradeId ? t('common.save') : t('trades.add')}</button
					>
					{#if editingTradeId}
						<button class="btn" onclick={cancelEditTrade}>{t('common.cancel')}</button>
					{/if}
				</div>
				{#if isCash}
					<p class="muted small">{t(moneyKind === 'deposit' ? 'trades.depositHint' : 'trades.cashHint')}</p>
				{/if}
				{#if tradeTotal}
					<div class="muted small">
						{t('trades.total')}: {fmtMoney(tradeTotal, bySymbol[tradeForm.instrumentId]?.currency)}
					</div>
				{/if}
			{/if}

			{#if trades.length === 0}
				<p class="muted">{t('trades.empty')}</p>
			{:else}
				<div class="table-wrap">
					<table>
						<thead>
							<tr>
								<th>{t('trades.date')}</th>
								<th>{t('trades.instrument')}</th>
								<th>{t('trades.side')}</th>
								<th class="num">{t('trades.quantity')}</th>
								<th class="num hide-sm">{t('trades.price')}</th>
								<th class="num hide-sm">{t('trades.fee')}</th>
								<th class="num">{t('trades.total')}</th>
								<th></th>
							</tr>
						</thead>
						<tbody>
							{#each trades as tr (tr.id)}
								{@const cur = bySymbol[tr.instrumentId]?.currency}
								<tr>
									<td>{tr.date}</td>
									<td>{symbol(tr.instrumentId)}</td>
									<td class={tr.side === 'buy' ? 'pos' : 'neg'}>{t(`trades.${tr.side}`)}</td>
									<td class="num">{fmtNumber(tr.quantity, 8)}</td>
									<td class="num hide-sm">{fmtMoney(tr.price, cur)}</td>
									<td class="num hide-sm">{fmtMoney(tr.fee, cur)}</td>
									<td class="num">{fmtMoney(Number(tr.quantity) * Number(tr.price), cur)}</td>
									<td class="actions">
										<button class="btn ghost" aria-label={t('common.edit')} title={t('common.edit')} onclick={() => startEditTrade(tr)}
											><Icon name="edit" size={16} /></button
										>
										<button class="btn ghost" aria-label={t('common.delete')} onclick={() => deleteTrade(tr)}
											><Icon name="trash" size={16} /></button
										>
									</td>
								</tr>
							{/each}
						</tbody>
					</table>
				</div>
			{/if}
		</section>
	{/if}
</div>

<style>
	p {
		margin: 0;
	}
	.back {
		text-decoration: none;
		width: fit-content;
	}
	.small {
		font-size: 0.85rem;
	}
	.head {
		display: flex;
		flex-wrap: wrap;
		justify-content: space-between;
		align-items: end;
		gap: var(--space);
	}
	.big {
		font-size: clamp(1.6rem, 7vw, 2rem);
		font-weight: 700;
		font-variant-numeric: tabular-nums;
	}
	.kpis {
		display: flex;
		gap: calc(var(--space) * 2);
	}
	.kpis div {
		display: flex;
		flex-direction: column;
		font-variant-numeric: tabular-nums;
	}
	.kpis span {
		font-size: 0.8rem;
	}
	.section-head {
		display: flex;
		justify-content: space-between;
		align-items: center;
		gap: var(--space);
	}
	.warn-text {
		color: var(--warning);
		font-weight: 600;
	}
	.short {
		width: 7rem;
	}
	.instr {
		white-space: normal;
		min-width: 9rem;
	}
	.instr span {
		display: block;
		font-size: 0.8rem;
	}
	.actions {
		white-space: nowrap;
	}
	h1 .btn {
		vertical-align: middle;
	}
</style>
