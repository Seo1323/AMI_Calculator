import { invoke, isTauri } from '$lib/tauri';
import { settings } from '$lib/settings.svelte';

export interface RefreshReport {
	fxUpdated: number;
	quotesUpdated: number;
	failures: { symbol: string; reason: string }[];
}

/** Shared refresh state; `version` bumps after every refresh so pages can reload. */
export const quotes = $state({
	refreshing: false,
	version: 0,
	lastAt: null as Date | null,
	lastReport: null as RefreshReport | null,
	error: null as unknown
});

export async function refreshQuotes(): Promise<RefreshReport | null> {
	if (!isTauri() || quotes.refreshing) return null;
	quotes.refreshing = true;
	quotes.error = null;
	try {
		const report = await invoke<RefreshReport>('quotes_refresh');
		quotes.lastReport = report;
		quotes.lastAt = new Date();
		quotes.version++;
		return report;
	} catch (e) {
		quotes.error = e;
		return null;
	} finally {
		quotes.refreshing = false;
	}
}

let timer: ReturnType<typeof setInterval> | undefined;

/** Refreshes now (if enabled) and then every `settings.refreshMinutes`. */
export function startAutoRefresh() {
	clearInterval(timer);
	if (!settings.autoRefreshQuotes) return;
	const minutes = Math.max(5, settings.refreshMinutes || 30);
	const due = !quotes.lastAt || Date.now() - quotes.lastAt.getTime() > minutes * 60_000;
	if (due) refreshQuotes();
	timer = setInterval(refreshQuotes, minutes * 60_000);
}
