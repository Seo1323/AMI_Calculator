import test from 'node:test';
import assert from 'node:assert/strict';
import { lean, tilt, MAX_TILT } from '../../src/lib/balance.ts';

test('lean is the signed deviation of the position furthest from its target', () => {
	assert.equal(lean([{ current: 52, target: 50 }, { current: 10, target: 20 }, { current: 38, target: 30 }]), -10);
	assert.equal(lean([{ current: 70, target: 40 }, { current: 5, target: 10 }]), 30);
	assert.equal(lean([]), 0);
});

test('a position without a price is ignored', () => {
	assert.equal(lean([{ current: null, target: 40 }, { current: 12, target: 10 }]), 2);
});

test('within tolerance the beam is level, otherwise it leans towards the heavy side and is capped', () => {
	assert.equal(tilt(25, true), 0);
	assert.equal(tilt(15, false), MAX_TILT / 2);
	assert.equal(tilt(-15, false), -MAX_TILT / 2);
	assert.equal(tilt(300, false), MAX_TILT);
	assert.equal(tilt(-300, false), -MAX_TILT);
});
