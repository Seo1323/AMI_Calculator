import test from 'node:test';
import assert from 'node:assert/strict';
import { WHOLE, toUnits, fromUnits, total, split, setUnits, addRow, removeRow, toggleLock, equalSplit, fromSaved } from '../../src/lib/targets.ts';

const rows = (...u) => u.map((units, i) => ({ id: String.fromCharCode(97 + i), units, locked: false }));
const units = (rs) => rs.map((r) => r.units);

test('toUnits reads decimals with a comma or a dot and rejects nonsense', () => {
	assert.equal(toUnits('12,5'), 1250);
	assert.equal(toUnits('12.5'), 1250);
	assert.equal(toUnits(' 7 '), 700);
	assert.equal(toUnits('0'), 0);
	assert.equal(toUnits('100'), 10000);
	for (const bad of ['', 'abc', '-1', '100.01', '1e3x']) assert.equal(toUnits(bad), null, bad);
	assert.equal(fromUnits(1250), 12.5);
});

test('split always adds up to the amount, with the largest remainders getting the extra unit', () => {
	assert.deepEqual(split(10000, [1, 1, 1]), [3334, 3333, 3333]);
	assert.deepEqual(split(10000, [0, 0]), [5000, 5000]);
	assert.deepEqual(split(7, [5, 3, 2]), [4, 2, 1]); // 3.5 / 2.1 / 1.4 -> remainders .5 .1 .4
	assert.deepEqual(split(5, []), []);
	for (const w of [[1, 2, 3, 4, 5], [7, 7, 7, 7], [0.1, 0.2, 0.7], [1000, 1]]) {
		assert.equal(split(10000, w).reduce((a, b) => a + b, 0), 10000, JSON.stringify(w));
	}
});

test('moving a slider shares the rest proportionally and keeps the total at 100 %', () => {
	const r = setUnits(rows(5000, 3000, 2000), 'a', 7000);
	assert.deepEqual(units(r), [7000, 1800, 1200]); // 3000:2000 of the remaining 3000
	assert.equal(total(r), WHOLE);
	for (const v of [0, 1, 3333, 9999, 10000]) assert.equal(total(setUnits(rows(5000, 3000, 2000), 'b', v)), WHOLE, String(v));
});

test('others at zero get an equal share of what is left', () => {
	assert.deepEqual(units(setUnits(rows(10000, 0, 0), 'a', 6000)), [6000, 2000, 2000]);
});

test('a value is clamped to 0-100 % and fractions are rounded', () => {
	assert.deepEqual(units(setUnits(rows(5000, 5000), 'a', 12345)), [10000, 0]);
	assert.deepEqual(units(setUnits(rows(5000, 5000), 'a', -50)), [0, 10000]);
	assert.equal(setUnits(rows(5000, 5000), 'a', 3333.6)[0].units, 3334);
});

test('a locked row never moves, and limits how far the others can go', () => {
	let r = toggleLock(rows(2000, 5000, 3000), 'a');
	r = setUnits(r, 'b', 9000);
	assert.deepEqual(units(r), [2000, 8000, 0]); // only 8000 are free
	assert.equal(total(r), WHOLE);
	assert.equal(r[0].locked, true);
	// A locked row cannot be moved by its own slider.
	assert.deepEqual(units(setUnits(r, 'a', 5000)), [2000, 8000, 0]);
});

test('with no other free row the moved row is simply the rest of the 100 %', () => {
	assert.deepEqual(units(setUnits(rows(10000), 'a', 40)), [10000]);
	let r = toggleLock(rows(3000, 7000), 'a');
	assert.deepEqual(units(setUnits(r, 'b', 100)), [3000, 7000]);
});

test('the first row takes 100 %, later rows start at 0 and move nothing; duplicates are ignored', () => {
	let r = addRow([], 'a');
	assert.deepEqual(units(r), [10000]);
	r = addRow(r, 'b');
	assert.deepEqual(units(r), [10000, 0]);
	assert.equal(addRow(r, 'a'), r);
	assert.equal(total(addRow(rows(6000, 4000), 'c')), WHOLE);
});

test('removing a row hands its share to the free rows in proportion', () => {
	assert.deepEqual(units(removeRow(rows(5000, 3000, 2000), 'a')), [6000, 4000]);
	assert.deepEqual(units(removeRow(rows(5000, 5000), 'a')), [10000]);
	assert.deepEqual(removeRow(rows(10000), 'a'), []);
	// Locked rows keep their share, so the removed share can be lost when nobody is free.
	const r = toggleLock(toggleLock(rows(4000, 6000), 'a'), 'b');
	assert.deepEqual(units(removeRow(r, 'a')), [6000]);
	assert.equal(removeRow(rows(5000, 5000), 'zzz').length, 2);
});

test('equal split respects locked rows and stays at exactly 100 %', () => {
	assert.deepEqual(units(equalSplit(rows(9000, 500, 500))), [3334, 3333, 3333]);
	const r = toggleLock(rows(4000, 3000, 3000), 'a');
	assert.deepEqual(units(equalSplit(r)), [4000, 3000, 3000]);
	assert.deepEqual(units(equalSplit(toggleLock(rows(1000, 8000, 1000), 'a'))), [1000, 4500, 4500]);
});

test('saved targets are read back; zero and unreadable ones are dropped', () => {
	const r = fromSaved([{ id: 'a', percent: '60' }, { id: 'b', percent: '12.5' }, { id: 'c', percent: '0' }, { id: 'd', percent: 'x' }]);
	assert.deepEqual(r, [{ id: 'a', units: 6000, locked: false }, { id: 'b', units: 1250, locked: false }]);
});

test('toggleLock flips only the named row', () => {
	const r = toggleLock(rows(5000, 5000), 'b');
	assert.deepEqual(r.map((x) => x.locked), [false, true]);
});
