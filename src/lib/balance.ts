// How far out of balance a portfolio is, as the tilt of the scale glyph.
export const MAX_TILT = 14;
const FULL_TILT_PP = 30; // a position this many points off tilts the beam all the way

/** Signed deviation (current - target, in points) of the position that is furthest from its target. */
export function lean(rows: { current: number | null; target: number }[]): number {
	let worst = 0;
	for (const r of rows) {
		if (r.current === null || !Number.isFinite(r.current)) continue;
		const d = r.current - r.target;
		if (Math.abs(d) > Math.abs(worst)) worst = d;
	}
	return worst;
}

/** Beam angle in degrees: level when within tolerance, otherwise leaning towards the heavy side. */
export function tilt(deviation: number, balanced: boolean): number {
	if (balanced) return 0;
	return Math.max(-1, Math.min(1, deviation / FULL_TILT_PP)) * MAX_TILT;
}
