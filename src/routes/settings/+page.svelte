<script lang="ts">
	import { onMount } from 'svelte';
	import { save, open, confirm } from '@tauri-apps/plugin-dialog';
	import { t, fmtDate, errorText } from '$lib/i18n/index.svelte';
	import {
		settings,
		ACCENTS,
		resetSettings,
		replaceSettings,
		flushSettings,
		widgetList,
		setWidgetList
	} from '$lib/settings.svelte';
	import { invoke, isTauri, type DataLocation, type ImportMode, type ImportReport } from '$lib/tauri';

	const CURRENCIES = ['PLN', 'EUR', 'USD', 'GBP', 'CHF'];
	let version = $state('');
	let location: DataLocation | null = $state(null);
	let importMode: ImportMode = $state('replace');
	let messages: string[] = $state([]);
	let error = $state('');

	onMount(async () => {
		if (!isTauri()) return;
		version = await invoke<string>('app_version');
		location = await invoke<DataLocation>('data_location');
	});

	const filters = [{ name: 'AMI_Calculator', extensions: ['ami'] }];

	const widgets = $derived(widgetList());
	function moveWidget(i: number, dir: -1 | 1) {
		const list = widgetList();
		const j = i + dir;
		if (j < 0 || j >= list.length) return;
		[list[i], list[j]] = [list[j], list[i]];
		setWidgetList(list);
	}
	function toggleWidget(i: number) {
		const list = widgetList();
		list[i].visible = !list[i].visible;
		setWidgetList(list);
	}

	async function doExport() {
		messages = [];
		error = '';
		try {
			if (!isTauri()) throw { code: 'notInTauri' };
			const date = new Date().toISOString().slice(0, 10);
			const path = await save({ defaultPath: `ami-calculator-${date}.ami`, filters });
			if (!path) return;
			await flushSettings();
			await invoke('export_bundle', { path });
			messages = [t('settings.exported', { path })];
		} catch (e) {
			error = errorText(e);
		}
	}

	async function doImport() {
		messages = [];
		error = '';
		try {
			if (!isTauri()) throw { code: 'notInTauri' };
			const path = await open({ multiple: false, directory: false, filters });
			if (!path) return;
			if (importMode === 'replace' && !(await confirm(t('settings.importConfirm'), { kind: 'warning' }))) return;
			const report = await invoke<ImportReport>('import_bundle', { path, mode: importMode });
			if (report.settings) replaceSettings(report.settings);
			messages = [
				t('settings.imported', { date: fmtDate(report.exportedAt), ...report.stats }),
				t('settings.backupSaved', { path: report.backup })
			];
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
		<h2>{t('settings.quotes')}</h2>
		<p class="muted">{t('settings.quotesHint')}</p>
		<label class="check">
			<input type="checkbox" bind:checked={settings.autoRefreshQuotes} />
			{t('settings.autoRefresh')}
		</label>
		<label class="field">
			{t('settings.refreshMinutes')}
			<select bind:value={settings.refreshMinutes} disabled={!settings.autoRefreshQuotes}>
				{#each [5, 15, 30, 60, 180] as m (m)}<option value={m}>{m}</option>{/each}
			</select>
		</label>
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
				<option value="/reports">{t('nav.reports')}</option>
			</select>
		</label>
	</section>

	<section class="card stack">
		<h2>{t('settings.dashboard')}</h2>
		<p class="muted">{t('settings.dashboard.hint')}</p>
		<ul class="widgets">
			{#each widgets as w, i (w.id)}
				<li>
					<label class="check">
						<input type="checkbox" checked={w.visible} onchange={() => toggleWidget(i)} />
						{t(`widget.${w.id}`)}
					</label>
					<span class="moves">
						<button class="btn ghost" disabled={i === 0} aria-label={t('settings.moveUp')} onclick={() => moveWidget(i, -1)}>↑</button>
						<button class="btn ghost" disabled={i === widgets.length - 1} aria-label={t('settings.moveDown')} onclick={() => moveWidget(i, 1)}>↓</button>
					</span>
				</li>
			{/each}
		</ul>
	</section>

	<section class="card stack">
		<h2>{t('settings.privacy')}</h2>
		<label class="check">
			<input type="checkbox" bind:checked={settings.hideAmounts} />
			{t('settings.hideAmounts')}
		</label>
	</section>

	<section class="card stack">
		<h2>{t('settings.shortcuts')}</h2>
		<dl class="keys">
			<dt><kbd>Alt</kbd> + <kbd>1</kbd>…<kbd>6</kbd></dt>
			<dd>{t('shortcut.sections')}</dd>
			<dt><kbd>Alt</kbd> + <kbd>H</kbd></dt>
			<dd>{t('shortcut.hide')}</dd>
			<dt><kbd>Alt</kbd> + <kbd>B</kbd></dt>
			<dd>{t('shortcut.sidebar')}</dd>
		</dl>
	</section>

	<section class="card stack">
		<h2>{t('settings.data')}</h2>
		<p class="muted">{t('settings.data.hint')}</p>
		<div class="row">
			<button class="btn primary" onclick={doExport}>{t('settings.export')}</button>
			<label class="field">
				{t('settings.importMode')}
				<select bind:value={importMode}>
					<option value="replace">{t('settings.importMode.replace')}</option>
					<option value="merge">{t('settings.importMode.merge')}</option>
				</select>
			</label>
			<button class="btn" onclick={doImport}>{t('settings.import')}</button>
		</div>
		{#each messages as m (m)}<div class="notice">{m}</div>{/each}
		{#if error}<div class="notice error">{error}</div>{/if}
		{#if location}
			<dl class="location">
				<dt>{t('settings.location.db')}</dt>
				<dd><code>{location.database}</code></dd>
				<dt>{t('settings.location.backups')}</dt>
				<dd><code>{location.backups}</code></dd>
			</dl>
		{/if}
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
	.widgets {
		list-style: none;
		margin: 0;
		padding: 0;
		max-width: 28rem;
	}
	.widgets li {
		display: flex;
		justify-content: space-between;
		align-items: center;
		border-bottom: 1px solid var(--border);
		padding: 0.2rem 0;
	}
	.keys {
		display: grid;
		grid-template-columns: max-content 1fr;
		gap: 0.4rem 1rem;
		margin: 0;
		align-items: center;
	}
	.keys dd {
		margin: 0;
	}
	kbd {
		font: inherit;
		font-size: 0.8rem;
		padding: 0.1rem 0.4rem;
		border: 1px solid var(--border);
		border-bottom-width: 2px;
		border-radius: 4px;
		background: var(--surface-2);
	}
	.location {
		margin: 0;
		font-size: 0.85rem;
	}
	.location dt {
		color: var(--text-muted);
	}
	.location dd {
		margin: 0 0 0.5rem;
		overflow-wrap: anywhere;
	}
</style>
