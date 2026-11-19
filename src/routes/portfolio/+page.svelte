<script lang="ts">
	import { onMount } from 'svelte';
	import { confirm } from '@tauri-apps/plugin-dialog';
	import { t, fmtMoney, fmtNumber, fmtDate, fmtDay, errorText } from '$lib/i18n/index.svelte';
	import { settings } from '$lib/settings.svelte';
	import { db } from '$lib/db';
	import { invoke, isTauri } from '$lib/tauri';
	import Icon from '$lib/components/Icon.svelte';
	import RefreshQuotes from '$lib/components/RefreshQuotes.svelte';
	import { quotes as quoteState, refreshQuotes } from '$lib/quotes.svelte';
	import { BOND_SERIES, addMonths, bondSymbol, firstOfMonth, offerMonth, remaining, termBetween } from '$lib/bond';
	import SearchSelect from '$lib/components/SearchSelect.svelte';
	import TargetSliders from '$lib/components/TargetSliders.svelte';
	import { fromUnits, type TargetRow } from '$lib/targets';
	import { loadBondOffers, SNAPSHOT_SET, type BondOffer, type OfferSet } from '$lib/bondOffers';
	import { CATALOG, catalogFor, DEPOSIT_TERMS } from '$lib/catalog';
	import {
		CURRENCIES,
		INSTRUMENT_KINDS,
		QUOTE_SOURCES,
		toDecimal,
		today,
		CAPITALIZATIONS,
		type Capitalization,
		type FxRate,
		type Instrument,
		type Portfolio,
		type PortfolioView,
		type Trade
	} from '$lib/types';

	interface QuoteRow {
		instrumentId: string;
		price: string;
		currency: string;
		fetchedAt: string;
		return12mPercent: string | null;
		dividendYieldPercent: string | null;
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
		fractional: false,
		interestPercent: '',
		startDate: '',
		maturityDate: '',
		/** Form-only: which entry of the second list is chosen ('custom' = typed by hand). */
		catalogKey: '',
		/** Form-only: bond/deposit term in months. */
		termMonths: '',
		capitalization: '' as '' | Capitalization
	});
	const emptyPosition = () => ({ portfolioId: '', quantity: '', price: '', date: today() });
	let newPortfolio = $state(emptyPortfolio());
	// Optional target allocation chosen while creating the portfolio.
	let newTargets: TargetRow[] = $state([]);
	let instrumentForm = $state(emptyInstrument());
	let position = $state(emptyPosition());
	let offerSet: OfferSet = $state(SNAPSHOT_SET);
	let offersRequested = false;
	// Values filled in from the bond offer; replaced by a new offer only while the user has not edited them.
	let autoRate = '';
	let autoPrice = '';
	// Price of a listed instrument comes from its online quote; a newer request makes older ones void.
	let priceToken = 0;
	let priceBusy = $state(false);
	let priceNote = $state('');
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
		if (!position.portfolioId && views.length) position.portfolioId = views[0].portfolio.id;
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
			const created = await db.save<Portfolio>('portfolio', {
				...newPortfolio,
				tolerancePercent: toDecimal(newPortfolio.tolerancePercent)
			});
			const targets = newTargets
				.filter((r) => r.units > 0)
				.map((r) => ({ instrumentId: r.id, targetPercent: String(fromUnits(r.units)) }));
			if (targets.length) await invoke('portfolio_set_targets', { id: created.id, targets });
			newPortfolio = emptyPortfolio();
			newTargets = [];
			await load();
		});

	const deletePortfolio = (p: Portfolio) =>
		run(async () => {
			if (!(await confirm(t('portfolio.deleteConfirm', { name: p.name }), { kind: 'warning' }))) return;
			await db.remove('portfolio', p.id);
			await load();
		});

	const listed = (k: Instrument['kind']) => ['etf', 'stock', 'crypto', 'cash', 'bond', 'deposit'].includes(k);
	const timed = (k: Instrument['kind']) => k === 'bond' || k === 'deposit';
	const hasManualRate = (k: Instrument['kind']) => k === 'bond' || k === 'cash' || k === 'deposit';

	/** Second list: concrete ETFs, bond series, coins or currencies for the chosen kind. */
	function options(kind: Instrument['kind']): { id: string; label: string }[] {
		if (kind === 'bond')
			return BOND_SERIES.map((s) => {
				const offer = offerFor(s.code);
				const rate = offer ? ` · ${pct(offer.firstPeriodRatePercent)}%` : '';
				return { id: s.code, label: `${s.code} · ${t('bond.termMonths', { n: s.months })}${rate}` };
			});
		if (kind === 'cash' || kind === 'deposit') return CURRENCIES.map((c) => ({ id: c, label: c }));
		return catalogFor(kind).map((c) => ({ id: c.id, label: `${c.name} (${c.symbol})` }));
	}

	const typedByHand = $derived(!listed(instrumentForm.kind) || instrumentForm.catalogKey === 'custom');

	function pickKind() {
		const f = instrumentForm;
		priceToken++;
		priceBusy = false;
		priceNote = '';
		position.price = '';
		autoPrice = '';
		if (f.kind === 'bond' && !offersRequested) {
			offersRequested = true;
			loadBondOffers().then((o) => (offerSet = o));
		}
		Object.assign(f, {
			catalogKey: '',
			symbol: '',
			name: '',
			interestPercent: '',
			startDate: '',
			maturityDate: '',
			termMonths: '',
			capitalization: '',
			currency: f.kind === 'bond' ? 'PLN' : settings.baseCurrency,
			fractional: ['crypto', 'cash', 'deposit'].includes(f.kind),
			quoteSource: f.kind === 'crypto' ? 'coinGecko' : f.kind === 'stock' ? 'yahoo' : f.kind === 'etf' ? 'yahoo' : 'manual'
		});
	}

	function offerFor(series: string): BondOffer | undefined {
		return offerSet.offers.find((o) => o.series === series);
	}

	/** Purchase month (YYYY-MM) the offer is valid for. */
	function offerMonthOf(offer: BondOffer): string | null {
		const months = BOND_SERIES.find((s) => s.code === offer.series)?.months;
		return months ? offerMonth(offer.symbol, months) : null;
	}

	const plain = (v: string) => String(Number(v));

	/** Default rate, capitalization and price from the current offer, only for bonds bought in its month. */
	function applyOffer() {
		const f = instrumentForm;
		if (f.kind !== 'bond' || f.catalogKey === 'custom') return;
		const offer = offerFor(f.catalogKey);
		if (!offer || !f.startDate || offerMonthOf(offer) !== f.startDate.slice(0, 7)) return;
		if (f.interestPercent === '' || f.interestPercent === autoRate) {
			autoRate = plain(offer.firstPeriodRatePercent);
			f.interestPercent = autoRate;
			f.capitalization = offer.capitalization ?? '';
		}
		if (position.price === '' || position.price === autoPrice) {
			autoPrice = plain(offer.price);
			position.price = autoPrice;
		}
	}

	/** Fills the holding's price from the current online quote; a price typed by hand is kept. */
	async function fetchPrice(force = false) {
		const f = instrumentForm;
		if (!isTauri() || f.id || f.quoteSource === 'manual' || !f.symbol.trim()) return;
		if (!['etf', 'stock', 'crypto'].includes(f.kind)) return;
		const token = ++priceToken;
		priceBusy = true;
		priceNote = '';
		try {
			const q = await invoke<{ price: string; currency: string; source: string }>('quote_preview', {
				symbol: f.symbol.trim(),
				source: f.quoteSource,
				currency: f.currency
			});
			if (token !== priceToken) return;
			if (q.currency !== f.currency) {
				priceNote = t('position.priceCurrency', { currency: q.currency });
			} else if (force || position.price === '' || position.price === autoPrice) {
				autoPrice = q.price;
				position.price = q.price;
				priceNote = t('position.priceAuto', { source: q.source });
			}
		} catch {
			if (token === priceToken) priceNote = t('position.priceFailed');
		} finally {
			if (token === priceToken) priceBusy = false;
		}
	}

	function pickCatalog() {
		const f = instrumentForm;
		const key = f.catalogKey;
		if (key === 'custom') return;
		if (f.kind === 'etf' || f.kind === 'stock' || f.kind === 'crypto') {
			const c = CATALOG.find((x) => x.id === key && x.kind === f.kind);
			if (!c) return;
			Object.assign(f, {
				symbol: c.symbol,
				name: c.name,
				quoteSource: c.quoteSource,
				fractional: c.fractional,
				currency: f.kind === 'crypto' ? settings.baseCurrency : c.currency
			});
			fetchPrice();
		} else if (f.kind === 'cash') {
			Object.assign(f, { symbol: key, name: `${t('kind.cash')} ${key}`, currency: key });
			position.price ||= '1';
			autoPrice = '1';
		} else if (f.kind === 'deposit') {
			f.currency = key;
			f.startDate ||= today();
			f.termMonths ||= '12';
			position.price ||= '1';
			refreshTerm();
		} else if (f.kind === 'bond') {
			const series = BOND_SERIES.find((x) => x.code === key);
			if (!series) return;
			f.currency = 'PLN';
			f.startDate ||= today();
			f.termMonths = String(series.months);
			position.price ||= '100';
			refreshTerm();
		}
	}

	/** Maturity = start + term; symbol (and a bond's name) follow from it. */
	function refreshTerm() {
		const f = instrumentForm;
		const n = Number(f.termMonths);
		const end = f.startDate && Number.isInteger(n) && n > 0 ? addMonths(f.startDate, n) : null;
		// Retail bonds mature on the 1st of the month; deposits on the same day-of-month.
		f.maturityDate = (f.kind === 'bond' && end ? firstOfMonth(end) : end) || '';
		autoName();
		applyOffer();
	}

	function autoName() {
		const f = instrumentForm;
		if (!timed(f.kind) || f.catalogKey === 'custom' || !f.maturityDate) return;
		const symbol = bondSymbol(f.kind === 'bond' ? f.catalogKey : 'LOK', f.maturityDate);
		if (!symbol) return;
		f.symbol = symbol;
		if (f.kind === 'bond') f.name = `${t('kind.bond')} ${symbol}`;
	}

	function remainingText(maturity: string | null): string {
		if (!maturity) return '';
		const r = remaining(today(), maturity);
		return r ? t('bond.remaining', { y: r.years, m: r.months, d: r.days }) : t('bond.matured');
	}

	const positionQty = $derived(Number(toDecimal(position.quantity)) || 0);
	const positionPrice = $derived(Number(toDecimal(position.price)) || 0);
	// A holding is optional; when a quantity is typed it needs a portfolio, a price and a date.
	const positionValid = $derived(
		!!instrumentForm.id || !position.quantity.trim() || (!!position.portfolioId && positionQty > 0 && positionPrice > 0 && !!position.date)
	);
	const canSave = $derived(
		!!instrumentForm.symbol.trim() &&
			!!instrumentForm.name.trim() &&
			(!timed(instrumentForm.kind) || instrumentForm.catalogKey === 'custom' || !!instrumentForm.maturityDate) &&
			positionValid
	);

	const positionTotal = $derived(positionQty > 0 && positionPrice > 0 ? positionQty * positionPrice : 0);
	const quantityLabel = $derived(
		instrumentForm.kind === 'bond'
			? t('position.quantityBonds')
			: instrumentForm.kind === 'cash' || instrumentForm.kind === 'deposit'
				? t('position.quantityMoney')
				: t('trades.quantity')
	);

	const offerNote = $derived.by(() => {
		const f = instrumentForm;
		if (f.kind !== 'bond' || !f.catalogKey || f.catalogKey === 'custom') return '';
		if (offerSet.skipped.some((x) => x.startsWith(f.catalogKey))) return t('bond.offerSkipped');
		const offer = offerFor(f.catalogKey);
		if (!offer) return '';
		const source = offerSet.source === 'snapshot' ? t('bond.offerSnapshot', { month: offerSet.month ?? '' }) : t('bond.offerOnline');
		const month = offerMonthOf(offer);
		const other = f.startDate && month && f.startDate.slice(0, 7) !== month ? ` ${t('bond.offerOther', { month })}` : '';
		return `${source} ${t('bond.rateNote')}${other}`;
	});

	const saveInstrument = () =>
		run(async () => {
			const f = instrumentForm;
			const existing = instruments.find((i) => i.id === f.id);
			const saved = await db.save<Instrument>('instrument', {
				...existing,
				symbol: f.symbol.trim(),
				name: f.name.trim(),
				kind: f.kind,
				currency: f.currency,
				quoteSource: f.quoteSource,
				fractional: f.fractional,
				interestPercent: hasManualRate(f.kind) && f.interestPercent.trim() ? toDecimal(f.interestPercent) : null,
				capitalization: timed(f.kind) && f.capitalization ? f.capitalization : null,
				startDate: timed(f.kind) && f.startDate ? f.startDate : null,
				maturityDate: timed(f.kind) && f.maturityDate ? f.maturityDate : null
			} as Instrument);
			if (!f.id && position.portfolioId && position.quantity.trim()) {
				await db.save<Trade>('trade', {
					portfolioId: position.portfolioId,
					instrumentId: saved.id,
					date: position.date,
					side: 'buy',
					quantity: toDecimal(position.quantity),
					price: toDecimal(position.price),
					fee: '0'
				} as Trade);
			}
			// A new instrument with an online source gets its quote now instead of at the next scheduled refresh.
			if (!f.id && f.quoteSource !== 'manual') void refreshQuotes();
			const keepPortfolio = position.portfolioId;
			position = { ...emptyPosition(), portfolioId: keepPortfolio };
			autoRate = '';
			autoPrice = '';
			priceToken++;
			priceBusy = false;
			priceNote = '';
			instrumentForm = emptyInstrument();
			await load();
		});

	function inferCatalogKey(i: Instrument): string {
		if (!listed(i.kind)) return '';
		if (i.kind === 'etf' || i.kind === 'crypto') return CATALOG.find((c) => c.kind === i.kind && c.symbol === i.symbol)?.id ?? 'custom';
		if (i.kind === 'bond') return BOND_SERIES.find((s) => i.symbol.startsWith(s.code))?.code ?? 'custom';
		return CURRENCIES.includes(i.currency) ? i.currency : 'custom';
	}

	const editInstrument = (i: Instrument) => {
		instrumentForm = {
			id: i.id,
			symbol: i.symbol,
			name: i.name,
			kind: i.kind,
			currency: i.currency,
			quoteSource: i.quoteSource,
			fractional: i.fractional,
			interestPercent: i.interestPercent ?? '',
			startDate: i.startDate ?? '',
			maturityDate: i.maturityDate ?? '',
			catalogKey: inferCatalogKey(i),
			termMonths: i.startDate && i.maturityDate ? String(termBetween(i.startDate, i.maturityDate) ?? '') : '',
			capitalization: i.capitalization ?? ''
		};
	};

	const pct = (v: string, signed = false) => {
		const n = Number(v);
		return (signed && n > 0 ? '+' : '') + fmtNumber(n, 2);
	};

	/** Manual rate for bonds and cash, exchange figures (12-month return, dividend) for the rest. */
	function rateText(i: Instrument): string {
		if (hasManualRate(i.kind)) {
			return i.interestPercent ? t('instruments.perYear', { rate: pct(i.interestPercent) }) : '—';
		}
		const q = quotes[i.id];
		const parts: string[] = [];
		if (q?.return12mPercent) parts.push(t('instruments.return12m', { value: pct(q.return12mPercent, true) }));
		if (q?.dividendYieldPercent) parts.push(t('instruments.dividend', { value: pct(q.dividendYieldPercent) }));
		return parts.join(' · ') || '—';
	}

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
				{t('portfolio.tolerance')}: {fmtNumber(Number(newPortfolio.tolerancePercent) || 0)}
				<input
					type="range"
					min="0"
					max="20"
					step="0.5"
					value={Number(newPortfolio.tolerancePercent) || 0}
					oninput={(e) => (newPortfolio.tolerancePercent = e.currentTarget.value)}
				/>
			</label>
		</div>
		<h3>{t('targets.optional')}</h3>
		<TargetSliders bind:rows={newTargets} options={instruments.map((i) => ({ id: i.id, label: `${i.symbol} · ${i.name}` }))} />
		<div class="row">
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
							<th>{t('instruments.rate')}</th>
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
									{#if (i.kind === 'cash' || i.kind === 'deposit') && !quotes[i.id]}
										<span class="muted small">{t('instruments.cashPrice')}</span>
									{:else}
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
									{/if}
								</td>
								<td class="small">
									{rateText(i)}
									{#if timed(i.kind) && i.maturityDate}
										<div class="muted">
											{t('instruments.maturity')}: {fmtDay(i.maturityDate)} · {remainingText(i.maturityDate)}
										</div>
									{/if}
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
				{t('instruments.kind')}
				<select bind:value={instrumentForm.kind} onchange={pickKind}>
					{#each INSTRUMENT_KINDS as k (k)}<option value={k}>{t(`kind.${k}`)}</option>{/each}
				</select>
			</label>
			{#if listed(instrumentForm.kind)}
				<div class="field grow">
					<span>{instrumentForm.kind === 'bond' ? t('instruments.pickBond') : t('instruments.catalog')}</span>
					<SearchSelect
						options={options(instrumentForm.kind)}
						value={instrumentForm.catalogKey}
						ariaLabel={instrumentForm.kind === 'bond' ? t('instruments.pickBond') : t('instruments.catalog')}
						placeholder={t('instruments.search')}
						pinned={{ id: 'custom', label: t('instruments.custom') }}
						onchange={(id) => {
							instrumentForm.catalogKey = id;
							pickCatalog();
						}}
					/>
				</div>
			{/if}
			{#if instrumentForm.kind === 'crypto' && instrumentForm.catalogKey && !typedByHand}
				<label class="field">
					{t('instruments.currency')}
					<select bind:value={instrumentForm.currency} onchange={() => fetchPrice(true)}>
						{#each CURRENCIES as c (c)}<option value={c}>{c}</option>{/each}
					</select>
				</label>
			{/if}
		</div>

		{#if typedByHand}
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
		{:else if instrumentForm.catalogKey}
			<p class="muted small">
				{instrumentForm.symbol} · {instrumentForm.currency} · {t(`source.${instrumentForm.quoteSource}`)}
			</p>
		{/if}

		{#if instrumentForm.kind === 'deposit' && instrumentForm.catalogKey && !typedByHand}
			<div class="row">
				<label class="field grow">
					{t('instruments.bank')}
					<input bind:value={instrumentForm.name} />
				</label>
			</div>
		{/if}

		{#if timed(instrumentForm.kind) && instrumentForm.catalogKey}
			<div class="row">
				<label class="field">
					{t('bond.start')}
					<input type="date" bind:value={instrumentForm.startDate} oninput={refreshTerm} />
				</label>
				<label class="field">
					{t('bond.term')}
					<input
						class="short"
						type="number"
						min="1"
						step="1"
						list="term-suggestions"
						bind:value={instrumentForm.termMonths}
						oninput={refreshTerm}
					/>
					<datalist id="term-suggestions">
						{#each DEPOSIT_TERMS as n (n)}<option value={n}></option>{/each}
					</datalist>
				</label>
				<label class="field">
					{t('instruments.maturity')}
					<input type="date" bind:value={instrumentForm.maturityDate} oninput={autoName} />
				</label>
			</div>
			{#if instrumentForm.maturityDate}
				<p class="small">
					{remainingText(instrumentForm.maturityDate)}
					{#if !typedByHand}<span class="muted"> · {t('bond.symbolAuto')}: {instrumentForm.symbol}</span>{/if}
				</p>
			{/if}
		{/if}

		{#if hasManualRate(instrumentForm.kind) && instrumentForm.catalogKey}
			<div class="row">
				<label class="field">
					{t('instruments.interest')}
					<input class="short" inputmode="decimal" bind:value={instrumentForm.interestPercent} />
				</label>
				{#if timed(instrumentForm.kind)}
					<label class="field">
						{t('instruments.capitalization')}
						<select bind:value={instrumentForm.capitalization}>
							<option value="">{t('capitalization.none')}</option>
							{#each CAPITALIZATIONS as c (c)}<option value={c}>{t(`capitalization.${c}`)}</option>{/each}
						</select>
					</label>
				{/if}
			</div>
			<p class="muted small">{offerNote || t('instruments.rateHint.manual')}</p>
		{:else if instrumentForm.kind === 'etf' || instrumentForm.kind === 'crypto'}
			<p class="muted small">{t('instruments.rateHint.auto')}</p>
		{/if}
		{#if !instrumentForm.id}
			<h4>{t('position.title')}</h4>
			<div class="row">
				<label class="field">
					{t('position.portfolio')}
					<select bind:value={position.portfolioId}>
						<option value="">{t('position.none')}</option>
						{#each views as v (v.portfolio.id)}<option value={v.portfolio.id}>{v.portfolio.name}</option>{/each}
					</select>
				</label>
				{#if position.portfolioId}
					<label class="field">
						{quantityLabel}
						<input class="short" inputmode="decimal" bind:value={position.quantity} />
					</label>
					<label class="field">
						{t('trades.price')} ({instrumentForm.currency})
						<input class="short" inputmode="decimal" bind:value={position.price} />
					</label>
					{#if ['etf', 'stock', 'crypto'].includes(instrumentForm.kind) && instrumentForm.quoteSource !== 'manual' && instrumentForm.symbol.trim()}
						<button class="btn" type="button" disabled={priceBusy} onclick={() => fetchPrice(true)}>
							{t('position.priceFetch')}
						</button>
					{/if}
					<label class="field">
						{t('trades.date')}
						<input type="date" bind:value={position.date} />
					</label>
				{/if}
			</div>
			{#if priceBusy}
				<p class="muted small">{t('position.priceLoading')}</p>
			{:else if priceNote}
				<p class="muted small">{priceNote}</p>
			{/if}
			{#if positionTotal}
				<p class="muted small">{t('trades.total')}: {fmtMoney(positionTotal, instrumentForm.currency)}</p>
			{/if}
		{/if}
		<div class="row">
			<button
				class="btn primary"
				disabled={!canSave}
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
