// Target allocation sliders: the maths of keeping the shares at exactly 100 %.
// Pure and dependency-free so `node --test` can run it directly. Amounts are integers in
// hundredths of a percent (10000 = 100 %), so no floating-point error can creep into the sum.

export const WHOLE = 10_000;

export interface TargetRow {
	id: string;
	/** Hundredths of a percent. */
	units: number;
	/** A locked row keeps its share while the others are redistributed. */
	locked: boolean;
}

/** "12,5" or "12.5" -> 1250 units; anything unreadable or outside 0-100 gives null. */
export function toUnits(input: string | number): number | null {
	const text = String(input).replace(/[\s ]/g, '').replace(',', '.');
	const n = text === '' ? NaN : Number(text); // Number('') is 0, but an empty field is not a value
	if (!Number.isFinite(n) || n < 0 || n > 100) return null;
	return Math.round(n * 100);
}

export const fromUnits = (units: number): number => units / 100;

export const total = (rows: TargetRow[]): number => rows.reduce((s, r) => s + r.units, 0);

/**
 * Splits `amount` units over `weights` proportionally (largest-remainder method), so the parts are
 * integers and add up to exactly `amount`. All weights zero means an equal split.
 */
export function split(amount: number, weights: number[]): number[] {
	if (weights.length === 0) return [];
	const sum = weights.reduce((s, w) => s + w, 0);
	const w = sum > 0 ? weights : weights.map(() => 1);
	const wsum = sum > 0 ? sum : w.length;
	const exact = w.map((x) => (x / wsum) * amount);
	const parts = exact.map(Math.floor);
	let left = amount - parts.reduce((s, p) => s + p, 0);
	const order = exact.map((e, i) => ({ i, frac: e - Math.floor(e) })).sort((a, b) => b.frac - a.frac || a.i - b.i);
	for (let k = 0; left > 0; k = (k + 1) % order.length, left--) parts[order[k].i]++;
	return parts;
}

/**
 * Moves row `id` to `units`. The others that are not locked share what is left, in proportion to
 * their current values. The value is limited to what the locked rows leave free; with no other
 * unlocked row there is nobody to take the difference, so the row is simply the rest of the 100 %.
 */
export function setUnits(rows: TargetRow[], id: string, units: number): TargetRow[] {
	const row = rows.find((r) => r.id === id);
	if (!row || row.locked) return rows;
	const lockedOthers = rows.filter((r) => r.id !== id && r.locked).reduce((s, r) => s + r.units, 0);
	const others = rows.filter((r) => r.id !== id && !r.locked);
	const room = WHOLE - lockedOthers;
	const value = others.length === 0 ? room : Math.min(Math.max(0, Math.round(units)), room);
	const shares = split(room - value, others.map((r) => r.units));
	let n = 0;
	return rows.map((r) => {
		if (r.id === id) return { ...r, units: value };
		return r.locked ? r : { ...r, units: shares[n++] };
	});
}

/** Adds a row. The first row takes the whole 100 %; later ones start at 0 so nothing else moves. */
export function addRow(rows: TargetRow[], id: string): TargetRow[] {
	if (rows.some((r) => r.id === id)) return rows;
	return [...rows, { id, units: rows.length === 0 ? WHOLE : 0, locked: false }];
}

/** Removes a row; its share goes to the unlocked rows in proportion (nobody else: it is lost). */
export function removeRow(rows: TargetRow[], id: string): TargetRow[] {
	const gone = rows.find((r) => r.id === id);
	const rest = rows.filter((r) => r.id !== id);
	if (!gone || rest.length === 0) return rest;
	const free = rest.filter((r) => !r.locked);
	if (free.length === 0) return rest;
	const shares = split(free.reduce((s, r) => s + r.units, 0) + gone.units, free.map((r) => r.units));
	let n = 0;
	return rest.map((r) => (r.locked ? r : { ...r, units: shares[n++] }));
}

export function toggleLock(rows: TargetRow[], id: string): TargetRow[] {
	return rows.map((r) => (r.id === id ? { ...r, locked: !r.locked } : r));
}

/** Equal shares for the unlocked rows over whatever the locked ones leave. */
export function equalSplit(rows: TargetRow[]): TargetRow[] {
	const free = rows.filter((r) => !r.locked);
	if (free.length === 0) return rows;
	const room = WHOLE - rows.filter((r) => r.locked).reduce((s, r) => s + r.units, 0);
	const shares = split(room, free.map(() => 1));
	let n = 0;
	return rows.map((r) => (r.locked ? r : { ...r, units: shares[n++] }));
}

/** Rows from saved targets (percent strings). The sum is not forced; saved data is shown as it is. */
export function fromSaved(saved: { id: string; percent: string }[]): TargetRow[] {
	return saved.flatMap((s) => {
		const units = toUnits(s.percent);
		return units === null || units === 0 ? [] : [{ id: s.id, units, locked: false }];
	});
}
