import test from 'node:test';
import assert from 'node:assert/strict';
import { keepAccent, LEGACY_ACCENT, ACCENT_VERSION } from '../../src/lib/accent.ts';

test('the old default blue is dropped once, so the new default shows', () => {
	assert.equal(keepAccent(LEGACY_ACCENT, undefined), false);
	assert.equal(keepAccent(LEGACY_ACCENT, 0), false);
});

test('after the migration the same blue is a deliberate choice and stays', () => {
	assert.equal(keepAccent(LEGACY_ACCENT, ACCENT_VERSION), true);
});

test('any other saved accent is always kept, a missing one stays missing', () => {
	assert.equal(keepAccent('#c2410c', undefined), true);
	assert.equal(keepAccent('#c2410c', 0), true);
	assert.equal(keepAccent(undefined, undefined), false);
});
