// Performance mode: detecting slow painting. Pure helpers are dependency-free so `node --test` can run them.
//
// Where WebKit has to paint in software (e.g. native Wayland without GL), the soft glow, shadows,
// gradients and transitions dominate the frame time. Measured on such a setup: a long page scrolled
// at 3 fps with them and 21 fps without. Performance mode switches them off.

export type PerfMode = 'auto' | 'on' | 'off';
export const PERF_MODES: PerfMode[] = ['auto', 'on', 'off'];

/** A smooth 60 Hz frame is 16.7 ms; a median above this means the page is not keeping up. */
export const SLOW_FRAME_MS = 45;
/** Frames needed for a verdict (at 3 fps these take about 2 seconds of scrolling). */
export const MIN_FRAMES = 5;
/** A gap this long is a pause (hidden window, breakpoint), not slow painting. */
const PAUSE_MS = 1000;

export function median(xs: number[]): number {
	if (xs.length === 0) return 0;
	const s = [...xs].sort((a, b) => a - b);
	const mid = Math.floor(s.length / 2);
	return s.length % 2 ? s[mid] : (s[mid - 1] + s[mid]) / 2;
}

/** Whether these gaps between consecutive animation frames (ms) show slow painting. */
export function isSlow(gaps: number[]): boolean {
	if (gaps.length < MIN_FRAMES || gaps.some((g) => g > PAUSE_MS)) return false;
	return median(gaps) > SLOW_FRAME_MS;
}

export const asPerfMode = (v: unknown): PerfMode => (PERF_MODES.includes(v as PerfMode) ? (v as PerfMode) : 'auto');

/** Performance mode is on when chosen, or in auto mode once slow painting was seen. */
export const perfActive = (mode: PerfMode, slowSeen: boolean): boolean => mode === 'on' || (mode === 'auto' && slowSeen);

/**
 * Watches the first scrolling and calls `onSlow` once if the frames come too slowly. Returns a
 * function that stops watching. Browser only.
 */
export function watchScrollPace(onSlow: () => void): () => void {
	let sampling = false;
	const onScroll = () => {
		if (sampling || document.visibilityState !== 'visible') return;
		sampling = true;
		const gaps: number[] = [];
		const t0 = performance.now();
		let last = t0;
		const tick = (t: number) => {
			gaps.push(t - last);
			last = t;
			if (gaps.length > MIN_FRAMES * 2 || t - t0 > 2500) {
				sampling = false;
				if (isSlow(gaps.slice(1))) {
					window.removeEventListener('scroll', onScroll);
					onSlow();
				}
				return;
			}
			requestAnimationFrame(tick);
		};
		requestAnimationFrame(tick);
	};
	window.addEventListener('scroll', onScroll, { passive: true });
	return () => window.removeEventListener('scroll', onScroll);
}
