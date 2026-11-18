//! One-call overview for the dashboard: net worth, this month's budget, portfolios.

use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::budget::{self, MonthSummary};
use crate::error::Result;
use crate::model::Portfolio;
use crate::money::{Currency, Money};
use crate::portfolio;
use crate::store::Store;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AllocationSlice {
    pub instrument_id: Uuid,
    pub symbol: String,
    pub current_percent: Decimal,
    pub target_percent: Decimal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PortfolioTile {
    pub id: Uuid,
    pub name: String,
    /// In the dashboard currency (None if it cannot be converted).
    pub value: Option<Decimal>,
    pub gain: Option<Decimal>,
    pub max_deviation: Option<Decimal>,
    pub needs_rebalance: bool,
    pub complete: bool,
    pub allocation: Vec<AllocationSlice>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Dashboard {
    pub currency: Currency,
    pub accounts_total: Decimal,
    pub portfolios_total: Decimal,
    pub net_worth: Decimal,
    /// Currencies that could not be converted (their amounts are left out).
    pub missing_rates: Vec<String>,
    pub month: MonthSummary,
    pub portfolios: Vec<PortfolioTile>,
}

pub fn build(store: &Store, today: NaiveDate, currency: &Currency) -> Result<Dashboard> {
    let fx = store.fx_table()?;
    let mut missing: Vec<String> = Vec::new();
    let mut note_missing = |c: &Currency| {
        if !missing.contains(&c.to_string()) {
            missing.push(c.to_string());
        }
    };
    let mut convert = |amount: Decimal, from: &Currency| -> Option<Decimal> {
        match fx.convert(&Money::new(amount, from.clone()), currency) {
            Ok(m) => Some(m.amount),
            Err(_) => {
                note_missing(from);
                None
            }
        }
    };

    let mut accounts_total = Decimal::ZERO;
    for b in budget::account_balances(store)? {
        if !b.account.archived
            && let Some(v) = convert(b.balance, &b.account.currency)
        {
            accounts_total += v;
        }
    }

    let mut portfolios_total = Decimal::ZERO;
    let mut tiles = Vec::new();
    let mut list = store.list::<Portfolio>()?;
    list.sort_by(|a, b| a.name.cmp(&b.name));
    for p in list {
        let v = portfolio::view(store, p.meta.id)?;
        let base = &v.portfolio.base_currency;
        let value = convert(v.total_value, base);
        let gain = convert(v.total_gain, base);
        portfolios_total += value.unwrap_or_default();
        tiles.push(PortfolioTile {
            id: p.meta.id,
            name: p.name,
            value: value.map(|x| x.round_dp(2)),
            gain: gain.map(|x| x.round_dp(2)),
            max_deviation: v.max_deviation,
            needs_rebalance: v.needs_rebalance,
            complete: v.missing_prices.is_empty() && v.missing_rates.is_empty(),
            allocation: v
                .positions
                .iter()
                .map(|pos| AllocationSlice {
                    instrument_id: pos.instrument.meta.id,
                    symbol: pos.instrument.symbol.clone(),
                    current_percent: pos.current_percent.unwrap_or_default(),
                    target_percent: pos.target_percent,
                })
                .collect(),
        });
    }

    let month = budget::month_summary(store, today, currency)?;
    for c in &month.missing_rates {
        if !missing.contains(c) {
            missing.push(c.clone());
        }
    }

    Ok(Dashboard {
        currency: currency.clone(),
        accounts_total: accounts_total.round_dp(2),
        portfolios_total: portfolios_total.round_dp(2),
        net_worth: (accounts_total + portfolios_total).round_dp(2),
        missing_rates: missing,
        month,
        portfolios: tiles,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::*;
    use chrono::Utc;
    use rust_decimal_macros::dec;

    #[test]
    fn net_worth_converts_and_reports_missing_rates() {
        let s = Store::open_in_memory().unwrap();
        let pln = Currency::new("PLN").unwrap();
        let eur = Currency::new("EUR").unwrap();
        for (name, cur, bal) in [("PLN", &pln, dec!(1000)), ("EUR", &eur, dec!(100))] {
            s.save(Account {
                meta: Meta::new(),
                name: name.into(),
                kind: AccountKind::Checking,
                currency: cur.clone(),
                opening_balance: bal,
                archived: false,
                color: None,
            })
            .unwrap();
        }
        let today = NaiveDate::from_ymd_opt(2026, 11, 18).unwrap();
        let d = build(&s, today, &pln).unwrap();
        assert_eq!(d.accounts_total, dec!(1000));
        assert_eq!(d.missing_rates, ["EUR"]);

        s.set_fx_rate(&FxRate { currency: eur, rate_pln: dec!(4.25), source: "nbp".into(), fetched_at: Utc::now() })
            .unwrap();
        let d = build(&s, today, &pln).unwrap();
        assert_eq!(d.net_worth, dec!(1425));
        assert!(d.missing_rates.is_empty());
    }
}
