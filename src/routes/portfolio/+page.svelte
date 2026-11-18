<script lang="ts">
	import { onMount } from 'svelte';
	import { confirm } from '@tauri-apps/plugin-dialog';
	import { t, fmtMoney, fmtNumber, fmtDate, errorText } from '$lib/i18n/index.svelte';
	import { settings } from '$lib/settings.svelte';
	import { db } from '$lib/db';
	import { invoke, isTauri } from '$lib/tauri';
	import Icon from '$lib/components/Icon.svelte';
	import RefreshQuotes from '$lib/components/RefreshQuotes.svelte';
	import { quotes as quoteState } from '$lib/quotes.svelte';
	import {
		CURRENCIES,
		INSTRUMENT_KINDS,
		QUOTE_SOURCES,
		toDecimal,
		type FxRate,
		type Instrument,
		type Portfolio,
		type PortfolioView
	} from '$lib/types';

	interface QuoteRow {
		instrumentId: string;
		price: string;
		currency: string;
		fetchedAt: string;
	}

	let views: PortfolioView[] = $state([]);
	let instruments: Instrument[] = $state([]);
	let quotes: Record<string, QuoteRow> = $state({});
	let rates: FxRate[] = $state([]);
	let error = $state('');

	const emptyPortfolio = () => ({ name: '', baseCurrency: settings.baseCurrency, tolerancePercent: '5' });
	const emptyInstrument = () => ({
		id: undefined as string | undefined,
		symbol: '',
		name: '',
		kind: 'etf' as Instrument['kind'],
		currency: 'PLN',
		quoteSource: 'yahoo' as Instrument['quoteSource'],
		fractional: false
	});
	let newPortfolio = $state(emptyPortfolio());
	let instrumentForm = $state(emptyInstrument());
	let priceInputs: Record<string, string> = $state({});
	let fxForm = $state({ currency: 'EUR', ratePln: '' });

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
		const portfolios = await db.list<Portfolio>('portfolio');
		views = await Promise.all(portfolios.map((p) => invoke<PortfolioView>('portfolio_view', { id: p.id })));
		instruments = (await db.list<Instrument>('instrument')).sort((a, b) => a.symbol.localeCompare(b.symbol));
		const q = await invoke<QuoteRow[]>('quote_list');
		quotes = Object.fromEntries(q.map((x) => [x.instrumentId, x]));
		priceInputs = Object.fromEntries(instruments.map((i) => [i.id, quotes[i.id]?.price ?? '']));
		rates = await invoke<FxRate[]>('fx_list');
	}

	onMount(() => run(load));

	// Reload after every quote refresh (manual or automatic).
	let seenVersion = quoteState.version;
	$effect(() => {
		if (quoteState.version !== seenVersion) {
			seenVersion = quoteState.version;
			run(load);
		}
	});

	const createPortfolio = () =>
		run(async () => {
			await db.save<Portfolio>('portfolio', {
				...newPortfolio,
				tolerancePercent: toDecimal(newPortfolio.tolerancePercent)
			});
			newPortfolio = emptyPortfolio();
			await load();
		});

	const deletePortfolio = (p: Portfolio) =>
		run(async () => {
			if (!(await confirm(t('portfolio.deleteConfirm', { name: p.name }), { kind: 'warning' }))) return;
			await db.remove('portfolio', p.id);
			await load();
		});

	const saveInstrument = () =>
		run(async () => {
			const { id, ...rest } = instrumentForm;
			const existing = instruments.find((i) => i.id === id);
			await db.save<Instrument>('instrument', { ...existing, ...rest, symbol: rest.symbol.trim() });
			instrumentForm = emptyInstrument();
			await load();
		});

	const editInstrument = (i: Instrument) => {
		instrumentForm = {
			id: i.id,
			symbol: i.symbol,
			name: i.name,
			kind: i.kind,
			currency: i.currency,
			quoteSource: i.quoteSource,
			fractional: i.fractional
		};
	};

	const savePrice = (i: Instrument) =>
		run(async () => {
			await invoke('quote_set', { instrumentId: i.id, price: toDecimal(priceInputs[i.id]), currency: i.currency });
			await load();
		});

	const saveRate = () =>
		run(async () => {
			await invoke('fx_set', { currency: fxForm.currency, ratePln: toDecimal(fxForm.ratePln) });
			fxForm = { currency: fxForm.currency, ratePln: '' };
			await load();
		});
</script>

