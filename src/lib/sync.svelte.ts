import { open, save } from '@tauri-apps/plugin-dialog';
import { hooks, invoke, isTauri } from '$lib/tauri';

// Sync through one shared `.ami` file (see crates/ami-core/src/sync.rs). On desktop the Rust side
// reads and writes it by path. On Android the file is a content:// URI that only the Kotlin bridge
// (scripts/android/MainActivity.kt) can keep access to across restarts.

const KEY = 'ami.sync';
const FILTERS = [{ name: 'AMI Calculator', extensions: ['ami'] }];
const DEBOUNCE_MS = 4000;
const EVERY_MS = 5 * 60_000;

interface Stored {
	/** A file path, or a content:// URI on Android. */
	target: string | null;
	/** What the user recognises: the path, or the file name on Android. */
	label: string;
	/** Seals the file; the same on every device. Kept like the database: readable by this device only. */
	passphrase: string;
}

function read(): Stored | null {
	try {
		return JSON.parse(localStorage.getItem(KEY) ?? 'null');
	} catch {
		return null;
	}
}

function remember() {
	try {
		const v: Stored = { target: sync.target, label: sync.label, passphrase: sync.passphrase };
		localStorage.setItem(KEY, JSON.stringify(v));
	} catch {
		/* storage unavailable: the choice then only lasts for this session */
	}
}

const saved = read();

/** `version` bumps when the file brought changes, so the open page reloads. */
export const sync = $state({
	target: saved?.target ?? null,
	label: saved?.label ?? '',
	passphrase: saved?.passphrase ?? '',
	busy: false,
	lastAt: null as Date | null,
	lastStats: null as { inserted: number; updated: number } | null,
	error: null as unknown,
	version: 0
});

export function setPassphrase(value: string) {
	sync.passphrase = value;
	remember();
}

// ---------- Android bridge ----------

interface Bridge {
	call(id: number, op: string, uri: string, text: string): void;
}
declare global {
	interface Window {
		AmiSync?: Bridge;
		__amiSyncResult?: (id: number, json: string) => void;
	}
}

const waiting = new Map<number, (r: { ok: boolean; error?: string; uri?: string; name?: string; text?: string }) => void>();
let nextId = 1;

if (typeof window !== 'undefined') {
	window.__amiSyncResult = (id, json) => {
		waiting.get(id)?.(JSON.parse(json));
		waiting.delete(id);
	};
}

const onAndroid = () => typeof window !== 'undefined' && !!window.AmiSync;

async function bridge(op: string, uri = '', text = '') {
	const id = nextId++;
	const reply = new Promise<{ ok: boolean; error?: string; uri?: string; name?: string; text?: string }>((resolve) =>
		waiting.set(id, resolve)
	);
	window.AmiSync!.call(id, op, uri, text);
	const r = await reply;
	if (!r.ok) throw { code: 'io', message: r.error ?? 'file access failed' };
	return r;
}

// ---------- choosing the file ----------

/** `create`: a new file (first device). Otherwise an existing one (every further device). */
export async function connect(create: boolean) {
	if (!isTauri()) return;
	let chosen: { target: string; label: string } | null = null;
	if (onAndroid()) {
		const r = await bridge('pick', create ? 'create' : 'open');
		if (r.uri) chosen = { target: r.uri, label: r.name ?? r.uri };
	} else {
		const path = create
			? await save({ defaultPath: 'ami-sync.ami', filters: FILTERS })
			: await open({ multiple: false, directory: false, filters: FILTERS });
		if (path) chosen = { target: path, label: path };
	}
	if (!chosen) return;
	sync.target = chosen.target;
	sync.label = chosen.label;
	sync.error = null;
	remember();
	await syncNow();
}

export function disconnect() {
	sync.target = null;
	sync.label = '';
	sync.lastAt = null;
	sync.lastStats = null;
	sync.error = null;
	remember();
}

// ---------- syncing ----------

export async function syncNow() {
	const target = sync.target;
	if (!target || sync.busy || !isTauri()) return;
	sync.busy = true;
	const passphrase = sync.passphrase || null;
	try {
		let stats: { inserted: number; updated: number };
		if (onAndroid()) {
			const text = (await bridge('read', target)).text ?? '';
			const out = await invoke<{ stats: typeof stats; outgoing: string | null }>('sync_text', {
				contents: text,
				passphrase
			});
			if (out.outgoing) await bridge('write', target, out.outgoing);
			stats = out.stats;
		} else {
			stats = (await invoke<{ stats: typeof stats }>('sync_file', { path: target, passphrase })).stats;
		}
		sync.lastAt = new Date();
		sync.lastStats = stats;
		sync.error = null;
		if (stats.inserted + stats.updated > 0) sync.version++;
	} catch (e) {
		sync.error = e;
	} finally {
		sync.busy = false;
	}
}

let timer: ReturnType<typeof setTimeout> | undefined;

/** Called after local changes; waits a moment so a burst of edits is one sync. */
export function scheduleSync() {
	if (!sync.target) return;
	clearTimeout(timer);
	timer = setTimeout(syncNow, DEBOUNCE_MS);
}

/** Syncs now and then: when the app opens, comes back to the front, goes away, and every few minutes. */
export function startSync(): () => void {
	hooks.mutated = scheduleSync;
	const onVisible = () => {
		clearTimeout(timer);
		syncNow(); // also when hiding: pushes the last edits before Android may stop the app
	};
	document.addEventListener('visibilitychange', onVisible);
	const every = setInterval(syncNow, EVERY_MS);
	syncNow();
	return () => {
		hooks.mutated = undefined;
		document.removeEventListener('visibilitychange', onVisible);
		clearInterval(every);
		clearTimeout(timer);
	};
}
