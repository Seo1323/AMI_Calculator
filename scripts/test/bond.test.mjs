// Run with: npm test  (Node strips the TypeScript types itself, no extra dependency)
import test from 'node:test';
import assert from 'node:assert/strict';
import { addMonths, remaining, bondSymbol, daysInMonth, termBetween, firstOfMonth, offerMonth, BOND_SERIES } from '../../src/lib/bond.ts';

test('addMonths keeps the day when it exists', () => {
	assert.equal(addMonths('2026-04-15', 120), '2036-04-15');
	assert.equal(addMonths('2026-11-19', 3), '2027-02-19');
	assert.equal(addMonths('2026-12-01', 1), '2027-01-01');
	assert.equal(addMonths('2026-01-01', 0), '2026-01-01');
});

test('addMonths clamps to the last day of a shorter month', () => {
	assert.equal(addMonths('2026-01-31', 1), '2026-02-28');
	assert.equal(addMonths('2027-12-31', 2), '2028-02-29'); // leap year
	assert.equal(addMonths('2024-02-29', 12), '2025-02-28');
	assert.equal(addMonths('2024-02-29', 48), '2028-02-29');
	assert.equal(addMonths('2026-08-31', 1), '2026-09-30');
});

test('addMonths rejects bad input', () => {
	assert.equal(addMonths('2026-02-30', 1), null);
	assert.equal(addMonths('not a date', 1), null);
	assert.equal(addMonths('2026-04-15', 1.5), null);
	assert.equal(addMonths('', 1), null);
});

test('remaining counts calendar years, months and days', () => {
	assert.deepEqual(remaining('2026-11-19', '2036-04-01'), { years: 9, months: 4, days: 13 });
	assert.deepEqual(remaining('2026-11-19', '2026-11-20'), { years: 0, months: 0, days: 1 });
	assert.deepEqual(remaining('2026-01-31', '2026-03-01'), { years: 0, months: 1, days: 1 }); // borrows February
	assert.deepEqual(remaining('2025-12-15', '2026-01-10'), { years: 0, months: 0, days: 26 }); // borrows December
	assert.deepEqual(remaining('2026-01-01', '2027-01-01'), { years: 1, months: 0, days: 0 });
});

test('remaining is null once the bond has matured or the input is invalid', () => {
	assert.equal(remaining('2026-11-19', '2026-11-19'), null);
	assert.equal(remaining('2026-11-19', '2026-11-18'), null);
	assert.equal(remaining('2026-11-19', 'garbage'), null);
});

test('bondSymbol follows the Polish naming', () => {
	assert.equal(bondSymbol('EDO', '2036-04-01'), 'EDO0436');
	assert.equal(bondSymbol('COI', '2028-11-15'), 'COI1128');
	assert.equal(bondSymbol('TOS', '2100-01-01'), 'TOS0100');
	assert.equal(bondSymbol('EDO', 'x'), null);
});

test('series table is sane', () => {
	assert.equal(daysInMonth(2028, 2), 29);
	assert.equal(daysInMonth(2026, 2), 28);
	assert.deepEqual(BOND_SERIES.map((s) => s.code), ['OTS', 'ROR', 'DOR', 'TOS', 'COI', 'ROS', 'EDO', 'ROD']);
	assert.ok(BOND_SERIES.every((s) => Number.isInteger(s.months) && s.months > 0));
});

test('termBetween recovers the term from two dates', () => {
	assert.equal(termBetween('2026-04-15', '2036-04-15'), 120);
	assert.equal(termBetween('2026-01-31', '2026-02-28'), 1); // clamped month end
	assert.equal(termBetween('2026-04-15', '2026-04-15'), null);
	assert.equal(termBetween('2026-04-15', '2026-07-16'), null); // not a whole number of months
	assert.equal(termBetween('2026-04-15', 'x'), null);
});

test('retail bonds mature on the first day of the month', () => {
	// Sold in November 2026, 10 years: EDO1136 matures on 1 Nov 2036, whatever day it was bought.
	assert.equal(firstOfMonth(addMonths('2026-11-19', 120)), '2036-11-01');
	assert.equal(firstOfMonth(addMonths('2026-11-30', 3)), '2027-02-01'); // OTS0227 (28 Feb clamp)
	assert.equal(firstOfMonth('x'), null);
});

test('offerMonth recovers the month an offer is sold in', () => {
	assert.equal(offerMonth('EDO1136', 120), '2026-11');
	assert.equal(offerMonth('OTS0227', 3), '2026-11');
	assert.equal(offerMonth('ROD1138', 144), '2026-11');
	assert.equal(offerMonth('COI1130', 48), '2026-11');
	assert.equal(offerMonth('bad', 12), null);
});
