import test from 'node:test';
import assert from 'node:assert/strict';
import { parseAmounts, summarize, MAX_AMOUNTS } from '../../src/lib/whatif.ts';

test('amounts accept comma decimals and spaces, drop junk, zero, negatives and duplicates', () => {
	assert.deepEqual(parseAmounts('500; 1 000; 2000,5'), ['500', '1000', '2000.5']);
	assert.deepEqual(parseAmounts('abc; 0; -5; ; 100; 100.0'), ['100']);
	assert.deepEqual(parseAmounts(''), []);
});

test('at most MAX_AMOUNTS amounts are kept, in typed order', () => {
	const text = Array.from({ length: 10 }, (_, i) => String(i + 1)).join(';');
	assert.deepEqual(parseAmounts(text), ['1', '2', '3', '4', '5', '6'].slice(0, MAX_AMOUNTS));
});

test('summary: worst deviation after, cash left, bought and sold value', () => {
	const pos = (deviationAfter, tradeValue) => ({ deviationAfter, tradeValue });
	const s = summarize({
		cashLeft: '12.5',
		positions: [pos('-1.5', '300'), pos('2', '-100'), pos('0.1', '50')]
	});
	assert.deepEqual(s, { maxDeviationAfter: 2, cashLeft: 12.5, bought: 350, sold: 100 });
});
