import test from 'node:test';
import assert from 'node:assert/strict';
import { median, isSlow, asPerfMode, perfActive, SLOW_FRAME_MS, MIN_FRAMES } from '../../src/lib/perf.ts';

test('median of odd, even and empty lists', () => {
	assert.equal(median([3, 1, 2]), 2);
	assert.equal(median([4, 1, 3, 2]), 2.5);
	assert.equal(median([]), 0);
	assert.equal(median([5]), 5);
});

test('a smooth 60 Hz scroll is not slow', () => {
	assert.equal(isSlow(Array(30).fill(16.7)), false);
	assert.equal(isSlow(Array(12).fill(33)), false); // 30 fps is still acceptable
	assert.equal(isSlow(Array(8).fill(SLOW_FRAME_MS)), false); // the threshold itself is not slow
});

test('the measured 3 fps case is slow, even with only a few frames', () => {
	assert.equal(isSlow(Array(7).fill(371)), true);
	assert.equal(isSlow(Array(MIN_FRAMES).fill(120)), true);
	assert.equal(isSlow(Array(10).fill(47.6)), true); // 21 fps with the effects off is still below 22 fps
});

test('too few frames, or a pause, give no verdict', () => {
	assert.equal(isSlow(Array(MIN_FRAMES - 1).fill(400)), false);
	assert.equal(isSlow([]), false);
	assert.equal(isSlow([400, 400, 400, 400, 400, 2500]), false); // hidden window / breakpoint, not slow painting
});

test('one slow frame among smooth ones does not trip it (median, not mean)', () => {
	assert.equal(isSlow([16, 16, 16, 16, 16, 16, 900]), false);
	assert.equal(isSlow([16, 16, 16, 300, 300, 300, 300]), true);
});

test('performance mode follows the setting, and auto needs a slow sighting', () => {
	assert.equal(perfActive('on', false), true);
	assert.equal(perfActive('off', true), false);
	assert.equal(perfActive('auto', false), false);
	assert.equal(perfActive('auto', true), true);
	assert.equal(asPerfMode('on'), 'on');
	assert.equal(asPerfMode('banana'), 'auto');
	assert.equal(asPerfMode(undefined), 'auto');
});
