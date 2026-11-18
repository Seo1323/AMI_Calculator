import { invoke, isTauri } from '$lib/tauri';

export type Locale = 'pl' | 'en';
export type Theme = 'system' | 'light' | 'dark';
export type Density = 'compact' | 'comfortable' | 'spacious';
export type NumberLocale = 'auto' | 'pl-PL' | 'en-US' | 'en-GB' | 'de-DE';
export type StartPage = '/' | '/budget' | '/portfolio' | '/rebalance';

export interface Settings {
	locale: Locale;
	theme: Theme;
	accent: string;
	density: Density;
	fontScale: number;
	radius: number;
	numberLocale: NumberLocale;
	baseCurrency: string;
	startPage: StartPage;
	reduceMotion: boolean;
	sidebarCollapsed: boolean;
}

export const ACCENTS = ['#2f6fed', '#0f9d76', '#c2410c', '#9333ea', '#db2777', '#0891b2', '#ca8a04', '#475569'];

export const DEFAULTS: Settings = {
	locale: 'pl',
	theme: 'system',
	accent: ACCENTS[0],
	density: 'comfortable',
	fontScale: 1,
	radius: 10,
	numberLocale: 'auto',
	baseCurrency: 'PLN',
	startPage: '/',
	reduceMotion: false,
	sidebarCollapsed: false
};

const KEY = 'ami.settings.v1';

function sanitize(raw: unknown): Partial<Settings> {
	if (!raw || typeof raw !== 'object') return {};
	const out: Partial<Settings> = {};
	for (const k of Object.keys(DEFAULTS) as (keyof Settings)[]) {
		const v = (raw as Record<string, unknown>)[k];
		if (v !== undefined && typeof v === typeof DEFAULTS[k]) (out as Record<string, unknown>)[k] = v;
	}
	return out;
}

function load(): Settings {
	try {
		const raw = localStorage.getItem(KEY);
		if (raw) return { ...DEFAULTS, ...sanitize(JSON.parse(raw)) };
	} catch {
		/* storage unavailable */
	}
	return { ...DEFAULTS };
}

// The SQLite store is the source of truth; localStorage is only a cache so the
// first paint already has the right theme and language.
export const settings: Settings = $state(load());

let hydrated = false;
let pending: ReturnType<typeof setTimeout> | undefined;

/** Loads settings from the database (call once at startup). */
export async function hydrateSettings() {
	if (isTauri()) {
		try {
			const stored = await invoke<unknown>('settings_load');
			if (stored) replaceSettings(stored);
		} catch (e) {
			console.error('settings_load failed', e);
		}
	}
	hydrated = true;
	saveSettings();
}

export function saveSettings() {
	const snapshot = $state.snapshot(settings);
	try {
		localStorage.setItem(KEY, JSON.stringify(snapshot));
	} catch {
		/* storage unavailable */
	}
	if (!hydrated || !isTauri()) return;
	clearTimeout(pending);
	pending = setTimeout(() => {
		pending = undefined;
		invoke('settings_save', { value: snapshot }).catch((e) => console.error('settings_save failed', e));
	}, 300);
}

/** Writes pending changes immediately (e.g. before an export). */
export async function flushSettings() {
	if (!isTauri()) return;
	clearTimeout(pending);
	pending = undefined;
	await invoke('settings_save', { value: $state.snapshot(settings) });
}

export function replaceSettings(next: unknown) {
	Object.assign(settings, DEFAULTS, sanitize(next));
}

export function resetSettings() {
	Object.assign(settings, DEFAULTS);
}

export function snapshotSettings(): Settings {
	return $state.snapshot(settings) as Settings;
}
