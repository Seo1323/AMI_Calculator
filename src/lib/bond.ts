// Date arithmetic for bonds. Pure and dependency-free so `node --test` can run it directly.
// Dates are ISO strings (YYYY-MM-DD) handled in UTC, so time zones and DST never shift a day.

/** Polish retail treasury bonds: series code and term in months. Rates change monthly, so they stay manual. */
export const BOND_SERIES: { code: string; months: number }[] = [
	{ code: 'OTS', months: 3 },
	{ code: 'ROR', months: 12 },
	{ code: 'DOR', months: 24 },
	{ code: 'TOS', months: 36 },
	{ code: 'COI', months: 48 },
	{ code: 'ROS', months: 72 },
	{ code: 'EDO', months: 120 },
	{ code: 'ROD', months: 144 }
];

const ISO = /^(\d{4})-(\d{2})-(\d{2})$/;

function parse(iso: string): { y: number; m: number; d: number } | null {
	const m = ISO.exec(iso);
	if (!m) return null;
	const [y, mo, d] = [Number(m[1]), Number(m[2]), Number(m[3])];
	// Reject impossible dates such as 2026-02-30.
	return daysInMonth(y, mo) >= d && mo >= 1 && mo <= 12 && d >= 1 ? { y, m: mo, d } : null;
}

const pad = (n: number, w = 2) => String(n).padStart(w, '0');

/** Days in month `m` (1-12) of year `y`. */
export function daysInMonth(y: number, m: number): number {
	return new Date(Date.UTC(y, m, 0)).getUTCDate();
}

/** `iso` plus `months`; a day that does not exist in the target month moves to its last day. */
export function addMonths(iso: string, months: number): string | null {
	const p = parse(iso);
	if (!p || !Number.isInteger(months)) return null;
	const total = p.y * 12 + (p.m - 1) + months;
	const y = Math.floor(total / 12);
	const m = (total % 12) + 1;
	return `${pad(y, 4)}-${pad(m)}-${pad(Math.min(p.d, daysInMonth(y, m)))}`;
}

const dayNumber = (iso: string) => {
	const p = parse(iso)!;
	return Date.UTC(p.y, p.m - 1, p.d) / 86_400_000;
};

/**
 * Calendar time from `fromIso` to `toIso`, or null when the target is today or already past.
 * Whole months are counted with `addMonths` (so 31 Jan + 1 month = 28 Feb) and the rest in days.
 */
export function remaining(fromIso: string, toIso: string): { years: number; months: number; days: number } | null {
	const a = parse(fromIso);
	const b = parse(toIso);
	if (!a || !b || toIso <= fromIso) return null;
	let total = (b.y - a.y) * 12 + (b.m - a.m);
	let anchor = addMonths(fromIso, total)!;
	if (anchor > toIso) {
		total -= 1;
		anchor = addMonths(fromIso, total)!;
	}
	return { years: Math.floor(total / 12), months: total % 12, days: dayNumber(toIso) - dayNumber(anchor) };
}

/** Polish convention: series + maturity month and two-digit year, e.g. EDO0436. */
export function bondSymbol(series: string, maturityIso: string): string | null {
	const p = parse(maturityIso);
	return p ? `${series}${pad(p.m)}${pad(p.y % 100)}` : null;
}

/** Whole months `n` such that `start` + n months = `end`, or null when the dates do not line up. */
export function termBetween(startIso: string, endIso: string): number | null {
	const a = parse(startIso);
	const b = parse(endIso);
	if (!a || !b) return null;
	const guess = (b.y - a.y) * 12 + (b.m - a.m);
	for (const n of [guess, guess + 1, guess - 1]) {
		if (n > 0 && addMonths(startIso, n) === endIso) return n;
	}
	return null;
}

/** First day of the month of `iso`: Polish retail bonds mature on the 1st. */
export function firstOfMonth(iso: string): string | null {
	const p = parse(iso);
	return p ? `${pad(p.y, 4)}-${pad(p.m)}-01` : null;
}

/**
 * Month (YYYY-MM) in which an offered series is sold, from its symbol (series + maturity MMYY)
 * and term: EDO1136 with 120 months is sold in 2026-11.
 */
export function offerMonth(symbol: string, termMonths: number): string | null {
	const m = /^[A-Z]{3}(\d{2})(\d{2})$/.exec(symbol);
	if (!m) return null;
	const matured = addMonths(`20${m[2]}-${m[1]}-01`, -termMonths);
	return matured ? matured.slice(0, 7) : null;
}

/**
 * A calendar date (YYYY-MM-DD) as a local Date at midnight, built from its parts. `new Date('2036-11-01')`
 * would be UTC midnight, which shows as 01:00 or 02:00 in Europe and as the previous day west of Greenwich.
 */
export function localDate(iso: string): Date | null {
	const p = parse(iso);
	return p ? new Date(p.y, p.m - 1, p.d) : null;
}
