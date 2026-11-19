import test from 'node:test';
import assert from 'node:assert/strict';
import { normalizePath, isRoot, isActivePath } from '../../src/lib/route.ts';

test('an empty pathname (tauri://localhost) is the root page', () => {
	assert.equal(normalizePath(''), '/');
	assert.equal(normalizePath('/'), '/');
	assert.equal(normalizePath('/portfolio'), '/portfolio');
	assert.equal(isRoot(''), true);
	assert.equal(isRoot('/'), true);
	assert.equal(isRoot('/portfolio'), false);
});

test('the dashboard entry is active on the root page only, however the root is spelled', () => {
	for (const root of ['', '/']) assert.equal(isActivePath('/', root), true, JSON.stringify(root));
	assert.equal(isActivePath('/', '/portfolio'), false);
	assert.equal(isActivePath('/', '/settings'), false);
});

test('other entries match by prefix, including detail pages', () => {
	assert.equal(isActivePath('/portfolio', '/portfolio'), true);
	assert.equal(isActivePath('/portfolio', '/portfolio/1a2b'), true);
	assert.equal(isActivePath('/portfolio', ''), false);
	assert.equal(isActivePath('/portfolio', '/rebalance'), false);
	assert.equal(isActivePath('/settings', '/portfolio'), false);
});
