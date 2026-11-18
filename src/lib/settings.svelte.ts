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

const KEY = 'balans.settings.v1';

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
	const lang = typeof navigator !== 'undefined' ? navigator.language : 'pl';
	return { ...DEFAULTS, locale: lang.startsWith('pl') ? 'pl' : 'en' };
}

// TODO(etap 1): persist in the SQLite store instead of localStorage.
export const settings: Settings = $state(load());

export function saveSettings() {
	try {
		localStorage.setItem(KEY, JSON.stringify($state.snapshot(settings)));
	} catch {
		/* storage unavailable */
	}
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
