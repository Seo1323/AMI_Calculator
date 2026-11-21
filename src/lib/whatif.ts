// "What if I deposit X?": amounts typed by the user and a one-line summary of each simulated result.
import type { RebalanceResult } from './tauri';

export const MAX_AMOUNTS = 6;

/** "500; 1 000; 2000,5" -> ["500", "1000", "2000.5"]: positive, unique, at most MAX_AMOUNTS, in typed order. */
export function parseAmounts(text: string): string[] {
	const out: string[] = [];
	for (const part of text.split(';')) {
		const s = part.replace(/[\s ]/g, '').replace(',', '.');
		const n = Number(s);
		if (s !== '' && Number.isFinite(n) && n > 0 && !out.some((o) => Number(o) === n)) out.push(s);
	}
	return out.slice(0, MAX_AMOUNTS);
}

export interface Summary {
	maxDeviationAfter: number;
	cashLeft: number;
	bought: number;
	sold: number;
}

export function summarize(r: RebalanceResult): Summary {
	let maxDeviationAfter = 0;
	let bought = 0;
	let sold = 0;
	for (const p of r.positions) {
		maxDeviationAfter = Math.max(maxDeviationAfter, Math.abs(Number(p.deviationAfter)));
		const v = Number(p.tradeValue);
		if (v > 0) bought += v;
		else sold -= v;
	}
	return { maxDeviationAfter, cashLeft: Number(r.cashLeft), bought, sold };
}
