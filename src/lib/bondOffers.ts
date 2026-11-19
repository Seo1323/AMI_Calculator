import { invoke, isTauri } from '$lib/tauri';
import type { Capitalization } from '$lib/types';

/** One series of the monthly offer of Polish retail treasury bonds (obligacjeskarbowe.pl). */
export interface BondOffer {
	series: string;
	/** Series + maturity month and year, e.g. EDO1136. */
	symbol: string;
	/** Annual rate of the FIRST interest period, percent. Later periods depend on inflation or the NBP rate. */
	firstPeriodRatePercent: string;
	capitalization: Capitalization | null;
	/** Price of one bond in PLN. */
	price: string;
}

export interface OfferSet {
	offers: BondOffer[];
	source: 'online' | 'cache' | 'snapshot';
	/** YYYY-MM for the built-in snapshot. */
	month?: string;
}

/** Offer of November 2026, read from obligacjeskarbowe.pl. Used when no fresh offer can be downloaded. */
const SNAPSHOT: BondOffer[] = [
	{ series: 'OTS', symbol: 'OTS0227', firstPeriodRatePercent: '2.00', capitalization: null, price: '100.00' },
	{ series: 'ROR', symbol: 'ROR1127', firstPeriodRatePercent: '4.00', capitalization: null, price: '100.00' },
	{ series: 'DOR', symbol: 'DOR1128', firstPeriodRatePercent: '4.15', capitalization: null, price: '100.00' },
	{ series: 'TOS', symbol: 'TOS1129', firstPeriodRatePercent: '4.40', capitalization: 'annual', price: '100.00' },
	{ series: 'COI', symbol: 'COI1130', firstPeriodRatePercent: '4.75', capitalization: null, price: '100.00' },
	{ series: 'ROS', symbol: 'ROS1132', firstPeriodRatePercent: '5.00', capitalization: 'annual', price: '100.00' },
	{ series: 'EDO', symbol: 'EDO1136', firstPeriodRatePercent: '5.35', capitalization: 'annual', price: '100.00' },
	{ series: 'ROD', symbol: 'ROD1138', firstPeriodRatePercent: '5.60', capitalization: 'annual', price: '100.00' }
];

export const SNAPSHOT_SET: OfferSet = { offers: SNAPSHOT, source: 'snapshot', month: '2026-11' };

const KEY = 'ami.bondOffers.v1';
const MAX_AGE_MS = 12 * 3600 * 1000;

function readCache(): { offers: BondOffer[]; at: number } | null {
	try {
		const raw = JSON.parse(localStorage.getItem(KEY) ?? 'null');
		return raw && Array.isArray(raw.offers) && typeof raw.at === 'number' ? raw : null;
	} catch {
		return null;
	}
}

/** Fresh cache, else the live offer (desktop app), else a stale cache, else the built-in snapshot. */
export async function loadBondOffers(): Promise<OfferSet> {
	const cached = readCache();
	if (cached && Date.now() - cached.at < MAX_AGE_MS) return { offers: cached.offers, source: 'cache' };
	if (isTauri()) {
		try {
			const offers = await invoke<BondOffer[]>('bond_offers');
			if (offers.length) {
				try {
					localStorage.setItem(KEY, JSON.stringify({ offers, at: Date.now() }));
				} catch {
					/* storage unavailable */
				}
				return { offers, source: 'online' };
			}
		} catch {
			/* offline or the page changed: fall through */
		}
	}
	return cached ? { offers: cached.offers, source: 'cache' } : SNAPSHOT_SET;
}
