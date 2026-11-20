import { invoke as tauriInvoke, isTauri } from '@tauri-apps/api/core';

export { isTauri };

/** Commands that change data kept in the sync file (online prices and settings do not count). */
const MUTATING = new Set(['db_save', 'db_delete', 'portfolio_set_targets', 'manual_quote', 'quote_set', 'import_json']);

/** `mutated` runs after such a command succeeds; sync.svelte.ts uses it to schedule a sync. */
export const hooks: { mutated?: () => void } = {};

/** Typed wrapper; rejects with `{ code, message }` like the Rust ErrorPayload. */
export async function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
	if (!isTauri()) throw { code: 'notInTauri', message: 'not running inside Tauri' };
	const result = await tauriInvoke<T>(cmd, args);
	if (MUTATING.has(cmd)) hooks.mutated?.();
	return result;
}

export type Mode = 'buyOnly' | 'full';

export interface PositionInput {
	id: string;
	quantity: string;
	price: string;
	targetPercent: string;
	fractional: boolean;
}

export interface RebalanceInput {
	positions: PositionInput[];
	contribution: string;
	mode: Mode;
	tolerancePercent: string;
	fractionDecimals?: number;
}

export interface PositionResult {
	id: string;
	currentValue: string;
	currentPercent: string;
	targetPercent: string;
	deviationBefore: string;
	targetValue: string;
	unitsToTrade: string;
	tradeValue: string;
	valueAfter: string;
	percentAfter: string;
	deviationAfter: string;
}

export interface RebalanceResult {
	totalBefore: string;
	contribution: string;
	totalAfter: string;
	cashLeft: string;
	maxDeviationBefore: string;
	needsRebalance: boolean;
	positions: PositionResult[];
	/** Only for stored portfolios: tax on the sales (see `TaxEstimate` in Rust). */
	tax?: { realizedGain: string; ratePercent: string; tax: string } | null;
}

export type ImportMode = 'replace' | 'merge';

export interface ImportReport {
	exportedAt: string;
	stats: { inserted: number; updated: number; skipped: number };
	settings: unknown | null;
	backup: string;
}

export interface DataLocation {
	database: string;
	backups: string;
	schemaVersion: number;
}
