// Geometry of a donut/pie chart. Pure and dependency-free so `node --test` can run it directly.
// Angles are in radians, 0 at 12 o'clock, growing clockwise.

export interface PieInput {
	id: string;
	value: number;
}

export interface Slice {
	id: string;
	value: number;
	/** Fraction of the whole, 0..1. */
	share: number;
	start: number;
	end: number;
}

const TAU = Math.PI * 2;

/** Slices in input order. Entries that are not a positive finite number are dropped. */
export function layout(items: PieInput[]): Slice[] {
	const valid = items.filter((i) => Number.isFinite(i.value) && i.value > 0);
	const total = valid.reduce((sum, i) => sum + i.value, 0);
	if (total <= 0) return [];
	let angle = 0;
	return valid.map((i, n) => {
		const share = i.value / total;
		const start = angle;
		// The last slice closes the circle exactly, so rounding can never leave a gap.
		const end = n === valid.length - 1 ? TAU : angle + share * TAU;
		angle = end;
		return { id: i.id, value: i.value, share, start, end };
	});
}

const num = (n: number) => String(Math.round(n * 1000) / 1000);

/** Point at `angle` on the circle of radius `r` around (cx, cy). */
export function polar(cx: number, cy: number, r: number, angle: number): [number, number] {
	return [cx + r * Math.sin(angle), cy - r * Math.cos(angle)];
}

/** One donut segment from `start` to `end` between the radii `inner` and `outer`. */
export function arcPath(cx: number, cy: number, outer: number, inner: number, start: number, end: number): string {
	// A single slice covers the whole ring; one arc cannot draw a full circle, so split it in two halves.
	if (end - start >= TAU - 1e-6) {
		return arcPath(cx, cy, outer, inner, 0, Math.PI) + ' ' + arcPath(cx, cy, outer, inner, Math.PI, TAU);
	}
	const large = end - start > Math.PI ? 1 : 0;
	const [ox0, oy0] = polar(cx, cy, outer, start);
	const [ox1, oy1] = polar(cx, cy, outer, end);
	const [ix1, iy1] = polar(cx, cy, inner, end);
	const [ix0, iy0] = polar(cx, cy, inner, start);
	return (
		`M ${num(ox0)} ${num(oy0)} A ${outer} ${outer} 0 ${large} 1 ${num(ox1)} ${num(oy1)} ` +
		`L ${num(ix1)} ${num(iy1)} A ${inner} ${inner} 0 ${large} 0 ${num(ix0)} ${num(iy0)} Z`
	);
}
