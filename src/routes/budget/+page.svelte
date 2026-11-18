<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { confirm } from '@tauri-apps/plugin-dialog';
	import { t, fmtMoney, numberLocale, errorText } from '$lib/i18n/index.svelte';
	import { settings } from '$lib/settings.svelte';
	import { db } from '$lib/db';
	import { invoke, isTauri } from '$lib/tauri';
	import Icon from '$lib/components/Icon.svelte';
	import {
		ACCOUNT_KINDS,
		CURRENCIES,
		monthKey,
		toDecimal,
		today,
		type Account,
		type AccountBalance,
		type Category,
		type MonthSummary,
		type Portfolio,
		type Transaction
	} from '$lib/types';

	let month = $state(monthKey(new Date()));
	let summary = $state<MonthSummary | null>(null);
	let balances: AccountBalance[] = $state([]);
	let categories: Category[] = $state([]);
	let transactions: Transaction[] = $state([]);
	let portfolios: Portfolio[] = $state([]);
	let limitInputs: Record<string, string> = $state({});
	let targetPortfolio = $state('');
	let error = $state('');
	let info = $state('');

	type TxType = 'expense' | 'income' | 'transfer';
	const emptyTx = (accountId = '') => ({
		type: 'expense' as TxType,
		date: today(),
		accountId,
		toAccountId: '',
		amount: '',
		amountTo: '',
		categoryId: '',
		note: ''
	});
	let txForm = $state(emptyTx());
	const emptyAccount = () => ({ name: '', kind: 'checking' as Account['kind'], currency: settings.baseCurrency, openingBalance: '0' });
	let accountForm = $state(emptyAccount());
	const emptyCategory = () => ({ name: '', kind: 'expense' as Category['kind'], parentId: '', color: '#2f6fed' });
	let categoryForm = $state(emptyCategory());

	const accounts = $derived(balances.map((b) => b.account));
	const accountById = $derived(Object.fromEntries(accounts.map((a) => [a.id, a])));
	const categoryById = $derived(Object.fromEntries(categories.map((c) => [c.id, c])));
	const txAccount = $derived(accountById[txForm.accountId]);
	const toAccount = $derived(accountById[txForm.toAccountId]);
	const needsAmountTo = $derived(txForm.type === 'transfer' && txAccount && toAccount && txAccount.currency !== toAccount.currency);
	const txCategories = $derived(categories.filter((c) => c.kind === (txForm.type === 'income' ? 'income' : 'expense')));
	const totalsByCurrency = $derived(
		balances.reduce<Record<string, number>>((acc, b) => {
			if (!b.account.archived) acc[b.account.currency] = (acc[b.account.currency] ?? 0) + Number(b.balance);
			return acc;
		}, {})
	);
	const monthLabel = $derived(
		new Intl.DateTimeFormat(numberLocale(), { month: 'long', year: 'numeric' }).format(new Date(month + 'T00:00:00'))
	);
	const expenseLines = $derived(summary?.categories.filter((c) => c.category.kind === 'expense') ?? []);
	const incomeLines = $derived(summary?.categories.filter((c) => c.category.kind === 'income' && Number(c.amount) > 0) ?? []);

	async function run(fn: () => Promise<unknown>) {
		error = '';
		info = '';
		try {
			if (!isTauri()) throw { code: 'notInTauri' };
			await fn();
		} catch (e) {
			error = errorText(e);
		}
	}

	async function load() {
		[balances, categories, portfolios] = await Promise.all([
			invoke<AccountBalance[]>('budget_balances'),
			db.list<Category>('category'),
			db.list<Portfolio>('portfolio')
		]);
		if (categories.length === 0) {
			await invoke('budget_seed_categories', { polish: settings.locale === 'pl' });
			categories = await db.list<Category>('category');
		}
		categories.sort((a, b) => a.name.localeCompare(b.name));
		await loadMonth();
		if (!txForm.accountId && accounts.length) txForm.accountId = accounts.find((a) => !a.archived)?.id ?? '';
		if (!targetPortfolio && portfolios.length) targetPortfolio = portfolios[0].id;
	}

	async function loadMonth() {
		[summary, transactions] = await Promise.all([
			invoke<MonthSummary>('budget_month', { month, currency: settings.baseCurrency }),
			invoke<Transaction[]>('budget_transactions', { month })
		]);
		transactions.reverse();
		limitInputs = Object.fromEntries((summary?.categories ?? []).map((c) => [c.category.id, c.limit ?? '']));
	}

	onMount(() => run(load));

	const shiftMonth = (offset: number) =>
		run(async () => {
			month = monthKey(new Date(month + 'T00:00:00'), offset);
			await loadMonth();
		});

	const addTransaction = () =>
		run(async () => {
			const amount = toDecimal(txForm.amount);
			if (txForm.type === 'transfer') {
				await invoke('budget_transfer', {
					input: {
						fromAccountId: txForm.accountId,
						toAccountId: txForm.toAccountId,
						date: txForm.date,
						amount,
						amountTo: needsAmountTo ? toDecimal(txForm.amountTo) : null,
						note: txForm.note
					}
				});
			} else {
				const signed = txForm.type === 'expense' ? `-${amount.replace(/^-/, '')}` : amount.replace(/^-/, '');
				await db.save<Transaction>('transaction', {
					accountId: txForm.accountId,
					date: txForm.date,
					amount: signed,
					categoryId: txForm.categoryId || null,
					transferPairId: null,
					note: txForm.note
				});
			}
			txForm = { ...emptyTx(txForm.accountId), type: txForm.type, date: txForm.date, categoryId: txForm.categoryId };
			await load();
		});

	const deleteTransaction = (tx: Transaction) =>
		run(async () => {
			if (!(await confirm(t('tx.deleteConfirm'), { kind: 'warning' }))) return;
			await db.remove('transaction', tx.id);
			await load();
		});

	const saveLimit = (categoryId: string) =>
		run(async () => {
			const raw = String(limitInputs[categoryId] ?? '').trim();
			await invoke('budget_set_limit', {
				categoryId,
				month,
				amount: raw === '' ? null : toDecimal(raw),
				currency: settings.baseCurrency
			});
			await loadMonth();
		});

	const copyLimits = () =>
		run(async () => {
			const count = await invoke<number>('budget_copy_limits', { month });
			await loadMonth();
			info = t('budget.copied', { count });
		});

	const addAccount = () =>
		run(async () => {
			await db.save<Account>('account', {
				...accountForm,
				openingBalance: toDecimal(accountForm.openingBalance),
				archived: false,
				color: null
			});
			accountForm = emptyAccount();
			await load();
		});

	const addCategory = () =>
		run(async () => {
			await db.save<Category>('category', {
				name: categoryForm.name.trim(),
				kind: categoryForm.kind,
				parentId: categoryForm.parentId || null,
				color: categoryForm.color,
				icon: null
			});
			categoryForm = { ...emptyCategory(), kind: categoryForm.kind };
			await load();
		});

	const deleteCategory = (c: Category) =>
		run(async () => {
			if (!(await confirm(t('cat.deleteConfirm', { name: c.name }), { kind: 'warning' }))) return;
			await db.remove('category', c.id);
			await load();
		});

	function describe(tx: Transaction): string {
		if (tx.transferPairId) {
			const pair = transactions.find((x) => x.id === tx.transferPairId);
			const other = pair ? accountById[pair.accountId]?.name ?? '?' : '?';
			return Number(tx.amount) < 0 ? t('tx.transferTo', { account: other }) : t('tx.transferFrom', { account: other });
		}
		const c = tx.categoryId ? categoryById[tx.categoryId] : null;
		if (!c) return t('budget.uncategorized');
		const parent = c.parentId ? categoryById[c.parentId] : null;
		return parent ? `${parent.name} › ${c.name}` : c.name;
	}

	const progress = (line: { amount: string; limit: string | null }) =>
		line.limit && Number(line.limit) > 0 ? Math.min(100, (Number(line.amount) / Number(line.limit)) * 100) : 0;
