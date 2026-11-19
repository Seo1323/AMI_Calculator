import test from 'node:test';
import assert from 'node:assert/strict';
import { layout, arcPath, polar } from '../../src/lib/pie.ts';

const close = (a, b, eps = 1e-9) => assert.ok(Math.abs(a - b) < eps, `${a} !~ ${b}`);

test('slices are contiguous, proportional and close the circle exactly', () => {
	const s = layout([{ id: 'a', value: 50 }, { id: 'b', value: 30 }, { id: 'c', value: 20 }]);
	assert.deepEqual(s.map((x) => x.id), ['a', 'b', 'c']);
	assert.equal(s[0].start, 0);
	close(s[0].share, 0.5); close(s[1].share, 0.3); close(s[2].share, 0.2);
	close(s[0].end, s[1].start); close(s[1].end, s[2].start);
	assert.equal(s[2].end, Math.PI * 2);
	close(s[0].end, Math.PI);
});

test('entries that are not positive numbers are dropped', () => {
	const s = layout([{ id: 'a', value: 10 }, { id: 'z', value: 0 }, { id: 'n', value: -5 }, { id: 'x', value: NaN }, { id: 'i', value: Infinity }, { id: 'b', value: 10 }]);
	assert.deepEqual(s.map((x) => x.id), ['a', 'b']);
	close(s[0].share + s[1].share, 1);
	assert.deepEqual(layout([]), []);
	assert.deepEqual(layout([{ id: 'a', value: 0 }]), []);
});

test('a tiny slice keeps a positive angle and the shares still add up to one', () => {
	const s = layout([{ id: 'big', value: 1_000_000 }, { id: 'dust', value: 0.01 }]);
	assert.ok(s[1].end - s[1].start > 0);
	close(s[0].share + s[1].share, 1);
});

test('polar: 12 o clock is straight up, 3 o clock to the right', () => {
	const [x0, y0] = polar(100, 100, 50, 0);
	close(x0, 100); close(y0, 50);
	const [x1, y1] = polar(100, 100, 50, Math.PI / 2);
	close(x1, 150); close(y1, 100);
});

test('arcPath: large-arc flag only past half a circle', () => {
	assert.match(arcPath(100, 100, 90, 50, 0, Math.PI), / 0 0 1 /); // exactly half: not "large"
	assert.match(arcPath(100, 100, 90, 50, 0, Math.PI * 1.5), / 0 1 1 /);
	assert.match(arcPath(100, 100, 90, 50, 0, 1), /^M 100 10 A 90 90 0 0 1 /);
	assert.ok(arcPath(100, 100, 90, 50, 0, 1).endsWith(' Z'));
});

test('arcPath: a single 100 % slice becomes a full ring made of two halves', () => {
	const d = arcPath(100, 100, 90, 50, 0, Math.PI * 2);
	assert.equal((d.match(/ Z/g) ?? []).length, 2);
	assert.equal((d.match(/M /g) ?? []).length, 2);
	const [only] = layout([{ id: 'etf', value: 123 }]);
	assert.equal((arcPath(100, 100, 90, 50, only.start, only.end).match(/M /g) ?? []).length, 2);
});

test('arcPath never produces NaN', () => {
	for (const [a, b] of [[0, 0.0001], [0, Math.PI], [1, 6.2831], [0, Math.PI * 2]]) {
		assert.ok(!/NaN|undefined/.test(arcPath(100, 100, 90, 50, a, b)), `${a}..${b}`);
	}
});

test('the last slice closes the circle exactly even when float error would leave a gap or an overlap', () => {
	// Naively adding value/total * 2π for these three values lands 9e-16 past 2π.
	const s = layout([{ id: 'a', value: 841.11 }, { id: 'b', value: 401.56 }, { id: 'c', value: 930.99 }]);
	assert.equal(s.at(-1).end, Math.PI * 2);
	for (let i = 1; i < s.length; i++) assert.equal(s[i].start, s[i - 1].end);
});
