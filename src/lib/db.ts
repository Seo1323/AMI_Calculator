import { invoke } from '$lib/tauri';

/** Mirrors `ami_core::store::EntityKind`. */
export type EntityKind =
	| 'account'
	| 'category'
	| 'transaction'
	| 'budgetLimit'
	| 'instrument'
	| 'portfolio'
	| 'allocationTarget'
	| 'trade';

/** Metadata the backend adds to every record. Amounts are decimal strings. */
export interface Meta {
	id: string;
	createdAt: string;
	updatedAt: string;
	deletedAt?: string;
}

export type Draft<T> = Omit<T, keyof Meta> & Partial<Meta>;

export const db = {
	list: <T>(kind: EntityKind) => invoke<(T & Meta)[]>('db_list', { kind }),
	/** Creates when `id` is missing, otherwise updates. Returns the stored record. */
	save: <T>(kind: EntityKind, value: Draft<T>) => invoke<T & Meta>('db_save', { kind, value }),
	remove: (kind: EntityKind, id: string) => invoke<boolean>('db_delete', { kind, id })
};
