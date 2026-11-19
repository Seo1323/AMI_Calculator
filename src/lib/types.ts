// Mirrors of the Rust models (ami-core). Decimal amounts are strings.
import type { Meta } from '$lib/db';

export type InstrumentKind = 'stock' | 'etf' | 'bond' | 'crypto' | 'cash' | 'deposit' | 'other';
export type QuoteSource = 'yahoo' | 'coinGecko' | 'manual';
export type TradeSide = 'buy' | 'sell';
export type Capitalization = 'monthly' | 'quarterly' | 'annual';
export const CAPITALIZATIONS: Capitalization[] = ['monthly', 'quarterly', 'annual'];

export const INSTRUMENT_KINDS: InstrumentKind[] = ['etf', 'stock', 'bond', 'crypto', 'cash', 'deposit', 'other'];
export const QUOTE_SOURCES: QuoteSource[] = ['yahoo', 'coinGecko', 'manual'];
export const CURRENCIES = ['PLN', 'EUR', 'USD', 'GBP', 'CHF'];

export interface Instrument extends Meta {
	symbol: string;
	name: string;
	kind: InstrumentKind;
	currency: string;
	quoteSource: QuoteSource;
	fractional: boolean;
	/** Annual rate in percent, entered by hand (bonds, cash). */
	interestPercent: string | null;
	/** Interest capitalization of a bond or deposit; null: none. */
	capitalization: Capitalization | null;
	/** Bond/deposit start, YYYY-MM-DD; maturity is start plus the term. */
	startDate: string | null;
	/** Bond/deposit maturity, YYYY-MM-DD. */
	maturityDate: string | null;
}

export interface Portfolio extends Meta {
	name: string;
	baseCurrency: string;
	tolerancePercent: string;
}

export interface Trade extends Meta {
	portfolioId: string;
	instrumentId: string;
	date: string;
	side: TradeSide;
	quantity: string;
	price: string;
	fee: string;
}

export interface FxRate {
	currency: string;
	ratePln: string;
	source: string;
	fetchedAt: string;
}

export interface PositionView {
	instrument: Instrument;
	quantity: string;
	avgCost: string | null;
	price: string | null;
	priceAt: string | null;
	priceSource: string | null;
	value: string | null;
	gain: string | null;
	realized: string;
	targetPercent: string;
	currentPercent: string | null;
}

/** Share of one asset kind in a portfolio's value (value in the base currency, percent of the total). */
export interface KindShare {
	kind: InstrumentKind;
	value: string;
	percent: string;
}

export interface PortfolioView {
	portfolio: Portfolio;
	positions: PositionView[];
	totalValue: string;
	totalGain: string;
	targetsSum: string;
	missingPrices: string[];
	missingRates: string[];
	maxDeviation: string | null;
	needsRebalance: boolean;
	kindShares: KindShare[];
}

/** Accepts "1 234,56" as well as "1234.56". */
export function toDecimal(input: string | number): string {
	const s = String(input).replace(/[\s ]/g, '').replace(',', '.');
	return s === '' ? '0' : s;
}

export function today(): string {
	const d = new Date();
	return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
}

// ---------- Dashboard ----------

export interface AllocationSlice {
	instrumentId: string;
	symbol: string;
	currentPercent: string;
	targetPercent: string;
}

export interface PortfolioTile {
	id: string;
	name: string;
	value: string | null;
	gain: string | null;
	maxDeviation: string | null;
	needsRebalance: boolean;
	complete: boolean;
	allocation: AllocationSlice[];
}

export interface Dashboard {
	currency: string;
	totalValue: string;
	totalGain: string;
	missingRates: string[];
	portfolios: PortfolioTile[];
}
