import { invoke as tauriInvoke, isTauri } from '@tauri-apps/api/core';

export { isTauri };

/** Typed wrapper; rejects with `{ code, message }` like the Rust ErrorPayload. */
export async function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
	if (!isTauri()) throw { code: 'notInTauri', message: 'not running inside Tauri' };
	return tauriInvoke<T>(cmd, args);
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
}

export interface ExportBundle {
	format: string;
	version: number;
	appVersion: string;
	exportedAt: string;
	data: { settings: unknown } & Record<string, unknown>;
}
