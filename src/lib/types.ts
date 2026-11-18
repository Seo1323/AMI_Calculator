// Mirrors of the Rust models (ami-core). Decimal amounts are strings.
import type { Meta } from '$lib/db';

export type InstrumentKind = 'stock' | 'etf' | 'bond' | 'crypto' | 'cash' | 'other';
export type QuoteSource = 'stooq' | 'coinGecko' | 'manual';
export type TradeSide = 'buy' | 'sell';

export const INSTRUMENT_KINDS: InstrumentKind[] = ['etf', 'stock', 'bond', 'crypto', 'cash', 'other'];
export const QUOTE_SOURCES: QuoteSource[] = ['manual', 'stooq', 'coinGecko'];
export const CURRENCIES = ['PLN', 'EUR', 'USD', 'GBP', 'CHF'];

export interface Instrument extends Meta {
	symbol: string;
	name: string;
	kind: InstrumentKind;
	currency: string;
	quoteSource: QuoteSource;
	fractional: boolean;
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

// ---------- Budget ----------

export type AccountKind = 'cash' | 'checking' | 'savings' | 'creditCard' | 'brokerage' | 'other';
export const ACCOUNT_KINDS: AccountKind[] = ['checking', 'savings', 'cash', 'creditCard', 'brokerage', 'other'];
export type CategoryKind = 'income' | 'expense';

export interface Account extends Meta {
	name: string;
	kind: AccountKind;
	currency: string;
	openingBalance: string;
	archived: boolean;
	color: string | null;
}

export interface Category extends Meta {
	name: string;
	kind: CategoryKind;
	parentId: string | null;
	color: string | null;
	icon: string | null;
}

export interface Transaction extends Meta {
	accountId: string;
	date: string;
	amount: string;
	categoryId: string | null;
	transferPairId: string | null;
	note: string;
}

export interface AccountBalance {
	account: Account;
	balance: string;
}

export interface CategoryLine {
	category: Category;
	amount: string;
	limit: string | null;
}

export interface MonthSummary {
	month: string;
	currency: string;
	income: string;
	expense: string;
	net: string;
	uncategorizedExpense: string;
	uncategorizedIncome: string;
	categories: CategoryLine[];
	missingRates: string[];
}

/** "YYYY-MM-01" for the month containing `d`, shifted by `offset` months. */
export function monthKey(d: Date, offset = 0): string {
	const m = new Date(d.getFullYear(), d.getMonth() + offset, 1);
	return `${m.getFullYear()}-${String(m.getMonth() + 1).padStart(2, '0')}-01`;
}
