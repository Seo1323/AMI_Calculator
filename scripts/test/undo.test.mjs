import test from 'node:test';
import assert from 'node:assert/strict';
import { orUndo } from '../../src/lib/undo.ts';

test('a successful second step leaves the first one in place', async () => {
	let undone = false;
	await orUndo(async () => (undone = true), async () => {});
	assert.equal(undone, false);
});

test('a failing second step undoes the first and rethrows the original error', async () => {
	let undone = false;
	const boom = new Error('boom');
	await assert.rejects(
		orUndo(async () => (undone = true), async () => { throw boom; }),
		(e) => e === boom
	);
	assert.equal(undone, true);
});

test('an undo that fails too does not hide the original error', async () => {
	const boom = new Error('boom');
	await assert.rejects(
		orUndo(async () => { throw new Error('undo failed'); }, async () => { throw boom; }),
		(e) => e === boom
	);
});
