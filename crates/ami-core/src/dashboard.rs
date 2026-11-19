//! One-call overview for the dashboard: total portfolio value and portfolio tiles.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

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
    pub total_value: Decimal,
    pub total_gain: Decimal,
    /// Currencies that could not be converted (their amounts are left out).
    pub missing_rates: Vec<String>,
    pub portfolios: Vec<PortfolioTile>,
}

pub fn build(store: &Store, currency: &Currency) -> Result<Dashboard> {
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

    let mut total_value = Decimal::ZERO;
    let mut total_gain = Decimal::ZERO;
    let mut tiles = Vec::new();
    let mut list = store.list::<Portfolio>()?;
    list.sort_by(|a, b| a.name.cmp(&b.name));
    for p in list {
        let v = portfolio::view(store, p.meta.id)?;
        let base = &v.portfolio.base_currency;
        let value = convert(v.total_value, base);
        let gain = convert(v.total_gain, base);
        total_value += value.unwrap_or_default();
        total_gain += gain.unwrap_or_default();
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

    Ok(Dashboard {
        currency: currency.clone(),
        total_value: total_value.round_dp(2),
        total_gain: total_gain.round_dp(2),
        missing_rates: missing,
        portfolios: tiles,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::*;
    use chrono::{NaiveDate, Utc};
    use rust_decimal_macros::dec;

    #[test]
    fn total_converts_and_reports_missing_rates() {
        let s = Store::open_in_memory().unwrap();
        let pln = Currency::new("PLN").unwrap();
        let eur = Currency::new("EUR").unwrap();
        let mut ids = Vec::new();
        for (name, cur) in [("PLN", &pln), ("EUR", &eur)] {
            let p = s
                .save(Portfolio {
                    meta: Meta::new(),
                    name: name.into(),
                    base_currency: cur.clone(),
                    tolerance_percent: dec!(5),
                })
                .unwrap();
            let i = s
                .save(Instrument {
                    meta: Meta::new(),
                    symbol: name.into(),
                    name: name.into(),
                    kind: InstrumentKind::Etf,
                    currency: cur.clone(),
                    quote_source: QuoteSource::Manual,
                    fractional: false,
                    interest_percent: None,
                    start_date: None,
                    maturity_date: None,
                })
                .unwrap();
            s.save(Trade {
                meta: Meta::new(),
                portfolio_id: p.meta.id,
                instrument_id: i.meta.id,
                date: NaiveDate::from_ymd_opt(2026, 11, 18).unwrap(),
                side: TradeSide::Buy,
                quantity: dec!(10),
                price: dec!(100),
                fee: dec!(0),
            })
            .unwrap();
            s.set_quote(&Quote {
                instrument_id: i.meta.id,
                price: dec!(100),
                currency: cur.clone(),
                source: "manual".into(),
                fetched_at: Utc::now(),
                return_12m_percent: None,
                dividend_yield_percent: None,
            })
            .unwrap();
            ids.push(p.meta.id);
        }
        let d = build(&s, &pln).unwrap();
        assert_eq!(d.total_value, dec!(1000));
        assert_eq!(d.missing_rates, ["EUR"]);

        s.set_fx_rate(&FxRate {
            currency: eur,
            rate_pln: dec!(4.25),
            source: "nbp".into(),
            fetched_at: Utc::now(),
        })
        .unwrap();
        let d = build(&s, &pln).unwrap();
        assert_eq!(d.total_value, dec!(5250));
        assert!(d.missing_rates.is_empty());
    }
}
