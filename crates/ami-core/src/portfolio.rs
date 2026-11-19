//! Portfolio state derived from trades, prices and exchange rates.

use std::collections::{BTreeMap, HashMap};

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{CoreError, Result};
use crate::fx::FxTable;
use crate::model::*;
use crate::money::Money;
use crate::rebalance::{self, Mode, Position, RebalanceInput, RebalanceResult};
use crate::store::Store;

const HUNDRED: Decimal = Decimal::ONE_HUNDRED;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Holding {
    pub quantity: Decimal,
    /// Remaining cost basis (average-cost method) incl. fees, in instrument currency.
    pub cost: Decimal,
    /// Realised gain/loss from sells, in instrument currency.
    pub realized: Decimal,
}

/// Replays trades in date order. Fails if a sell exceeds the quantity held.
pub fn holdings(trades: &[Trade]) -> Result<BTreeMap<Uuid, Holding>> {
    let mut sorted: Vec<&Trade> = trades
        .iter()
        .filter(|t| t.meta.deleted_at.is_none())
        .collect();
    sorted.sort_by_key(|t| (t.date, t.meta.created_at));
    let mut map: BTreeMap<Uuid, Holding> = BTreeMap::new();
    for t in sorted {
        let h = map.entry(t.instrument_id).or_default();
        match t.side {
            TradeSide::Buy => {
                h.quantity += t.quantity;
                h.cost += t.quantity * t.price + t.fee;
            }
            TradeSide::Sell => {
                if t.quantity > h.quantity {
                    return Err(CoreError::Oversold {
                        date: t.date.to_string(),
                    });
                }
                let avg = h.cost / h.quantity;
                let cost_out = avg * t.quantity;
                h.realized += t.quantity * t.price - t.fee - cost_out;
                h.cost -= cost_out;
                h.quantity -= t.quantity;
                if h.quantity.is_zero() {
                    h.cost = Decimal::ZERO;
                }
            }
        }
    }
    Ok(map)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PositionView {
    pub instrument: Instrument,
    pub quantity: Decimal,
    /// Average purchase price in instrument currency.
    pub avg_cost: Option<Decimal>,
    pub price: Option<Decimal>,
    pub price_at: Option<DateTime<Utc>>,
    pub price_source: Option<String>,
    /// Value in the portfolio base currency (None if price or FX rate is missing).
    pub value: Option<Decimal>,
    /// Unrealised gain in base currency.
    pub gain: Option<Decimal>,
    pub realized: Decimal,
    pub target_percent: Decimal,
    pub current_percent: Option<Decimal>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PortfolioView {
    pub portfolio: Portfolio,
    pub positions: Vec<PositionView>,
    /// Sum of positions whose value is known, in base currency.
    pub total_value: Decimal,
    pub total_gain: Decimal,
    pub targets_sum: Decimal,
    /// Symbols without a price.
    pub missing_prices: Vec<String>,
    /// Currencies without an exchange rate.
    pub missing_rates: Vec<String>,
    pub max_deviation: Option<Decimal>,
    pub needs_rebalance: bool,
}

struct Inputs {
    portfolio: Portfolio,
    instruments: HashMap<Uuid, Instrument>,
    holdings: BTreeMap<Uuid, Holding>,
    targets: HashMap<Uuid, Decimal>,
    quotes: HashMap<Uuid, Quote>,
    fx: FxTable,
}

fn load(store: &Store, portfolio_id: Uuid) -> Result<Inputs> {
    let portfolio = store
        .get::<Portfolio>(portfolio_id)?
        .ok_or(CoreError::NotFound)?;
    let instruments: HashMap<Uuid, Instrument> = store
        .list::<Instrument>()?
        .into_iter()
        .map(|i| (i.meta.id, i))
        .collect();
    let mut quotes: HashMap<Uuid, Quote> = store
        .quotes()?
        .into_iter()
        .map(|q| (q.instrument_id, q))
        .collect();
    // Cash and deposits need no price entry: one unit is one unit of their own currency.
    for i in instruments
        .values()
        .filter(|i| matches!(i.kind, InstrumentKind::Cash | InstrumentKind::Deposit))
    {
        quotes.entry(i.meta.id).or_insert_with(|| Quote {
            instrument_id: i.meta.id,
            price: Decimal::ONE,
            currency: i.currency.clone(),
            source: "cash".into(),
            fetched_at: chrono::Utc::now(),
            return_12m_percent: None,
            dividend_yield_percent: None,
        });
    }
    let holdings = holdings(&store.trades_of(portfolio_id)?)?;
    // A bond has no market feed: until its price is entered it is valued at what it cost, not at zero.
    for (id, h) in &holdings {
        let Some(i) = instruments
            .get(id)
            .filter(|i| i.kind == InstrumentKind::Bond)
        else {
            continue;
        };
        let Some(unit_cost) = h.cost.checked_div(h.quantity) else {
            continue;
        };
        quotes.entry(*id).or_insert_with(|| Quote {
            instrument_id: *id,
            price: unit_cost.round_dp(6),
            currency: i.currency.clone(),
            source: "cost".into(),
            fetched_at: chrono::Utc::now(),
            return_12m_percent: None,
            dividend_yield_percent: None,
        });
    }
    Ok(Inputs {
        portfolio,
        instruments,
        holdings,
        targets: store
            .targets_of(portfolio_id)?
            .into_iter()
            .map(|t| (t.instrument_id, t.target_percent))
            .collect(),
        quotes,
        fx: store.fx_table()?,
    })
}

impl Inputs {
    /// Instruments that are held or have a target, in a stable order.
    fn instrument_ids(&self) -> Vec<Uuid> {
        let mut ids: Vec<Uuid> = self
            .holdings
            .iter()
            .filter(|(_, h)| !h.quantity.is_zero())
            .map(|(id, _)| *id)
            .chain(self.targets.keys().copied())
            .filter(|id| self.instruments.contains_key(id))
            .collect();
        ids.sort_by_key(|id| (self.instruments[id].symbol.to_lowercase(), *id));
        ids.dedup();
        ids
    }

    fn to_base(&self, amount: Decimal, from: &crate::money::Currency) -> Option<Decimal> {
        self.fx
            .convert(
                &Money::new(amount, from.clone()),
                &self.portfolio.base_currency,
            )
            .ok()
            .map(|m| m.amount)
    }

    /// Unit price in base currency.
    fn price_in_base(&self, id: Uuid) -> Option<Decimal> {
        let q = self.quotes.get(&id)?;
        self.to_base(q.price, &q.currency)
    }
}

pub fn view(store: &Store, portfolio_id: Uuid) -> Result<PortfolioView> {
    let inp = load(store, portfolio_id)?;
    let mut positions = Vec::new();
    let mut missing_prices = Vec::new();
    let mut missing_rates = Vec::new();

    for id in inp.instrument_ids() {
        let instrument = inp.instruments[&id].clone();
        let h = inp.holdings.get(&id).cloned().unwrap_or_default();
        let quote = inp.quotes.get(&id);
        if quote.is_none() && !h.quantity.is_zero() {
            missing_prices.push(instrument.symbol.clone());
        }
        let unit = inp.price_in_base(id);
        if let Some(q) = quote
            && unit.is_none()
            && !missing_rates.contains(&q.currency.to_string())
        {
            missing_rates.push(q.currency.to_string());
        }
        let value = unit.map(|p| p * h.quantity);
        let cost_base = inp.to_base(h.cost, &instrument.currency);
        positions.push(PositionView {
            avg_cost: (!h.quantity.is_zero()).then(|| (h.cost / h.quantity).round_dp(6)),
            price: quote.map(|q| q.price),
            price_at: quote.map(|q| q.fetched_at),
            price_source: quote.map(|q| q.source.clone()),
            gain: value.zip(cost_base).map(|(v, c)| (v - c).round_dp(2)),
            value: value.map(|v| v.round_dp(2)),
            realized: h.realized.round_dp(2),
            quantity: h.quantity.normalize(),
            target_percent: inp.targets.get(&id).copied().unwrap_or_default(),
            current_percent: None,
            instrument,
        });
    }

    let total_value: Decimal = positions.iter().filter_map(|p| p.value).sum();
    let complete = missing_prices.is_empty() && missing_rates.is_empty();
    if total_value > Decimal::ZERO {
        for p in &mut positions {
            p.current_percent = p.value.map(|v| (v / total_value * HUNDRED).round_dp(2));
        }
    }
    let max_deviation = (complete && total_value > Decimal::ZERO)
        .then(|| {
            positions
                .iter()
                .filter_map(|p| p.current_percent.map(|c| (c - p.target_percent).abs()))
                .max()
        })
        .flatten();

    Ok(PortfolioView {
        needs_rebalance: max_deviation.is_some_and(|d| d > inp.portfolio.tolerance_percent),
        total_gain: positions.iter().filter_map(|p| p.gain).sum(),
        targets_sum: inp.targets.values().copied().sum(),
        portfolio: inp.portfolio,
        positions,
        total_value,
        missing_prices,
        missing_rates,
        max_deviation,
    })
}

/// Rebalance a stored portfolio. Result position ids are instrument ids.
pub fn rebalance(
    store: &Store,
    portfolio_id: Uuid,
    contribution: Decimal,
    mode: Mode,
) -> Result<RebalanceResult> {
    let inp = load(store, portfolio_id)?;
    let mut positions = Vec::new();
    for id in inp.instrument_ids() {
        let instrument = &inp.instruments[&id];
        let price = inp
            .price_in_base(id)
            .ok_or_else(|| CoreError::MissingPrice(instrument.symbol.clone()))?;
        positions.push(Position {
            id: id.to_string(),
            quantity: inp
                .holdings
                .get(&id)
                .map(|h| h.quantity)
                .unwrap_or_default(),
            price,
            target_percent: inp.targets.get(&id).copied().unwrap_or_default(),
            fractional: instrument.fractional,
        });
    }
    rebalance::compute(&RebalanceInput {
        positions,
        contribution,
        mode,
        tolerance_percent: inp.portfolio.tolerance_percent,
        fraction_decimals: 8,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::money::Currency;
    use chrono::NaiveDate;
    use rust_decimal_macros::dec;

    fn trade(
        instrument_id: Uuid,
        day: u32,
        side: TradeSide,
        quantity: Decimal,
        price: Decimal,
    ) -> Trade {
        Trade {
            meta: Meta::new(),
            portfolio_id: Uuid::nil(),
            instrument_id,
            date: NaiveDate::from_ymd_opt(2026, 1, day).unwrap(),
            side,
            quantity,
            price,
            fee: Decimal::ZERO,
        }
    }

    #[test]
    fn average_cost_and_realized_gain() {
        let a = Uuid::now_v7();
        let h = holdings(&[
            trade(a, 1, TradeSide::Buy, dec!(10), dec!(100)),
            trade(a, 2, TradeSide::Buy, dec!(10), dec!(200)),
            trade(a, 3, TradeSide::Sell, dec!(5), dec!(300)),
        ])
        .unwrap();
        let h = &h[&a];
        assert_eq!(h.quantity, dec!(15));
        assert_eq!(h.cost, dec!(2250)); // avg 150
        assert_eq!(h.realized, dec!(750)); // 5 * (300 - 150)
    }

    #[test]
    fn trades_are_replayed_in_date_order() {
        let a = Uuid::now_v7();
        // Entered out of order: the sell on day 2 is valid because the buy is on day 1.
        let sell = trade(a, 2, TradeSide::Sell, dec!(1), dec!(10));
        let buy = trade(a, 1, TradeSide::Buy, dec!(1), dec!(10));
        assert!(holdings(&[sell, buy]).is_ok());
    }

    #[test]
    fn overselling_is_rejected() {
        let a = Uuid::now_v7();
        let r = holdings(&[
            trade(a, 1, TradeSide::Buy, dec!(1), dec!(10)),
            trade(a, 2, TradeSide::Sell, dec!(2), dec!(10)),
        ]);
        assert!(matches!(r, Err(CoreError::Oversold { .. })));
    }

    struct Fixture {
        store: Store,
        portfolio: Portfolio,
        etf: Instrument,
        btc: Instrument,
    }

    fn fixture() -> Fixture {
        let store = Store::open_in_memory().unwrap();
        let pln = Currency::new("PLN").unwrap();
        let portfolio = store
            .save(Portfolio {
                meta: Meta::new(),
                name: "IKE".into(),
                base_currency: pln.clone(),
                tolerance_percent: dec!(5),
            })
            .unwrap();
        let mk = |symbol: &str, kind, currency: &str, fractional| Instrument {
            meta: Meta::new(),
            symbol: symbol.into(),
            name: symbol.into(),
            kind,
            currency: Currency::new(currency).unwrap(),
            quote_source: QuoteSource::Manual,
            fractional,
            interest_percent: None,
            capitalization: None,
            start_date: None,
            maturity_date: None,
        };
        let etf = store
            .save(mk("VWCE", InstrumentKind::Etf, "EUR", false))
            .unwrap();
        let btc = store
            .save(mk("BTC", InstrumentKind::Crypto, "PLN", true))
            .unwrap();
        store
            .set_targets(
                portfolio.meta.id,
                &[(etf.meta.id, dec!(80)), (btc.meta.id, dec!(20))],
            )
            .unwrap();
        let mut t = trade(etf.meta.id, 1, TradeSide::Buy, dec!(10), dec!(100));
        t.portfolio_id = portfolio.meta.id;
        store.save_trade(t).unwrap();
        Fixture {
            store,
            portfolio,
            etf,
            btc,
        }
    }

    fn quote(id: Uuid, price: Decimal, currency: &str) -> Quote {
        Quote {
            instrument_id: id,
            price,
            currency: Currency::new(currency).unwrap(),
            source: "manual".into(),
            fetched_at: Utc::now(),
            return_12m_percent: None,
            dividend_yield_percent: None,
        }
    }

    #[test]
    fn view_reports_missing_data_then_values() {
        let f = fixture();
        let v = view(&f.store, f.portfolio.meta.id).unwrap();
        assert_eq!(v.positions.len(), 2);
        assert_eq!(v.missing_prices, ["VWCE"]);
        assert_eq!(v.targets_sum, dec!(100));
        assert!(v.max_deviation.is_none());

        f.store
            .set_quote(&quote(f.etf.meta.id, dec!(110), "EUR"))
            .unwrap();
        let v = view(&f.store, f.portfolio.meta.id).unwrap();
        assert_eq!(v.missing_rates, ["EUR"]);

        f.store
            .set_fx_rate(&FxRate {
                currency: Currency::new("EUR").unwrap(),
                rate_pln: dec!(4),
                source: "manual".into(),
                fetched_at: Utc::now(),
            })
            .unwrap();
        f.store
            .set_quote(&quote(f.btc.meta.id, dec!(400000), "PLN"))
            .unwrap();
        let v = view(&f.store, f.portfolio.meta.id).unwrap();
        let etf = v
            .positions
            .iter()
            .find(|p| p.instrument.symbol == "VWCE")
            .unwrap();
        assert_eq!(etf.value, Some(dec!(4400))); // 10 * 110 EUR * 4
        assert_eq!(etf.gain, Some(dec!(400))); // cost 1000 EUR = 4000 PLN
        assert_eq!(v.total_value, dec!(4400));
        assert_eq!(v.max_deviation, Some(dec!(20)));
        assert!(v.needs_rebalance);
    }

    fn money_in_portfolio(
        kind: InstrumentKind,
        currency: &str,
        quantity: Decimal,
        price: Decimal,
    ) -> (Store, Portfolio) {
        let store = Store::open_in_memory().unwrap();
        let portfolio = store
            .save(Portfolio {
                meta: Meta::new(),
                name: "Gotówka".into(),
                base_currency: Currency::new("PLN").unwrap(),
                tolerance_percent: dec!(5),
            })
            .unwrap();
        let cash = store
            .save(Instrument {
                meta: Meta::new(),
                symbol: currency.into(),
                name: format!("Cash {currency}"),
                kind,
                currency: Currency::new(currency).unwrap(),
                quote_source: QuoteSource::Manual,
                fractional: true,
                interest_percent: Some(dec!(3.5)),
                capitalization: None,
                start_date: None,
                maturity_date: None,
            })
            .unwrap();
        let mut deposit = trade(cash.meta.id, 1, TradeSide::Buy, quantity, price);
        deposit.portfolio_id = portfolio.meta.id;
        store.save_trade(deposit).unwrap();
        (store, portfolio)
    }

    #[test]
    fn cash_is_worth_one_unit_of_its_currency_without_a_price_entry() {
        let (store, portfolio) =
            money_in_portfolio(InstrumentKind::Cash, "PLN", dec!(1000.50), dec!(1));
        let v = view(&store, portfolio.meta.id).unwrap();
        assert!(v.missing_prices.is_empty());
        assert!(v.missing_rates.is_empty());
        assert_eq!(v.total_value, dec!(1000.50));
        assert_eq!(v.positions[0].price, Some(dec!(1)));
    }

    #[test]
    fn a_bank_deposit_is_valued_at_its_principal() {
        let (store, portfolio) =
            money_in_portfolio(InstrumentKind::Deposit, "PLN", dec!(5000), dec!(1));
        let v = view(&store, portfolio.meta.id).unwrap();
        assert!(v.missing_prices.is_empty());
        assert_eq!(v.total_value, dec!(5000));
        // The manual rate stays on the instrument and does not change the valuation.
        assert_eq!(v.positions[0].instrument.interest_percent, Some(dec!(3.5)));
    }

    #[test]
    fn a_bond_without_a_price_is_valued_at_cost_not_zero() {
        // 30 bonds bought at 100 PLN each.
        let (store, portfolio) =
            money_in_portfolio(InstrumentKind::Bond, "PLN", dec!(30), dec!(100));
        let v = view(&store, portfolio.meta.id).unwrap();
        assert!(v.missing_prices.is_empty());
        assert_eq!(v.total_value, dec!(3000));
        assert_eq!(v.positions[0].price, Some(dec!(100)));
        assert_eq!(v.positions[0].gain, Some(dec!(0)));
    }

    #[test]
    fn an_entered_bond_price_wins_over_the_cost() {
        let (store, portfolio) =
            money_in_portfolio(InstrumentKind::Bond, "PLN", dec!(30), dec!(100));
        let bond_id = store.list::<Instrument>().unwrap()[0].meta.id;
        store
            .set_quote(&quote(bond_id, dec!(101.5), "PLN"))
            .unwrap();
        let v = view(&store, portfolio.meta.id).unwrap();
        assert_eq!(v.total_value, dec!(3045));
        assert_eq!(v.positions[0].gain, Some(dec!(45)));
    }

    #[test]
    fn a_sold_out_bond_is_not_valued() {
        let (store, portfolio) =
            money_in_portfolio(InstrumentKind::Bond, "PLN", dec!(30), dec!(100));
        let bond_id = store.list::<Instrument>().unwrap()[0].meta.id;
        let mut sell = trade(bond_id, 2, TradeSide::Sell, dec!(30), dec!(100));
        sell.portfolio_id = portfolio.meta.id;
        store.save_trade(sell).unwrap();
        let v = view(&store, portfolio.meta.id).unwrap();
        assert_eq!(v.total_value, dec!(0));
        assert!(v.missing_prices.is_empty());
    }

    #[test]
    fn foreign_cash_still_needs_an_exchange_rate() {
        let (store, portfolio) =
            money_in_portfolio(InstrumentKind::Cash, "EUR", dec!(100), dec!(1));
        let v = view(&store, portfolio.meta.id).unwrap();
        assert!(v.missing_prices.is_empty());
        assert_eq!(v.missing_rates, ["EUR"]);
        store
            .set_fx_rate(&FxRate {
                currency: Currency::new("EUR").unwrap(),
                rate_pln: dec!(4),
                source: "manual".into(),
                fetched_at: Utc::now(),
            })
            .unwrap();
        let v = view(&store, portfolio.meta.id).unwrap();
        assert_eq!(v.total_value, dec!(400));
    }

    #[test]
    fn an_entered_cash_price_wins_over_the_default() {
        let (store, portfolio) = money_in_portfolio(InstrumentKind::Cash, "PLN", dec!(10), dec!(1));
        let cash_id = store.list::<Instrument>().unwrap()[0].meta.id;
        store.set_quote(&quote(cash_id, dec!(2), "PLN")).unwrap();
        let v = view(&store, portfolio.meta.id).unwrap();
        assert_eq!(v.total_value, dec!(20));
    }

    #[test]
    fn rebalance_uses_converted_prices() {
        let f = fixture();
        assert!(matches!(
            rebalance(&f.store, f.portfolio.meta.id, dec!(1000), Mode::BuyOnly),
            Err(CoreError::MissingPrice(_))
        ));
        f.store
            .set_fx_rate(&FxRate {
                currency: Currency::new("EUR").unwrap(),
                rate_pln: dec!(4),
                source: "manual".into(),
                fetched_at: Utc::now(),
            })
            .unwrap();
        f.store
            .set_quote(&quote(f.etf.meta.id, dec!(100), "EUR"))
            .unwrap();
        f.store
            .set_quote(&quote(f.btc.meta.id, dec!(400000), "PLN"))
            .unwrap();
        // 4000 PLN in ETF, 0 in BTC, +1000 PLN: total 5000, BTC target 1000.
        let r = rebalance(&f.store, f.portfolio.meta.id, dec!(1000), Mode::BuyOnly).unwrap();
        let btc = r
            .positions
            .iter()
            .find(|p| p.id == f.btc.meta.id.to_string())
            .unwrap();
        assert_eq!(btc.units_to_trade, dec!(0.0025));
    }

    #[test]
    fn store_rejects_oversell_and_breaking_deletes() {
        let f = fixture();
        let mut sell = trade(f.etf.meta.id, 2, TradeSide::Sell, dec!(11), dec!(100));
        sell.portfolio_id = f.portfolio.meta.id;
        assert!(matches!(
            f.store.save_trade(sell.clone()),
            Err(CoreError::Oversold { .. })
        ));
        sell.quantity = dec!(10);
        f.store.save_trade(sell).unwrap();
        // Deleting the original buy would leave a sell of shares never bought.
        let buy = f
            .store
            .trades_of(f.portfolio.meta.id)
            .unwrap()
            .into_iter()
            .find(|t| t.side == TradeSide::Buy)
            .unwrap();
        assert!(matches!(
            f.store.delete_trade(buy.meta.id),
            Err(CoreError::Oversold { .. })
        ));
    }

    #[test]
    fn editing_a_trade_keeps_history_valid() {
        let f = fixture();
        let pid = f.portfolio.meta.id;
        let mut sell = trade(f.etf.meta.id, 2, TradeSide::Sell, dec!(5), dec!(100));
        sell.portfolio_id = pid;
        let sell = f.store.save_trade(sell).unwrap();
        let buy = f
            .store
            .trades_of(pid)
            .unwrap()
            .into_iter()
            .find(|t| t.side == TradeSide::Buy)
            .unwrap();

        // Same id: an update, not a new record.
        let mut edited = sell.clone();
        edited.quantity = dec!(10);
        f.store.save_trade(edited).unwrap();
        assert_eq!(f.store.trades_of(pid).unwrap().len(), 2);

        // Shrinking the buy below the sell would oversell.
        let mut shrunk = buy.clone();
        shrunk.quantity = dec!(9);
        assert!(matches!(
            f.store.save_trade(shrunk),
            Err(CoreError::Oversold { .. })
        ));
        assert_eq!(
            f.store.get::<Trade>(buy.meta.id).unwrap().unwrap().quantity,
            buy.quantity
        );
    }

    #[test]
    fn set_targets_replaces_allocation() {
        let f = fixture();
        f.store
            .set_targets(f.portfolio.meta.id, &[(f.etf.meta.id, dec!(100))])
            .unwrap();
        let targets = f.store.targets_of(f.portfolio.meta.id).unwrap();
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].target_percent, dec!(100));
    }
}
