import type { InstrumentKind, QuoteSource } from '$lib/types';

export interface CatalogItem {
	id: string;
	kind: InstrumentKind;
	/** Symbol at the quote source (Yahoo ticker or CoinGecko id). */
	symbol: string;
	name: string;
	currency: string;
	quoteSource: QuoteSource;
	fractional: boolean;
}

const etf = (symbol: string, name: string, currency: string): CatalogItem => ({
	id: symbol,
	kind: 'etf',
	symbol,
	name,
	currency,
	quoteSource: 'yahoo',
	fractional: false
});

const stock = (symbol: string, name: string, currency: string): CatalogItem => ({
	id: symbol,
	kind: 'stock',
	symbol,
	name,
	currency,
	quoteSource: 'yahoo',
	fractional: false
});

const crypto = (id: string, name: string): CatalogItem => ({
	id,
	kind: 'crypto',
	symbol: id,
	name,
	currency: 'PLN',
	quoteSource: 'coinGecko',
	fractional: true
});

/** Tickers were checked against Yahoo Finance / CoinGecko; anything else is entered as "custom". */
export const CATALOG: CatalogItem[] = [
	etf('VWCE.DE', 'Vanguard FTSE All-World (Acc)', 'EUR'),
	etf('EUNL.DE', 'iShares Core MSCI World (Acc)', 'EUR'),
	etf('SXR8.DE', 'iShares Core S&P 500 (Acc)', 'EUR'),
	etf('VUAA.DE', 'Vanguard S&P 500 (Acc)', 'EUR'),
	etf('IS3N.DE', 'iShares Core MSCI EM IMI (Acc)', 'EUR'),
	etf('VHYL.AS', 'Vanguard FTSE All-World High Dividend Yield (Dist)', 'EUR'),
	etf('EUNA.DE', 'iShares Core Global Aggregate Bond EUR Hedged (Acc)', 'EUR'),
	etf('ETFBW20TR.WA', 'Beta ETF WIG20TR', 'PLN'),
	etf('ETFBM40TR.WA', 'Beta ETF mWIG40TR', 'PLN'),
	etf('ETFBS80TR.WA', 'Beta ETF sWIG80TR', 'PLN'),
	etf('ETFBTBSP.WA', 'Beta ETF Obligacji Skarbowych TBSP', 'PLN'),
	etf('ETFBSPXPL.WA', 'Beta ETF S&P 500 PLN-Hedged', 'PLN'),
	stock('PKN.WA', 'Orlen', 'PLN'),
	stock('PKO.WA', 'PKO Bank Polski', 'PLN'),
	stock('PZU.WA', 'PZU', 'PLN'),
	stock('PEO.WA', 'Bank Pekao', 'PLN'),
	stock('KGH.WA', 'KGHM Polska Miedź', 'PLN'),
	stock('PGE.WA', 'PGE Polska Grupa Energetyczna', 'PLN'),
	stock('DNP.WA', 'Dino Polska', 'PLN'),
	stock('CDR.WA', 'CD Projekt', 'PLN'),
	stock('LPP.WA', 'LPP', 'PLN'),
	stock('ALE.WA', 'Allegro', 'PLN'),
	stock('MBK.WA', 'mBank', 'PLN'),
	stock('KTY.WA', 'Grupa Kęty', 'PLN'),
	stock('OPL.WA', 'Orange Polska', 'PLN'),
	stock('CPS.WA', 'Cyfrowy Polsat', 'PLN'),
	stock('JSW.WA', 'Jastrzębska Spółka Węglowa', 'PLN'),
	stock('TPE.WA', 'Tauron Polska Energia', 'PLN'),
	stock('ACP.WA', 'Asseco Poland', 'PLN'),
	stock('BDX.WA', 'Budimex', 'PLN'),
	stock('PCO.WA', 'Pepco Group', 'PLN'),
	stock('KRU.WA', 'Kruk', 'PLN'),
	stock('XTB.WA', 'XTB', 'PLN'),
	stock('AAPL', 'Apple', 'USD'),
	stock('MSFT', 'Microsoft', 'USD'),
	stock('NVDA', 'NVIDIA', 'USD'),
	stock('GOOGL', 'Alphabet (Google)', 'USD'),
	stock('AMZN', 'Amazon', 'USD'),
	stock('META', 'Meta Platforms', 'USD'),
	stock('TSLA', 'Tesla', 'USD'),
	stock('JPM', 'JPMorgan Chase', 'USD'),
	stock('KO', 'Coca-Cola', 'USD'),
	stock('NESN.SW', 'Nestlé', 'CHF'),
	stock('SAP.DE', 'SAP', 'EUR'),
	crypto('bitcoin', 'Bitcoin'),
	crypto('ethereum', 'Ethereum'),
	crypto('solana', 'Solana'),
	crypto('ripple', 'XRP'),
	crypto('cardano', 'Cardano'),
	crypto('tether', 'Tether (USDT)')
];

export const catalogFor = (kind: InstrumentKind) => CATALOG.filter((c) => c.kind === kind);

/** Typical deposit terms in months, offered as suggestions (any number can be typed). */
export const DEPOSIT_TERMS = [1, 3, 6, 12, 24, 36];
