import { watchScrollPace } from '$lib/perf';

const KEY = 'ami.perf.slow';

function read(): boolean {
	try {
		return localStorage.getItem(KEY) === '1';
	} catch {
		return false;
	}
}

/** Slow painting seen on this machine (remembered, so the next start is smooth from the first frame). */
export const perf = $state({ slowSeen: read() });

export function markSlow() {
	perf.slowSeen = true;
	try {
		localStorage.setItem(KEY, '1');
	} catch {
		/* storage unavailable: the verdict then only lasts for this session */
	}
}

export function clearSlow() {
	perf.slowSeen = false;
	try {
		localStorage.removeItem(KEY);
	} catch {
		/* storage unavailable */
	}
}

/** Starts watching the first scrolling; returns a stop function. */
export const startWatching = () => watchScrollPace(markSlow);
