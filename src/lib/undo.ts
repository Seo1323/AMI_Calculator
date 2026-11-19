// Dependency-free so `node --test` can run it directly.

/**
 * Runs the second step of a two-step save. If it fails the first step is undone (best effort) and
 * the original error is rethrown, so a retry cannot leave a duplicate of the first record behind.
 */
export async function orUndo(undo: () => Promise<unknown>, step: () => Promise<unknown>): Promise<void> {
	try {
		await step();
	} catch (e) {
		await undo().catch(() => {});
		throw e;
	}
}
