<script lang="ts">
	import { onMount } from 'svelte';
	import { save, open } from '@tauri-apps/plugin-dialog';
	import { t, fmtDate, errorText } from '$lib/i18n/index.svelte';
	import { settings, ACCENTS, resetSettings, replaceSettings, snapshotSettings } from '$lib/settings.svelte';
	import { invoke, isTauri, type ExportBundle } from '$lib/tauri';

	const CURRENCIES = ['PLN', 'EUR', 'USD', 'GBP', 'CHF'];
	let version = $state('');
	let message = $state('');
	let error = $state('');

	onMount(async () => {
		if (isTauri()) version = await invoke<string>('app_version');
	});

	const filters = [{ name: 'Balans', extensions: ['balans'] }];

	async function doExport() {
		message = error = '';
		try {
			if (!isTauri()) throw { code: 'notInTauri' };
			const date = new Date().toISOString().slice(0, 10);
			const path = await save({ defaultPath: `balans-${date}.balans`, filters });
			if (!path) return;
			await invoke('export_bundle', { path, settings: snapshotSettings() });
			message = t('settings.exported', { path });
		} catch (e) {
			error = errorText(e);
		}
	}

	async function doImport() {
		message = error = '';
		try {
			if (!isTauri()) throw { code: 'notInTauri' };
			const path = await open({ multiple: false, directory: false, filters });
			if (!path) return;
			const bundle = await invoke<ExportBundle>('import_bundle', { path });
			replaceSettings(bundle.data.settings);
			message = t('settings.imported', { date: fmtDate(bundle.exportedAt) });
		} catch (e) {
			error = errorText(e);
		}
	}
</script>

<div class="stack">
	<h1>{t('settings.title')}</h1>

	<section class="card stack">
		<h2>{t('settings.appearance')}</h2>

		<div class="field">
			<span>{t('settings.theme')}</span>
			<div class="segmented" role="radiogroup" aria-label={t('settings.theme')}>
				{#each ['system', 'light', 'dark'] as const as th (th)}
					<button
						role="radio"
						aria-checked={settings.theme === th}
						class:on={settings.theme === th}
						onclick={() => (settings.theme = th)}>{t(`settings.theme.${th}`)}</button
					>
				{/each}
			</div>
		</div>

		<div class="field">
			<span>{t('settings.accent')}</span>
			<div class="swatches">
				{#each ACCENTS as c (c)}
					<button
						class="swatch"
						class:on={settings.accent === c}
						style:background={c}
						aria-label={c}
						onclick={() => (settings.accent = c)}
					></button>
				{/each}
				<input type="color" bind:value={settings.accent} aria-label={t('settings.accent')} />
			</div>
		</div>

		<div class="field">
			<span>{t('settings.density')}</span>
			<div class="segmented" role="radiogroup" aria-label={t('settings.density')}>
				{#each ['compact', 'comfortable', 'spacious'] as const as d (d)}
					<button
						role="radio"
						aria-checked={settings.density === d}
						class:on={settings.density === d}
						onclick={() => (settings.density = d)}>{t(`settings.density.${d}`)}</button
					>
				{/each}
			</div>
		</div>

		<label class="field">
			{t('settings.fontScale')} ({Math.round(settings.fontScale * 100)}%)
			<input type="range" min="0.8" max="1.4" step="0.05" bind:value={settings.fontScale} />
		</label>

		<label class="field">
			{t('settings.radius')} ({settings.radius}px)
			<input type="range" min="0" max="20" step="1" bind:value={settings.radius} />
		</label>

		<label class="check">
			<input type="checkbox" bind:checked={settings.reduceMotion} />
			{t('settings.reduceMotion')}
		</label>
	</section>

	<section class="card stack">
		<h2>{t('settings.regional')}</h2>
		<div class="row">
			<label class="field">
				{t('settings.language')}
				<select bind:value={settings.locale}>
					<option value="pl">Polski</option>
					<option value="en">English</option>
				</select>
			</label>
			<label class="field">
				{t('settings.numberLocale')}
				<select bind:value={settings.numberLocale}>
					<option value="auto">{t('settings.numberLocale.auto')}</option>
					<option value="pl-PL">1 234,56 · pl-PL</option>
					<option value="en-US">1,234.56 · en-US</option>
					<option value="en-GB">1,234.56 · en-GB</option>
					<option value="de-DE">1.234,56 · de-DE</option>
				</select>
			</label>
			<label class="field">
				{t('settings.baseCurrency')}
				<select bind:value={settings.baseCurrency}>
					{#each CURRENCIES as c (c)}
						<option value={c}>{c}</option>
					{/each}
				</select>
			</label>
		</div>
	</section>

	<section class="card stack">
		<h2>{t('settings.behavior')}</h2>
		<label class="field">
			{t('settings.startPage')}
			<select bind:value={settings.startPage}>
				<option value="/">{t('nav.dashboard')}</option>
				<option value="/budget">{t('nav.budget')}</option>
				<option value="/portfolio">{t('nav.portfolio')}</option>
				<option value="/rebalance">{t('nav.rebalance')}</option>
			</select>
		</label>
	</section>

	<section class="card stack">
		<h2>{t('settings.data')}</h2>
		<p class="muted">{t('settings.data.hint')}</p>
		<div class="row">
			<button class="btn primary" onclick={doExport}>{t('settings.export')}</button>
			<button class="btn" onclick={doImport}>{t('settings.import')}</button>
		</div>
		{#if message}<div class="notice">{message}</div>{/if}
		{#if error}<div class="notice error">{error}</div>{/if}
	</section>

	<section class="card stack">
		<h2>{t('settings.about')}</h2>
		<p class="muted">{t('app.name')} · {t('settings.version', { version: version || '—' })}</p>
		<div><button class="btn" onclick={resetSettings}>{t('common.reset')}</button></div>
	</section>
</div>

<style>
	p {
		margin: 0;
	}
	.field {
		display: flex;
		flex-direction: column;
		gap: 0.35rem;
		font-size: 0.85rem;
		color: var(--text-muted);
		max-width: 28rem;
	}
	.check {
		display: flex;
		gap: 0.5rem;
		align-items: center;
	}
	.segmented {
		display: inline-flex;
		flex-wrap: wrap;
		border: 1px solid var(--border);
		border-radius: calc(var(--radius) * 0.7);
		overflow: hidden;
		width: fit-content;
	}
	.segmented button {
		border: none;
		background: var(--surface-2);
		padding: calc(var(--space) * 0.4) calc(var(--space) * 0.9);
		cursor: pointer;
		color: var(--text);
	}
	.segmented button.on {
		background: var(--accent);
		color: white;
	}
	.swatches {
		display: flex;
		flex-wrap: wrap;
		gap: 0.5rem;
		align-items: center;
	}
	.swatch {
		width: 28px;
		height: 28px;
		border-radius: 50%;
		border: 2px solid transparent;
		cursor: pointer;
	}
	.swatch.on {
		border-color: var(--text);
		box-shadow: 0 0 0 2px var(--surface) inset;
	}
	input[type='color'] {
		width: 36px;
		height: 30px;
		border: none;
		background: none;
		padding: 0;
		cursor: pointer;
	}
	input[type='range'] {
		max-width: 20rem;
	}
</style>
