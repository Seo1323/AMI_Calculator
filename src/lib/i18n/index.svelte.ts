import pl from './pl';
import en from './en';
import { settings } from '$lib/settings.svelte';

export type MessageKey = keyof typeof pl;

const dictionaries: Record<string, Record<MessageKey, string>> = { pl, en };

/** Reactive: re-renders when `settings.locale` changes. */
export function t(key: MessageKey, params?: Record<string, string | number>): string {
	let s = dictionaries[settings.locale]?.[key] ?? pl[key] ?? key;
	if (params) for (const [k, v] of Object.entries(params)) s = s.replaceAll(`{${k}}`, String(v));
	return s;
}

export function numberLocale(): string {
	if (settings.numberLocale !== 'auto') return settings.numberLocale;
	return settings.locale === 'pl' ? 'pl-PL' : 'en-GB';
}

export function fmtNumber(value: string | number, maxDigits = 2): string {
	const n = typeof value === 'number' ? value : Number(value);
	if (!Number.isFinite(n)) return String(value);
	return new Intl.NumberFormat(numberLocale(), { maximumFractionDigits: maxDigits }).format(n);
}

export function fmtMoney(value: string | number, currency = settings.baseCurrency): string {
	const n = typeof value === 'number' ? value : Number(value);
	if (!Number.isFinite(n)) return String(value);
	try {
		return new Intl.NumberFormat(numberLocale(), { style: 'currency', currency }).format(n);
	} catch {
		return `${fmtNumber(n)} ${currency}`; // non-ISO codes such as BTC
	}
}

export function fmtDate(value: string | Date): string {
	return new Intl.DateTimeFormat(numberLocale(), { dateStyle: 'medium', timeStyle: 'short' }).format(new Date(value));
}

interface ErrorPayload {
	code?: string;
	message?: string;
}

/** Turns an error from a Tauri command into a translated message. */
export function errorText(e: unknown): string {
	const p = (typeof e === 'object' && e ? e : { message: String(e) }) as ErrorPayload;
	const key = `error.${p.code}` as MessageKey;
	if (p.code && key in pl) return t(key, { detail: p.message ?? '' });
	return t('error.unknown', { detail: p.message ?? String(e) });
}
