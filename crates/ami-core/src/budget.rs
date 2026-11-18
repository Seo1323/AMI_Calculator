//! Household budget: account balances, transfers, monthly summaries and limits.

use std::collections::HashMap;

use chrono::{Datelike, NaiveDate};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{CoreError, Result};
use crate::model::*;
use crate::money::{Currency, Money};
use crate::store::Store;

/// First day of the month containing `date`.
pub fn month_start(date: NaiveDate) -> NaiveDate {
    date.with_day(1).expect("day 1 always exists")
}

/// First day of the following month.
pub fn next_month(month: NaiveDate) -> NaiveDate {
    let m = month_start(month);
    if m.month() == 12 {
        NaiveDate::from_ymd_opt(m.year() + 1, 1, 1).unwrap()
    } else {
        NaiveDate::from_ymd_opt(m.year(), m.month() + 1, 1).unwrap()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountBalance {
    pub account: Account,
    pub balance: Decimal,
}

pub fn account_balances(store: &Store) -> Result<Vec<AccountBalance>> {
    let mut sums: HashMap<Uuid, Decimal> = HashMap::new();
    for t in store.list::<Transaction>()? {
        *sums.entry(t.account_id).or_default() += t.amount;
    }
    let mut out: Vec<AccountBalance> = store
        .list::<Account>()?
        .into_iter()
        .map(|a| AccountBalance { balance: a.opening_balance + sums.get(&a.meta.id).copied().unwrap_or_default(), account: a })
        .collect();
    out.sort_by(|a, b| a.account.archived.cmp(&b.account.archived).then_with(|| a.account.name.cmp(&b.account.name)));
    Ok(out)
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferInput {
    pub from_account_id: Uuid,
    pub to_account_id: Uuid,
    pub date: NaiveDate,
    /// Amount leaving the source account (positive, source currency).
    pub amount: Decimal,
    /// Amount arriving (target currency); defaults to `amount` for same-currency transfers.
    #[serde(default)]
    pub amount_to: Option<Decimal>,
    #[serde(default)]
    pub note: String,
}

/// Creates the two linked transactions of a transfer. Returns (outgoing, incoming).
pub fn create_transfer(store: &Store, input: &TransferInput) -> Result<(Transaction, Transaction)> {
    if input.from_account_id == input.to_account_id {
        return Err(CoreError::Validation("transfer needs two different accounts".into()));
    }
    if input.amount <= Decimal::ZERO || input.amount_to.is_some_and(|a| a <= Decimal::ZERO) {
        return Err(CoreError::Validation("transfer amount must be positive".into()));
    }
    let from = store.get::<Account>(input.from_account_id)?.ok_or(CoreError::NotFound)?;
    let to = store.get::<Account>(input.to_account_id)?.ok_or(CoreError::NotFound)?;
    let amount_to = match input.amount_to {
        Some(a) => a,
        None if from.currency == to.currency => input.amount,
        None => return Err(CoreError::Validation("amount in the target currency is required".into())),
    };
    let out_meta = Meta::new();
    let in_meta = Meta::new();
    let outgoing = Transaction {
        transfer_pair_id: Some(in_meta.id),
        meta: out_meta,
        account_id: from.meta.id,
        date: input.date,
        amount: -input.amount,
        category_id: None,
        note: input.note.clone(),
    };
    let incoming = Transaction {
        transfer_pair_id: Some(outgoing.meta.id),
        meta: in_meta,
        account_id: to.meta.id,
        date: input.date,
        amount: amount_to,
        category_id: None,
        note: input.note.clone(),
    };
    Ok((store.save(outgoing)?, store.save(incoming)?))
}

/// Deletes a transaction and, for transfers, its pair.
pub fn delete_transaction(store: &Store, id: Uuid) -> Result<bool> {
    let Some(t) = store.get::<Transaction>(id)? else { return Ok(false) };
    if let Some(pair) = t.transfer_pair_id {
        store.delete::<Transaction>(pair)?;
    }
    store.delete::<Transaction>(id)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CategoryLine {
    pub category: Category,
    /// Spent (expense, positive number) or earned (income) this month, base currency.
    pub amount: Decimal,
    pub limit: Option<Decimal>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MonthSummary {
    pub month: NaiveDate,
    pub currency: Currency,
    pub income: Decimal,
    pub expense: Decimal,
    /// income - expense
    pub net: Decimal,
    pub uncategorized_expense: Decimal,
    pub uncategorized_income: Decimal,
    /// Top-level categories (children are rolled up into their parent).
    pub categories: Vec<CategoryLine>,
    /// Currencies of accounts that could not be converted.
    pub missing_rates: Vec<String>,
}

/// Monthly totals in `currency`. Transfers are excluded.
pub fn month_summary(store: &Store, month: NaiveDate, currency: &Currency) -> Result<MonthSummary> {
    let month = month_start(month);
    let end = next_month(month);
    let fx = store.fx_table()?;
    let accounts: HashMap<Uuid, Account> = store.list::<Account>()?.into_iter().map(|a| (a.meta.id, a)).collect();
    let categories: Vec<Category> = store.list::<Category>()?;
    // A child of a deleted parent is reported as top-level.
    let live: std::collections::HashSet<Uuid> = categories.iter().map(|c| c.meta.id).collect();
    let parent_of: HashMap<Uuid, Uuid> = categories
        .iter()
        .map(|c| (c.meta.id, c.parent_id.filter(|p| live.contains(p)).unwrap_or(c.meta.id)))
        .collect();

    let mut income = Decimal::ZERO;
    let mut expense = Decimal::ZERO;
    let mut uncategorized_expense = Decimal::ZERO;
    let mut uncategorized_income = Decimal::ZERO;
    let mut per_top: HashMap<Uuid, Decimal> = HashMap::new();
    let mut missing_rates: Vec<String> = Vec::new();

    for t in store.transactions_between(month, end)? {
        if t.transfer_pair_id.is_some() {
            continue;
        }
        let Some(account) = accounts.get(&t.account_id) else { continue };
        let Ok(converted) = fx.convert(&Money::new(t.amount, account.currency.clone()), currency) else {
            if !missing_rates.contains(&account.currency.to_string()) {
                missing_rates.push(account.currency.to_string());
            }
            continue;
        };
        let amount = converted.amount;
        if amount >= Decimal::ZERO {
            income += amount;
        } else {
            expense -= amount;
        }
        match t.category_id.and_then(|c| parent_of.get(&c)) {
            Some(top) => *per_top.entry(*top).or_default() += amount.abs(),
            None if amount < Decimal::ZERO => uncategorized_expense -= amount,
            None => uncategorized_income += amount,
        }
    }

    let limits: HashMap<Uuid, Decimal> = store
        .list::<BudgetLimit>()?
        .into_iter()
        .filter(|l| l.month == month)
        .filter_map(|l| fx.convert(&Money::new(l.amount, l.currency), currency).ok().map(|m| (l.category_id, m.amount)))
        .collect();

    let mut lines: Vec<CategoryLine> = categories
        .into_iter()
        .filter(|c| c.parent_id.is_none_or(|p| !live.contains(&p)))
        .map(|c| CategoryLine {
            amount: per_top.get(&c.meta.id).copied().unwrap_or_default().round_dp(2),
            limit: limits.get(&c.meta.id).copied(),
            category: c,
        })
        .collect();
    lines.sort_by(|a, b| b.amount.cmp(&a.amount).then_with(|| a.category.name.cmp(&b.category.name)));

    Ok(MonthSummary {
        month,
        currency: currency.clone(),
        income: income.round_dp(2),
        expense: expense.round_dp(2),
        net: (income - expense).round_dp(2),
        uncategorized_expense: uncategorized_expense.round_dp(2),
        uncategorized_income: uncategorized_income.round_dp(2),
        categories: lines,
        missing_rates,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MonthPoint {
    pub month: NaiveDate,
    pub income: Decimal,
    pub expense: Decimal,
    pub net: Decimal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PeriodReport {
    pub currency: Currency,
    pub from: NaiveDate,
    /// First day of the last month included.
    pub to: NaiveDate,
    pub months: Vec<MonthPoint>,
    pub income: Decimal,
    pub expense: Decimal,
    pub net: Decimal,
    /// Average monthly expense over the period.
    pub avg_expense: Decimal,
    /// Share of income kept (net / income), in percent; None without income.
    pub savings_rate: Option<Decimal>,
    /// Totals per top-level category over the whole period (limits = sum of monthly limits).
    pub categories: Vec<CategoryLine>,
    pub uncategorized_expense: Decimal,
    pub uncategorized_income: Decimal,
    pub missing_rates: Vec<String>,
}

/// Month-by-month totals for `[from_month, to_month]` (both inclusive, any day in the month).
pub fn period_report(store: &Store, from_month: NaiveDate, to_month: NaiveDate, currency: &Currency) -> Result<PeriodReport> {
    let from = month_start(from_month.min(to_month));
    let to = month_start(from_month.max(to_month));
    let mut months = Vec::new();
    let mut totals: HashMap<Uuid, CategoryLine> = HashMap::new();
    let mut order: Vec<Uuid> = Vec::new();
    let (mut unc_e, mut unc_i) = (Decimal::ZERO, Decimal::ZERO);
    let mut missing: Vec<String> = Vec::new();
    let mut m = from;
    // Hard cap so a typo like year 1026 cannot spin for ages.
    while m <= to && months.len() < 240 {
        let s = month_summary(store, m, currency)?;
        months.push(MonthPoint { month: m, income: s.income, expense: s.expense, net: s.net });
        for line in s.categories {
            let id = line.category.meta.id;
            match totals.get_mut(&id) {
                Some(t) => {
                    t.amount += line.amount;
                    t.limit = match (t.limit, line.limit) {
                        (Some(a), Some(b)) => Some(a + b),
                        (a, b) => a.or(b),
                    };
                }
                None => {
                    order.push(id);
                    totals.insert(id, line);
                }
            }
        }
        unc_e += s.uncategorized_expense;
        unc_i += s.uncategorized_income;
        for c in s.missing_rates {
            if !missing.contains(&c) {
                missing.push(c);
            }
        }
        m = next_month(m);
    }
    let income: Decimal = months.iter().map(|p| p.income).sum();
    let expense: Decimal = months.iter().map(|p| p.expense).sum();
    let mut categories: Vec<CategoryLine> = order.into_iter().filter_map(|id| totals.remove(&id)).collect();
    categories.sort_by(|a, b| b.amount.cmp(&a.amount).then_with(|| a.category.name.cmp(&b.category.name)));
    let n = Decimal::from(months.len().max(1) as u64);
    Ok(PeriodReport {
        currency: currency.clone(),
        from,
        to,
        income,
        expense,
        net: income - expense,
        avg_expense: (expense / n).round_dp(2),
        savings_rate: (income > Decimal::ZERO).then(|| ((income - expense) / income * Decimal::ONE_HUNDRED).round_dp(1)),
        months,
        categories,
        uncategorized_expense: unc_e,
        uncategorized_income: unc_i,
        missing_rates: missing,
    })
}

/// Sets (or clears, with `None`) a category's limit for one month.
pub fn set_limit(store: &Store, category_id: Uuid, month: NaiveDate, amount: Option<Decimal>, currency: Currency) -> Result<()> {
    let month = month_start(month);
    let existing = store.list::<BudgetLimit>()?.into_iter().find(|l| l.category_id == category_id && l.month == month);
    match (existing, amount) {
        (Some(l), None) => {
            store.delete::<BudgetLimit>(l.meta.id)?;
        }
        (Some(l), Some(amount)) => {
            store.save(BudgetLimit { amount, currency, ..l })?;
        }
        (None, Some(amount)) => {
            store.save(BudgetLimit { meta: Meta::new(), category_id, month, amount, currency })?;
        }
        (None, None) => {}
    }
    Ok(())
}

/// Copies last month's limits into `month` for categories that have none yet.
pub fn copy_limits_from_previous(store: &Store, month: NaiveDate) -> Result<usize> {
    let month = month_start(month);
    let prev = month_start(month - chrono::Duration::days(1));
    let all = store.list::<BudgetLimit>()?;
    let mut copied = 0;
    for l in all.iter().filter(|l| l.month == prev) {
        if !all.iter().any(|x| x.month == month && x.category_id == l.category_id) {
            store.save(BudgetLimit { meta: Meta::new(), month, ..l.clone() })?;
            copied += 1;
        }
    }
    Ok(copied)
}

/// Creates a starter set of categories if there are none. Returns how many were created.
pub fn seed_default_categories(store: &Store, polish: bool) -> Result<usize> {
    if !store.list::<Category>()?.is_empty() {
        return Ok(0);
    }
    let defaults: &[(&str, &str, CategoryKind, &str)] = &[
        ("Jedzenie", "Food", CategoryKind::Expense, "#0f9d76"),
        ("Dom i rachunki", "Home & bills", CategoryKind::Expense, "#2f6fed"),
        ("Transport", "Transport", CategoryKind::Expense, "#c2410c"),
        ("Zdrowie", "Health", CategoryKind::Expense, "#db2777"),
        ("Rozrywka", "Entertainment", CategoryKind::Expense, "#9333ea"),
        ("Zakupy", "Shopping", CategoryKind::Expense, "#ca8a04"),
        ("Subskrypcje", "Subscriptions", CategoryKind::Expense, "#0891b2"),
        ("Inne wydatki", "Other expenses", CategoryKind::Expense, "#475569"),
        ("Wynagrodzenie", "Salary", CategoryKind::Income, "#0f9d76"),
        ("Inne przychody", "Other income", CategoryKind::Income, "#475569"),
    ];
    for (pl, en, kind, color) in defaults {
        store.save(Category {
            meta: Meta::new(),
            name: if polish { pl } else { en }.to_string(),
            kind: *kind,
            parent_id: None,
            color: Some(color.to_string()),
            icon: None,
        })?;
    }
    Ok(defaults.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use rust_decimal_macros::dec;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    fn pln() -> Currency {
        Currency::new("PLN").unwrap()
    }

    fn account(store: &Store, name: &str, currency: &str, opening: Decimal) -> Account {
        store
            .save(Account {
                meta: Meta::new(),
                name: name.into(),
                kind: AccountKind::Checking,
                currency: Currency::new(currency).unwrap(),
                opening_balance: opening,
                archived: false,
                color: None,
            })
            .unwrap()
    }

    fn category(store: &Store, name: &str, kind: CategoryKind, parent: Option<Uuid>) -> Category {
        store
            .save(Category { meta: Meta::new(), name: name.into(), kind, parent_id: parent, color: None, icon: None })
            .unwrap()
    }

    fn tx(store: &Store, account: &Account, date: NaiveDate, amount: Decimal, category: Option<&Category>) -> Transaction {
        store
            .save(Transaction {
                meta: Meta::new(),
                account_id: account.meta.id,
                date,
                amount,
                category_id: category.map(|c| c.meta.id),
                transfer_pair_id: None,
                note: String::new(),
            })
            .unwrap()
    }

    #[test]
    fn month_helpers() {
        assert_eq!(month_start(d(2026, 11, 18)), d(2026, 11, 1));
        assert_eq!(next_month(d(2026, 12, 5)), d(2027, 1, 1));
    }

    #[test]
    fn balances_and_transfers() {
        let s = Store::open_in_memory().unwrap();
        let a = account(&s, "Konto", "PLN", dec!(1000));
        let b = account(&s, "Oszczędności", "PLN", dec!(0));
        tx(&s, &a, d(2026, 11, 2), dec!(-200), None);
        let (out, inc) = create_transfer(
            &s,
            &TransferInput { from_account_id: a.meta.id, to_account_id: b.meta.id, date: d(2026, 11, 3), amount: dec!(300), amount_to: None, note: String::new() },
        )
        .unwrap();
        assert_eq!(out.transfer_pair_id, Some(inc.meta.id));
        let balances: HashMap<String, Decimal> =
            account_balances(&s).unwrap().into_iter().map(|b| (b.account.name, b.balance)).collect();
        assert_eq!(balances["Konto"], dec!(500));
        assert_eq!(balances["Oszczędności"], dec!(300));

        // Deleting one side removes the pair.
        assert!(delete_transaction(&s, inc.meta.id).unwrap());
        let balances: HashMap<String, Decimal> =
            account_balances(&s).unwrap().into_iter().map(|b| (b.account.name, b.balance)).collect();
        assert_eq!(balances["Konto"], dec!(800));
        assert_eq!(balances["Oszczędności"], dec!(0));
    }

    #[test]
    fn cross_currency_transfer_needs_target_amount() {
        let s = Store::open_in_memory().unwrap();
        let a = account(&s, "PLN", "PLN", dec!(0));
        let b = account(&s, "EUR", "EUR", dec!(0));
        let mut input = TransferInput { from_account_id: a.meta.id, to_account_id: b.meta.id, date: d(2026, 1, 1), amount: dec!(430), amount_to: None, note: String::new() };
        assert!(matches!(create_transfer(&s, &input), Err(CoreError::Validation(_))));
        input.amount_to = Some(dec!(100));
        let (_, inc) = create_transfer(&s, &input).unwrap();
        assert_eq!(inc.amount, dec!(100));
    }

    #[test]
    fn summary_rolls_up_children_and_skips_transfers() {
        let s = Store::open_in_memory().unwrap();
        let a = account(&s, "Konto", "PLN", dec!(0));
        let eur = account(&s, "Euro", "EUR", dec!(0));
        let b = account(&s, "Drugie", "PLN", dec!(0));
        let food = category(&s, "Jedzenie", CategoryKind::Expense, None);
        let restaurants = category(&s, "Restauracje", CategoryKind::Expense, Some(food.meta.id));
        let salary = category(&s, "Pensja", CategoryKind::Income, None);

        tx(&s, &a, d(2026, 11, 1), dec!(5000), Some(&salary));
        tx(&s, &a, d(2026, 11, 5), dec!(-300), Some(&food));
        tx(&s, &a, d(2026, 11, 6), dec!(-120.50), Some(&restaurants));
        tx(&s, &a, d(2026, 11, 7), dec!(-50), None);
        tx(&s, &eur, d(2026, 11, 8), dec!(-10), Some(&food)); // no EUR rate yet
        tx(&s, &a, d(2026, 10, 31), dec!(-999), Some(&food)); // previous month
        create_transfer(
            &s,
            &TransferInput { from_account_id: a.meta.id, to_account_id: b.meta.id, date: d(2026, 11, 9), amount: dec!(1000), amount_to: None, note: String::new() },
        )
        .unwrap();
        set_limit(&s, food.meta.id, d(2026, 11, 15), Some(dec!(500)), pln()).unwrap();

        let m = month_summary(&s, d(2026, 11, 20), &pln()).unwrap();
        assert_eq!(m.income, dec!(5000));
        assert_eq!(m.expense, dec!(470.50));
        assert_eq!(m.net, dec!(4529.50));
        assert_eq!(m.uncategorized_expense, dec!(50));
        assert_eq!(m.missing_rates, ["EUR"]);
        let food_line = m.categories.iter().find(|c| c.category.name == "Jedzenie").unwrap();
        assert_eq!(food_line.amount, dec!(420.50));
        assert_eq!(food_line.limit, Some(dec!(500)));
        assert!(m.categories.iter().all(|c| c.category.name != "Restauracje"));

        s.set_fx_rate(&FxRate { currency: Currency::new("EUR").unwrap(), rate_pln: dec!(4.3), source: "manual".into(), fetched_at: Utc::now() })
            .unwrap();
        let m = month_summary(&s, d(2026, 11, 1), &pln()).unwrap();
        assert_eq!(m.expense, dec!(513.50));
        assert!(m.missing_rates.is_empty());
    }

    #[test]
    fn period_report_sums_months() {
        let s = Store::open_in_memory().unwrap();
        let a = account(&s, "Konto", "PLN", dec!(0));
        let food = category(&s, "Jedzenie", CategoryKind::Expense, None);
        let salary = category(&s, "Pensja", CategoryKind::Income, None);
        for (m, inc, exp) in [(9, dec!(5000), dec!(-1000)), (10, dec!(5000), dec!(-2000)), (11, dec!(6000), dec!(-1500))] {
            tx(&s, &a, d(2026, m, 1), inc, Some(&salary));
            tx(&s, &a, d(2026, m, 10), exp, Some(&food));
        }
        set_limit(&s, food.meta.id, d(2026, 10, 1), Some(dec!(1800)), pln()).unwrap();
        set_limit(&s, food.meta.id, d(2026, 11, 1), Some(dec!(1800)), pln()).unwrap();

        let r = period_report(&s, d(2026, 11, 30), d(2026, 9, 5), &pln()).unwrap();
        assert_eq!(r.from, d(2026, 9, 1));
        assert_eq!(r.months.len(), 3);
        assert_eq!(r.months[1].expense, dec!(2000));
        assert_eq!(r.income, dec!(16000));
        assert_eq!(r.expense, dec!(4500));
        assert_eq!(r.avg_expense, dec!(1500));
        assert_eq!(r.savings_rate, Some(dec!(71.9)));
        let food_line = r.categories.iter().find(|c| c.category.name == "Jedzenie").unwrap();
        assert_eq!(food_line.amount, dec!(4500));
        assert_eq!(food_line.limit, Some(dec!(3600)));
    }

    #[test]
    fn limits_set_clear_and_copy() {
        let s = Store::open_in_memory().unwrap();
        let food = category(&s, "Jedzenie", CategoryKind::Expense, None);
        set_limit(&s, food.meta.id, d(2026, 10, 1), Some(dec!(400)), pln()).unwrap();
        set_limit(&s, food.meta.id, d(2026, 10, 9), Some(dec!(450)), pln()).unwrap();
        assert_eq!(s.list::<BudgetLimit>().unwrap().len(), 1);
        assert_eq!(copy_limits_from_previous(&s, d(2026, 11, 1)).unwrap(), 1);
        assert_eq!(copy_limits_from_previous(&s, d(2026, 11, 1)).unwrap(), 0);
        set_limit(&s, food.meta.id, d(2026, 10, 1), None, pln()).unwrap();
        let left = s.list::<BudgetLimit>().unwrap();
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].amount, dec!(450));
        assert_eq!(left[0].month, d(2026, 11, 1));
    }

    #[test]
    fn default_categories_seed_once() {
        let s = Store::open_in_memory().unwrap();
        assert_eq!(seed_default_categories(&s, true).unwrap(), 10);
        assert_eq!(seed_default_categories(&s, true).unwrap(), 0);
        assert!(s.list::<Category>().unwrap().iter().any(|c| c.name == "Jedzenie"));
    }
}