</script>

<div class="stack">
	<header class="head">
		<h1>{t('budget.title')}</h1>
		<div class="month-nav">
			<button class="btn ghost" aria-label="←" onclick={() => shiftMonth(-1)}>←</button>
			<strong class="month">{monthLabel}</strong>
			<button class="btn ghost" aria-label="→" onclick={() => shiftMonth(1)}>→</button>
		</div>
	</header>

	{#if error}<div class="notice error">{error}</div>{/if}
	{#if info}<div class="notice">{info}</div>{/if}
	{#if summary?.missingRates.length}
		<div class="notice warn">{t('budget.missingRates', { list: summary.missingRates.join(', ') })}</div>
	{/if}

	{#if summary}
		<div class="tiles">
			<section class="card">
				<span class="muted">{t('budget.income')}</span>
				<div class="big pos">{fmtMoney(summary.income, summary.currency)}</div>
			</section>
			<section class="card">
				<span class="muted">{t('budget.expense')}</span>
				<div class="big neg">{fmtMoney(summary.expense, summary.currency)}</div>
			</section>
			<section class="card">
				<span class="muted">{t('budget.net')}</span>
				<div class="big" class:pos={Number(summary.net) > 0} class:neg={Number(summary.net) < 0}>
					{fmtMoney(summary.net, summary.currency)}
				</div>
				{#if Number(summary.net) > 0 && portfolios.length}
					<div class="row surplus">
						{#if portfolios.length > 1}
							<select bind:value={targetPortfolio} aria-label={t('budget.toPortfolio')}>
								{#each portfolios as p (p.id)}<option value={p.id}>{p.name}</option>{/each}
							</select>
						{/if}
						<button
							class="btn primary"
							onclick={() => goto(`/portfolio/${targetPortfolio}?contribution=${summary?.net}`)}
						>
							{t('budget.toRebalance')} →
						</button>
					</div>
				{/if}
			</section>
		</div>
	{/if}

	<div class="columns">
		<div class="stack">
			<!-- Transactions -->
			<section class="card stack">
				<h2>{t('tx.title')}</h2>
				{#if accounts.length === 0}
					<p class="muted">{t('accounts.empty')}</p>
				{:else}
					<div class="segmented" role="radiogroup" aria-label={t('tx.type')}>
						{#each ['expense', 'income', 'transfer'] as const as ty (ty)}
							<button
								role="radio"
								aria-checked={txForm.type === ty}
								class:on={txForm.type === ty}
								onclick={() => {
									txForm.type = ty;
									txForm.categoryId = '';
								}}>{t(`tx.${ty}`)}</button
							>
						{/each}
					</div>
					<div class="row">
						<label class="field">
							{t('tx.date')}
							<input type="date" bind:value={txForm.date} />
						</label>
						<label class="field">
							{t('tx.account')}
							<select bind:value={txForm.accountId}>
								{#each accounts.filter((a) => !a.archived) as a (a.id)}<option value={a.id}>{a.name}</option>{/each}
							</select>
						</label>
						{#if txForm.type === 'transfer'}
							<label class="field">
								{t('tx.toAccount')}
								<select bind:value={txForm.toAccountId}>
									<option value="">{t('tx.none')}</option>
									{#each accounts.filter((a) => !a.archived && a.id !== txForm.accountId) as a (a.id)}
										<option value={a.id}>{a.name}</option>
									{/each}
								</select>
							</label>
						{:else}
							<label class="field">
								{t('tx.category')}
								<select bind:value={txForm.categoryId}>
									<option value="">{t('tx.none')}</option>
									{#each txCategories.filter((c) => !c.parentId) as c (c.id)}
										<option value={c.id}>{c.name}</option>
										{#each txCategories.filter((x) => x.parentId === c.id) as sub (sub.id)}
											<option value={sub.id}>&nbsp;&nbsp;› {sub.name}</option>
										{/each}
									{/each}
								</select>
							</label>
						{/if}
						<label class="field">
							{t('tx.amount')} {txAccount ? `(${txAccount.currency})` : ''}
							<input class="short" inputmode="decimal" bind:value={txForm.amount} />
						</label>
						{#if needsAmountTo}
							<label class="field">
								{t('tx.amountTo')} ({toAccount?.currency})
								<input class="short" inputmode="decimal" bind:value={txForm.amountTo} />
							</label>
						{/if}
						<label class="field grow">
							{t('tx.note')}
							<input bind:value={txForm.note} />
						</label>
						<button
							class="btn primary"
							disabled={!txForm.accountId ||
								!txForm.amount.trim() ||
								(txForm.type === 'transfer' && !txForm.toAccountId) ||
								(needsAmountTo && !txForm.amountTo.trim())}
							onclick={addTransaction}>{t('tx.add')}</button
						>
					</div>
				{/if}

				{#if transactions.length === 0}
					<p class="muted">{t('tx.empty')}</p>
				{:else}
					<div class="table-wrap">
						<table>
							<tbody>
								{#each transactions as tx (tx.id)}
									{@const acc = accountById[tx.accountId]}
									{@const cat = tx.categoryId ? categoryById[tx.categoryId] : null}
									<tr>
										<td class="muted">{tx.date.slice(5)}</td>
										<td class="wrap">
											<span class="desc">
												<span class="dot" style:background={cat?.color ?? 'var(--border)'}></span>
												<span>
													{describe(tx)}
													{#if tx.note}<span class="muted note">{tx.note}</span>{/if}
												</span>
											</span>
										</td>
										<td class="muted">{acc?.name}</td>
										<td class="num" class:pos={Number(tx.amount) > 0 && !tx.transferPairId} class:neg={Number(tx.amount) < 0 && !tx.transferPairId}>
											{fmtMoney(tx.amount, acc?.currency)}
										</td>
										<td>
											<button class="btn ghost" aria-label={t('common.delete')} onclick={() => deleteTransaction(tx)}
												><Icon name="trash" size={16} /></button
											>
										</td>
									</tr>
								{/each}
							</tbody>
						</table>
					</div>
				{/if}
			</section>

			<!-- Limits -->
			{#if summary}
				<section class="card stack">
					<div class="section-head">
						<h2>{t('budget.limits')}</h2>
						<button class="btn" onclick={copyLimits}>{t('budget.copyLimits')}</button>
					</div>
					{#each expenseLines as line (line.category.id)}
						{@const pct = progress(line)}
						{@const over = line.limit !== null && Number(line.amount) > Number(line.limit)}
						<div class="limit-row">
							<div class="limit-head">
								<span class="desc"><span class="dot" style:background={line.category.color ?? 'var(--border)'}></span>{line.category.name}</span>
								<span class="num">
									{fmtMoney(line.amount, summary.currency)}
									{#if line.limit !== null}
										<span class="muted">/ {fmtMoney(line.limit, summary.currency)}</span>
									{/if}
								</span>
							</div>
							{#if line.limit !== null}
								<div class="bar" class:over><div style:width={`${over ? 100 : pct}%`}></div></div>
								<div class="small" class:neg={over} class:muted={!over}>
									{over
										? t('budget.over', { amount: fmtMoney(Number(line.amount) - Number(line.limit), summary.currency) })
										: t('budget.left', { amount: fmtMoney(Number(line.limit) - Number(line.amount), summary.currency) })}
								</div>
							{/if}
							<form
								class="limit-edit"
								onsubmit={(e) => {
									e.preventDefault();
									saveLimit(line.category.id);
								}}
							>
								<input
									class="short"
									inputmode="decimal"
									placeholder={t('budget.noLimit')}
									bind:value={limitInputs[line.category.id]}
									aria-label={t('budget.limit')}
								/>
								<button class="btn" type="submit">{t('common.save')}</button>
							</form>
						</div>
					{/each}
					{#if Number(summary.uncategorizedExpense) > 0}
						<div class="limit-head muted">
							<span>{t('budget.uncategorized')}</span>
							<span class="num">{fmtMoney(summary.uncategorizedExpense, summary.currency)}</span>
						</div>
					{/if}
				</section>

				{#if incomeLines.length || Number(summary.uncategorizedIncome) > 0}
					<section class="card stack">
						<h2>{t('budget.incomeByCategory')}</h2>
						{#each incomeLines as line (line.category.id)}
							<div class="limit-head">
								<span class="desc"><span class="dot" style:background={line.category.color ?? 'var(--border)'}></span>{line.category.name}</span>
								<span class="num pos">{fmtMoney(line.amount, summary.currency)}</span>
							</div>
						{/each}
						{#if Number(summary.uncategorizedIncome) > 0}
							<div class="limit-head muted">
								<span>{t('budget.uncategorized')}</span>
								<span class="num">{fmtMoney(summary.uncategorizedIncome, summary.currency)}</span>
							</div>
						{/if}
					</section>
				{/if}
			{/if}
		</div>

		<aside class="stack">
			<!-- Accounts -->
			<section class="card stack">
				<h2>{t('accounts.title')}</h2>
				{#each balances as b (b.account.id)}
					<div class="limit-head" class:muted={b.account.archived}>
						<span>
							{b.account.name}
							<span class="muted small">· {t(`akind.${b.account.kind}`)}</span>
						</span>
						<span class="num" class:neg={Number(b.balance) < 0}>{fmtMoney(b.balance, b.account.currency)}</span>
					</div>
				{/each}
				{#if Object.keys(totalsByCurrency).length}
					<div class="total">
						{#each Object.entries(totalsByCurrency) as [cur, sum] (cur)}
							<div class="limit-head">
								<strong>{t('accounts.total', { currency: cur })}</strong>
								<strong class="num">{fmtMoney(sum, cur)}</strong>
							</div>
						{/each}
					</div>
				{/if}
				<h3>{t('accounts.add')}</h3>
				<label class="field">
					{t('accounts.name')}
					<input bind:value={accountForm.name} />
				</label>
				<div class="row">
					<label class="field">
						{t('accounts.kind')}
						<select bind:value={accountForm.kind}>
							{#each ACCOUNT_KINDS as k (k)}<option value={k}>{t(`akind.${k}`)}</option>{/each}
						</select>
					</label>
					<label class="field">
						{t('accounts.currency')}
						<select bind:value={accountForm.currency}>
							{#each CURRENCIES as c (c)}<option value={c}>{c}</option>{/each}
						</select>
					</label>
				</div>
				<label class="field">
					{t('accounts.opening')}
					<input inputmode="decimal" bind:value={accountForm.openingBalance} />
				</label>
				<button class="btn primary" disabled={!accountForm.name.trim()} onclick={addAccount}>{t('accounts.add')}</button>
			</section>

			<!-- Categories -->
			<section class="card stack">
				<h2>{t('cat.title')}</h2>
				{#each ['expense', 'income'] as const as kind (kind)}
					<div class="stack tight">
						<strong class="small muted">{t(`cat.${kind}`)}</strong>
						{#each categories.filter((c) => c.kind === kind && !c.parentId) as c (c.id)}
							<div class="cat-row">
								<span class="desc"><span class="dot" style:background={c.color ?? 'var(--border)'}></span>{c.name}</span>
								<button class="btn ghost" aria-label={t('common.delete')} onclick={() => deleteCategory(c)}><Icon name="trash" size={14} /></button>
							</div>
							{#each categories.filter((x) => x.parentId === c.id) as sub (sub.id)}
								<div class="cat-row sub">
									<span>› {sub.name}</span>
									<button class="btn ghost" aria-label={t('common.delete')} onclick={() => deleteCategory(sub)}><Icon name="trash" size={14} /></button>
								</div>
							{/each}
						{/each}
					</div>
				{/each}
				<h3>{t('cat.add')}</h3>
				<label class="field">
					{t('cat.name')}
					<input bind:value={categoryForm.name} />
				</label>
				<div class="row">
					<label class="field">
						{t('cat.kind')}
						<select bind:value={categoryForm.kind} onchange={() => (categoryForm.parentId = '')}>
							<option value="expense">{t('cat.expense')}</option>
							<option value="income">{t('cat.income')}</option>
						</select>
					</label>
					<label class="field">
						{t('cat.color')}
						<input type="color" bind:value={categoryForm.color} />
					</label>
				</div>
				<label class="field">
					{t('cat.parent')}
					<select bind:value={categoryForm.parentId}>
						<option value="">{t('tx.none')}</option>
						{#each categories.filter((c) => c.kind === categoryForm.kind && !c.parentId) as c (c.id)}
							<option value={c.id}>{c.name}</option>
						{/each}
					</select>
				</label>
				<button class="btn primary" disabled={!categoryForm.name.trim()} onclick={addCategory}>{t('cat.add')}</button>
			</section>
		</aside>
	</div>
</div>

<style>
	p {
		margin: 0;
	}
	h3 {
		font-size: 0.95rem;
		margin: calc(var(--space) * 0.5) 0 0;
	}
	.small {
		font-size: 0.8rem;
	}
	.head {
		display: flex;
		flex-wrap: wrap;
		justify-content: space-between;
		align-items: center;
		gap: var(--space);
	}
	.month-nav {
		display: flex;
		align-items: center;
		gap: 0.25rem;
	}
	.month {
		min-width: 11rem;
		text-align: center;
		text-transform: capitalize;
	}
	.tiles {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
		gap: var(--space);
	}
	.big {
		font-size: 1.5rem;
		font-weight: 700;
		font-variant-numeric: tabular-nums;
	}
	.surplus {
		margin-top: 0.5rem;
		align-items: center;
	}
	.columns {
		display: grid;
		grid-template-columns: minmax(0, 1fr) 320px;
		gap: var(--space);
		align-items: start;
	}
	@media (max-width: 1000px) {
		.columns {
			grid-template-columns: minmax(0, 1fr);
		}
	}
	.section-head {
		display: flex;
		flex-wrap: wrap;
		justify-content: space-between;
		align-items: center;
		gap: var(--space);
	}
	.segmented {
		display: inline-flex;
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
	.short {
		width: 7rem;
	}
	.grow {
		flex: 1 1 10rem;
	}
	.grow input {
		width: 100%;
	}
	.desc {
		display: inline-flex;
		align-items: center;
		gap: 0.5rem;
		white-space: normal;
	}
	td.wrap {
		white-space: normal;
		width: 100%;
	}
	.note {
		display: block;
		font-size: 0.8rem;
	}
	.dot {
		width: 10px;
		height: 10px;
		border-radius: 50%;
		flex: none;
	}
	.limit-row {
		display: flex;
		flex-direction: column;
		gap: 0.3rem;
		padding-bottom: calc(var(--space) * 0.5);
		border-bottom: 1px solid var(--border);
	}
	.limit-head {
		display: flex;
		justify-content: space-between;
		gap: 0.5rem;
		font-variant-numeric: tabular-nums;
	}
	.bar {
		height: 8px;
		border-radius: 4px;
		background: var(--surface-2);
		overflow: hidden;
	}
	.bar div {
		height: 100%;
		background: var(--accent);
		border-radius: 4px;
	}
	.bar.over div {
		background: var(--negative);
	}
	.limit-edit {
		display: flex;
		gap: 0.3rem;
	}
	.total {
		border-top: 1px solid var(--border);
		padding-top: calc(var(--space) * 0.5);
	}
	.tight {
		gap: 0.2rem;
	}
	.cat-row {
		display: flex;
		justify-content: space-between;
		align-items: center;
	}
	.cat-row.sub {
		padding-left: 1.2rem;
		font-size: 0.9rem;
	}
	aside input:not([type='color']),
	aside select {
		width: 100%;
	}
</style>
