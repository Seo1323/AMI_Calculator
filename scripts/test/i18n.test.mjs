// Translations must keep the same {placeholders} as the Polish source and never be empty.
import test from 'node:test';
import assert from 'node:assert/strict';
import pl from '../../src/lib/i18n/pl.ts';
import en from '../../src/lib/i18n/en.ts';
import de from '../../src/lib/i18n/de.ts';
import es from '../../src/lib/i18n/es.ts';
import fr from '../../src/lib/i18n/fr.ts';
import uk from '../../src/lib/i18n/uk.ts';

const placeholders = (s) => [...s.matchAll(/\{(\w+)\}/g)].map((m) => m[1]).sort();

for (const [name, dict] of Object.entries({ en, de, es, fr, uk })) {
	test(`${name}: same keys, non-empty, same placeholders as pl`, () => {
		assert.deepEqual(Object.keys(dict).sort(), Object.keys(pl).sort());
		for (const [key, text] of Object.entries(dict)) {
			assert.ok(text.trim().length > 0, `${name} ${key} is empty`);
			assert.deepEqual(placeholders(text), placeholders(pl[key]), `${name} ${key}: placeholders differ`);
		}
	});
}