<div class="stack">
	<h1>{t('portfolio.title')}</h1>
	<RefreshQuotes />

	{#if error}<div class="notice error">{error}</div>{/if}

	<!-- Portfolios -->
	{#if views.length === 0}
		<p class="muted">{t('portfolio.empty')}</p>
	{:else}
		<div class="tiles">
			{#each views as v (v.portfolio.id)}
				<a class="card tile" href={`/portfolio/${v.portfolio.id}`}>
					<div class="tile-head">
						<h2>{v.portfolio.name}</h2>
						<button
							class="btn ghost"
							title={t('portfolio.delete')}
							aria-label={t('portfolio.delete')}
							onclick={(e) => {
								e.preventDefault();
								deletePortfolio(v.portfolio);
							}}><Icon name="trash" size={16} /></button
						>
					</div>
					<div class="big">{fmtMoney(v.totalValue, v.portfolio.baseCurrency)}</div>
					<div class:pos={Number(v.totalGain) > 0} class:neg={Number(v.totalGain) < 0}>
						{t('portfolio.gain')}: {fmtMoney(v.totalGain, v.portfolio.baseCurrency)}
					</div>
					{#if v.missingPrices.length || v.missingRates.length}
						<div class="muted small">{t('portfolio.incomplete')}</div>
					{:else if v.maxDeviation !== null}
						<div class="small" class:warn-text={v.needsRebalance}>
							{t('portfolio.drift', { dev: fmtNumber(v.maxDeviation) })}
						</div>
					{/if}
				</a>
			{/each}
		</div>
	{/if}

	<section class="card stack">
		<h2>{t('portfolio.new')}</h2>
		<div class="row">
			<label class="field grow">
				{t('portfolio.name')}
				<input bind:value={newPortfolio.name} placeholder="IKE, IKZE…" />
			</label>
			<label class="field">
				{t('portfolio.baseCurrency')}
				<select bind:value={newPortfolio.baseCurrency}>
					{#each CURRENCIES as c (c)}<option value={c}>{c}</option>{/each}
				</select>
			</label>
			<label class="field">
				{t('portfolio.tolerance')}
				<input class="short" inputmode="decimal" bind:value={newPortfolio.tolerancePercent} />
			</label>
			<button class="btn primary" disabled={!newPortfolio.name.trim()} onclick={createPortfolio}>
				{t('portfolio.create')}
			</button>
		</div>
	</section>

	<!-- Instruments -->
	<section class="card stack">
		<h2>{t('instruments.title')}</h2>
		<p class="muted small">{t('instruments.hint')}</p>
		{#if instruments.length === 0}
			<p class="muted">{t('instruments.empty')}</p>
		{:else}
			<div class="table-wrap">
				<table>
					<thead>
						<tr>
							<th>{t('instruments.symbol')}</th>
							<th>{t('instruments.kind')}</th>
							<th>{t('instruments.currency')}</th>
							<th class="num">{t('instruments.price')}</th>
							<th>{t('instruments.priceAt')}</th>
							<th></th>
						</tr>
					</thead>
					<tbody>
						{#each instruments as i (i.id)}
							<tr>
								<td class="instr"><strong>{i.symbol}</strong><span class="muted">{i.name}</span></td>
								<td>{t(`kind.${i.kind}`)}{i.fractional ? ' · ½' : ''}</td>
								<td>{i.currency}</td>
								<td class="num">
									<form
										class="inline"
										onsubmit={(e) => {
											e.preventDefault();
											savePrice(i);
										}}
									>
										<input class="price" inputmode="decimal" bind:value={priceInputs[i.id]} aria-label={t('instruments.price')} />
										<button class="btn" type="submit">{t('common.save')}</button>
									</form>
								</td>
								<td class="muted small">{quotes[i.id] ? fmtDate(quotes[i.id].fetchedAt) : '—'}</td>
								<td><button class="btn ghost" onclick={() => editInstrument(i)}>{t('common.edit')}</button></td>
							</tr>
						{/each}
					</tbody>
				</table>
			</div>
		{/if}

		<h3>{instrumentForm.id ? t('common.edit') : t('instruments.add')}</h3>
		<div class="row">
			<label class="field">
				{t('instruments.symbol')}
				<input
					class="mid"
					bind:value={instrumentForm.symbol}
					placeholder={instrumentForm.quoteSource === 'coinGecko' ? 'bitcoin' : 'PKN.WA, VWCE.DE'}
				/>
			</label>
			<label class="field grow">
				{t('instruments.name')}
				<input bind:value={instrumentForm.name} />
			</label>
			<label class="field">
				{t('instruments.kind')}
				<select
					bind:value={instrumentForm.kind}
					onchange={() => {
						if (instrumentForm.kind === 'crypto') {
							instrumentForm.fractional = true;
							instrumentForm.quoteSource = 'coinGecko';
						} else if (instrumentForm.quoteSource === 'coinGecko') {
							instrumentForm.quoteSource = 'yahoo';
						}
					}}
				>
					{#each INSTRUMENT_KINDS as k (k)}<option value={k}>{t(`kind.${k}`)}</option>{/each}
				</select>
			</label>
			<label class="field">
				{t('instruments.currency')}
				<select bind:value={instrumentForm.currency}>
					{#each CURRENCIES as c (c)}<option value={c}>{c}</option>{/each}
				</select>
			</label>
			<label class="field">
				{t('instruments.source')}
				<select bind:value={instrumentForm.quoteSource}>
					{#each QUOTE_SOURCES as s (s)}<option value={s}>{t(`source.${s}`)}</option>{/each}
				</select>
			</label>
			<label class="check">
				<input type="checkbox" bind:checked={instrumentForm.fractional} />
				{t('instruments.fractional')}
			</label>
		</div>
		<p class="muted small">{t(`instruments.symbolHint.${instrumentForm.quoteSource}`)}</p>
		<div class="row">
			<button
				class="btn primary"
				disabled={!instrumentForm.symbol.trim() || !instrumentForm.name.trim()}
				onclick={saveInstrument}
			>
				{instrumentForm.id ? t('common.save') : t('instruments.add')}
			</button>
			{#if instrumentForm.id}
				<button class="btn" onclick={() => (instrumentForm = emptyInstrument())}>{t('common.cancel')}</button>
			{/if}
		</div>
	</section>

	<!-- Exchange rates -->
	<section class="card stack">
		<h2>{t('fx.title')}</h2>
		<p class="muted small">{t('fx.hint')}</p>
		{#if rates.length === 0}
			<p class="muted">{t('fx.empty')}</p>
		{:else}
			<table class="narrow">
				<tbody>
					{#each rates as r (r.currency)}
						<tr>
							<td><strong>1 {r.currency}</strong></td>
							<td class="num">{fmtNumber(r.ratePln, 4)} PLN</td>
							<td class="muted small">{fmtDate(r.fetchedAt)}</td>
						</tr>
					{/each}
				</tbody>
			</table>
		{/if}
		<div class="row">
			<label class="field">
				{t('fx.currency')}
				<select bind:value={fxForm.currency}>
					{#each CURRENCIES.filter((c) => c !== 'PLN') as c (c)}<option value={c}>{c}</option>{/each}
				</select>
			</label>
			<label class="field">
				{t('fx.rate')}
				<input class="short" inputmode="decimal" bind:value={fxForm.ratePln} placeholder="4,30" />
			</label>
			<button class="btn primary" disabled={!fxForm.ratePln.trim()} onclick={saveRate}>{t('fx.add')}</button>
		</div>
	</section>
</div>

<style>
	h3 {
		font-size: 1rem;
		margin: calc(var(--space) * 0.5) 0 0;
	}
	p {
		margin: 0;
	}
	.small {
		font-size: 0.85rem;
	}
	.tiles {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(240px, 1fr));
		gap: var(--space);
	}
	.tile {
		display: flex;
		flex-direction: column;
		gap: 0.3rem;
		color: inherit;
		text-decoration: none;
		transition: border-color 0.15s;
	}
	.tile:hover {
		border-color: var(--accent);
	}
	.tile-head {
		display: flex;
		justify-content: space-between;
		align-items: center;
	}
	.big {
		font-size: 1.5rem;
		font-weight: 700;
		font-variant-numeric: tabular-nums;
	}
	.warn-text {
		color: var(--warning);
		font-weight: 600;
	}
	.grow {
		flex: 1 1 12rem;
	}
	.grow input {
		width: 100%;
	}
	.short {
		width: 6rem;
	}
	.mid {
		width: 12rem;
	}
	.check {
		display: flex;
		gap: 0.4rem;
		align-items: center;
		padding-bottom: 0.4rem;
	}
	.inline {
		display: inline-flex;
		gap: 0.3rem;
		justify-content: flex-end;
	}
	.price {
		width: 7rem;
		text-align: right;
	}
	.instr {
		white-space: normal;
		min-width: 9rem;
	}
	.instr span {
		display: block;
		font-size: 0.8rem;
	}
	.narrow {
		width: auto;
	}
</style>
