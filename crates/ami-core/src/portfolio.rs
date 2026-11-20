//! Portfolio state derived from trades, prices and exchange rates.

use std::collections::{BTreeMap, HashMap};

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{CoreError, Result};
use crate::fx::FxTable;
use crate::model::*;
use crate::money::{MAX_AMOUNT, Money};
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
        // A zero-quantity trade changes nothing (and a sell of 0 would divide the cost by a zero quantity).
        if t.quantity.is_zero() {
            continue;
        }
        // Saved before the limit existed? A product of absurd values would panic, so refuse it here.
        if t.quantity.max(t.price).max(t.fee) > MAX_AMOUNT {
            return Err(CoreError::Validation("trade value is too large".into()));
        }
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

/// Why a portfolio could not be valued (its trade history is broken), in the frontend's error shape.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Problem {
    pub code: String,
    pub message: String,
}

/// Share of one asset kind (ETF, bonds, cash...) in the portfolio value.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KindShare {
    pub kind: InstrumentKind,
    /// Value in the portfolio base currency.
    pub value: Decimal,
    /// Percent of `total_value`.
    pub percent: Decimal,
}

/// Fixed order, so a kind keeps its place (and its chart color) whatever the amounts are.
const KIND_ORDER: [InstrumentKind; 7] = [
    InstrumentKind::Etf,
    InstrumentKind::Stock,
    InstrumentKind::Bond,
    InstrumentKind::Crypto,
    InstrumentKind::Cash,
    InstrumentKind::Deposit,
    InstrumentKind::Other,
];

/// Value per asset kind for the allocation chart: only valued positions count, kinds without value
/// are left out.
fn kind_shares(positions: &[PositionView], total: Decimal) -> Vec<KindShare> {
    if total <= Decimal::ZERO {
        return Vec::new();
    }
    KIND_ORDER
        .iter()
        .filter_map(|kind| {
            let value: Decimal = positions
                .iter()
                .filter(|p| p.instrument.kind == *kind)
                .filter_map(|p| p.value)
                .sum();
            (value > Decimal::ZERO).then(|| KindShare {
                kind: *kind,
                value: value.round_dp(2),
                percent: (value / total * HUNDRED).round_dp(2),
            })
        })
        .collect()
}

/// Mean of the manual interest rates weighted by position value, and the total weight.
/// Positions without a rate or without a value are left out.
fn weighted_interest(positions: &[PositionView]) -> (Option<Decimal>, Decimal) {
    let (mut sum, mut weight) = (Decimal::ZERO, Decimal::ZERO);
    for p in positions {
        if let (Some(rate), Some(v)) = (p.instrument.interest_percent, p.value) {
            sum += rate * v;
            weight += v;
        }
    }
    (
        sum.checked_div(weight).map(|a| a.round_dp(2)),
        weight.round_dp(2),
    )
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
    /// Split of the value by asset kind, for the pie chart.
    pub kind_shares: Vec<KindShare>,
    /// Value-weighted mean of the hand-entered interest rates (bonds, cash, deposits), percent.
    pub avg_interest_percent: Option<Decimal>,
    /// Value of the positions that have such a rate: the weight behind `avg_interest_percent`.
    pub interest_value: Decimal,
    /// Symbols valued at their purchase cost because no price was entered (bonds): an estimate.
    pub estimated_prices: Vec<String>,
    /// Set when the trade history is broken (e.g. a sale above the quantity held, after merging two
    /// backups). The view is then empty, but the trades stay listed so the bad one can be removed.
    pub problem: Option<Problem>,
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
    // The cost basis includes buy fees, so the unit price is slightly above the entered purchase price
    // and the gain shows 0 rather than minus the fees.
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
    match load(store, portfolio_id) {
        Ok(inp) => Ok(build_view(inp)),
        // One broken history must not take the dashboard and the portfolio list down with it.
        Err(e @ (CoreError::Oversold { .. } | CoreError::Validation(_))) => {
            let portfolio = store
                .get::<Portfolio>(portfolio_id)?
                .ok_or(CoreError::NotFound)?;
            Ok(broken_view(portfolio, &e))
        }
        Err(e) => Err(e),
    }
}

fn broken_view(portfolio: Portfolio, e: &CoreError) -> PortfolioView {
    PortfolioView {
        portfolio,
        positions: Vec::new(),
        total_value: Decimal::ZERO,
        total_gain: Decimal::ZERO,
        targets_sum: Decimal::ZERO,
        missing_prices: Vec::new(),
        missing_rates: Vec::new(),
        max_deviation: None,
        needs_rebalance: false,
        kind_shares: Vec::new(),
        avg_interest_percent: None,
        interest_value: Decimal::ZERO,
        estimated_prices: Vec::new(),
        problem: Some(Problem {
            code: e.code().into(),
            message: e.to_string(),
        }),
    }
}

fn build_view(inp: Inputs) -> PortfolioView {
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

    let kind_shares = kind_shares(&positions, total_value);
    let (avg_interest_percent, interest_value) = weighted_interest(&positions);
    let estimated_prices = positions
        .iter()
        .filter(|p| p.price_source.as_deref() == Some("cost"))
        .map(|p| p.instrument.symbol.clone())
        .collect();
    PortfolioView {
        kind_shares,
        avg_interest_percent,
        interest_value,
        estimated_prices,
        needs_rebalance: max_deviation.is_some_and(|d| d > inp.portfolio.tolerance_percent),
        total_gain: positions.iter().filter_map(|p| p.gain).sum(),
        targets_sum: inp.targets.values().copied().sum(),
        portfolio: inp.portfolio,
        positions,
        total_value,
        missing_prices,
        missing_rates,
        max_deviation,
        problem: None,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryPoint {
    pub day: chrono::NaiveDate,
    pub value: Decimal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct History {
    /// Portfolio value, one point per day the app computed it (starts when recording started).
    pub values: Vec<HistoryPoint>,
    /// Money put in so far (buys plus fees minus sales), from the trade history, per trade date.
    /// Converted at today's rates; empty when a rate is missing.
    pub invested: Vec<HistoryPoint>,
}

/// Stores today's value of a fully valued portfolio (called whenever a view is computed).
pub fn record_today(store: &Store, v: &PortfolioView) -> Result<()> {
    let complete = v.problem.is_none()
        && v.missing_prices.is_empty()
        && v.missing_rates.is_empty()
        && v.total_value > Decimal::ZERO;
    if complete {
        let day = chrono::Local::now().date_naive();
        store.record_value(
            v.portfolio.meta.id,
            day,
            &v.portfolio.base_currency,
            v.total_value,
        )?;
    }
    Ok(())
}

pub fn history(store: &Store, portfolio_id: Uuid) -> Result<History> {
    let p = store
        .get::<Portfolio>(portfolio_id)?
        .ok_or(CoreError::NotFound)?;
    let values = store
        .value_history(portfolio_id, &p.base_currency)?
        .into_iter()
        .map(|(day, value)| HistoryPoint { day, value })
        .collect();

    let currencies: HashMap<Uuid, crate::money::Currency> = store
        .list::<Instrument>()?
        .into_iter()
        .map(|i| (i.meta.id, i.currency))
        .collect();
    let fx = store.fx_table()?;
    let mut trades = store.trades_of(portfolio_id)?;
    trades.sort_by_key(|t| t.date);
    let mut invested: Vec<HistoryPoint> = Vec::new();
    let mut total = Decimal::ZERO;
    for t in &trades {
        let flow = match t.side {
            TradeSide::Buy => t.quantity * t.price + t.fee,
            TradeSide::Sell => -(t.quantity * t.price - t.fee),
        };
        let converted = currencies.get(&t.instrument_id).and_then(|c| {
            fx.convert(&Money::new(flow, c.clone()), &p.base_currency)
                .ok()
        });
        let Some(m) = converted else {
            invested.clear();
            break;
        };
        total += m.amount;
        match invested.last_mut() {
            Some(last) if last.day == t.date => last.value = total.round_dp(2),
            _ => invested.push(HistoryPoint {
                day: t.date,
                value: total.round_dp(2),
            }),
        }
    }
    Ok(History { values, invested })
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
    let mut result = rebalance::compute(&RebalanceInput {
        positions,
        contribution,
        mode,
        tolerance_percent: inp.portfolio.tolerance_percent,
        fraction_decimals: 8,
    })?;
    result.tax = Some(sales_tax(&inp, &result));
    Ok(result)
}

/// Polish capital gains tax rate on an ordinary account.
const BELKA_PERCENT: Decimal = Decimal::from_parts(19, 0, 0, false, 0);

/// Tax on the sales of a rebalance: gain = sold units x (price - average cost), losses netted against
/// gains, 19% of a positive total on an ordinary account, nothing on IKE/IKZE.
/// shortcut: average cost instead of the FIFO the tax office uses, buy fees in the cost but no sell
/// fees, no losses carried over from earlier trades, and the cost of a foreign-currency position is
/// converted at today's rate instead of the rate of the purchase day (currency gains and losses are
/// ignored); upgrade if the estimate must match the PIT-38.
fn sales_tax(inp: &Inputs, result: &RebalanceResult) -> rebalance::TaxEstimate {
    let mut gain = Decimal::ZERO;
    for p in result
        .positions
        .iter()
        .filter(|p| p.units_to_trade < Decimal::ZERO)
    {
        let Some((id, h)) = Uuid::parse_str(&p.id)
            .ok()
            .and_then(|id| inp.holdings.get(&id).map(|h| (id, h)))
        else {
            continue;
        };
        let (Some(price), Some(cost)) = (
            inp.price_in_base(id),
            inp.instruments
                .get(&id)
                .and_then(|i| inp.to_base(h.cost, &i.currency)),
        ) else {
            continue;
        };
        let Some(avg) = cost.checked_div(h.quantity) else {
            continue;
        };
        gain += -p.units_to_trade * (price - avg);
    }
    let rate = match inp.portfolio.account_type {
        AccountType::Regular => BELKA_PERCENT,
        AccountType::Ike | AccountType::Ikze => Decimal::ZERO,
    };
    rebalance::TaxEstimate {
        realized_gain: gain.round_dp(2),
        rate_percent: rate,
        tax: (gain.max(Decimal::ZERO) * rate / HUNDRED).round_dp(2),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::money::Currency;
    use crate::money::MAX_AMOUNT;
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
    fn a_zero_quantity_trade_is_a_no_op_not_a_division_by_zero() {
        let a = Uuid::now_v7();
        // Selling 0 of something never held used to pass the oversell check and divide by zero.
        let h = holdings(&[trade(a, 1, TradeSide::Sell, dec!(0), dec!(10))]).unwrap();
        assert!(
            h.get(&a)
                .is_none_or(|h| h.quantity.is_zero() && h.cost.is_zero())
        );
        // The same on a real holding changes nothing.
        let h = holdings(&[
            trade(a, 1, TradeSide::Buy, dec!(10), dec!(100)),
            trade(a, 2, TradeSide::Sell, dec!(0), dec!(150)),
            trade(a, 3, TradeSide::Buy, dec!(0), dec!(150)),
        ])
        .unwrap();
        assert_eq!(
            (h[&a].quantity, h[&a].cost, h[&a].realized),
            (dec!(10), dec!(1000), dec!(0))
        );
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
                account_type: AccountType::default(),
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
                account_type: AccountType::default(),
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

    fn plain_instrument(kind: InstrumentKind, symbol: &str, currency: &str) -> Instrument {
        Instrument {
            meta: Meta::new(),
            symbol: symbol.into(),
            name: symbol.into(),
            kind,
            currency: Currency::new(currency).unwrap(),
            quote_source: QuoteSource::Manual,
            fractional: true,
            interest_percent: None,
            capitalization: None,
            start_date: None,
            maturity_date: None,
        }
    }

    #[test]
    fn value_is_split_by_asset_kind_in_a_fixed_order() {
        let f = fixture(); // 10 VWCE (ETF, EUR) and a BTC target without units
        f.store
            .set_quote(&quote(f.etf.meta.id, dec!(110), "EUR"))
            .unwrap();
        f.store
            .set_fx_rate(&FxRate {
                currency: Currency::new("EUR").unwrap(),
                rate_pln: dec!(4),
                source: "manual".into(),
                fetched_at: Utc::now(),
            })
            .unwrap();
        // Saved cash first, bond second: the result must still come out ETF, bond, cash.
        for (kind, symbol, qty, price) in [
            (InstrumentKind::Cash, "PLN", dec!(2200), dec!(1)),
            (InstrumentKind::Bond, "EDO1136", dec!(10), dec!(100)),
        ] {
            let i = f.store.save(plain_instrument(kind, symbol, "PLN")).unwrap();
            let mut t = trade(i.meta.id, 5, TradeSide::Buy, qty, price);
            t.portfolio_id = f.portfolio.meta.id;
            f.store.save_trade(t).unwrap();
        }
        let v = view(&f.store, f.portfolio.meta.id).unwrap();
        let shares: Vec<_> = v
            .kind_shares
            .iter()
            .map(|s| (s.kind, s.value, s.percent))
            .collect();
        // ETF 4400 + bond 1000 (at cost) + cash 2200 = 7600; the empty BTC position is left out.
        assert_eq!(
            shares,
            [
                (InstrumentKind::Etf, dec!(4400), dec!(57.89)),
                (InstrumentKind::Bond, dec!(1000), dec!(13.16)),
                (InstrumentKind::Cash, dec!(2200), dec!(28.95)),
            ]
        );
        assert_eq!(v.total_value, dec!(7600));
    }

    #[test]
    fn nothing_valued_gives_no_shares() {
        let f = fixture(); // the ETF has no price yet, so nothing has a value
        let v = view(&f.store, f.portfolio.meta.id).unwrap();
        assert!(v.kind_shares.is_empty());
        let json = serde_json::to_value(&v).unwrap();
        assert_eq!(json["kindShares"], serde_json::json!([]));
    }

    #[test]
    fn a_share_serializes_as_a_whole_object() {
        let share = KindShare {
            kind: InstrumentKind::Deposit,
            value: dec!(5000),
            percent: dec!(12.5),
        };
        // Decimals travel as strings, the kind as camelCase: the frontend reads exactly this.
        assert_eq!(
            serde_json::to_value(&share).unwrap(),
            serde_json::json!({"kind": "deposit", "value": "5000", "percent": "12.5"})
        );
    }

    fn bond_in_portfolio(
        currency: &str,
        quantity: Decimal,
        price: Decimal,
        fee: Decimal,
    ) -> (Store, Portfolio, Instrument) {
        let store = Store::open_in_memory().unwrap();
        let portfolio = store
            .save(Portfolio {
                meta: Meta::new(),
                name: "Obligacje".into(),
                base_currency: Currency::new("PLN").unwrap(),
                tolerance_percent: dec!(5),
                account_type: AccountType::default(),
            })
            .unwrap();
        let bond = store
            .save(plain_instrument(InstrumentKind::Bond, "EDO1136", currency))
            .unwrap();
        let mut buy = trade(bond.meta.id, 1, TradeSide::Buy, quantity, price);
        buy.portfolio_id = portfolio.meta.id;
        buy.fee = fee;
        store.save_trade(buy).unwrap();
        (store, portfolio, bond)
    }

    #[test]
    fn a_bond_bought_with_a_fee_is_valued_at_cost_including_the_fee() {
        let (store, portfolio, _) = bond_in_portfolio("PLN", dec!(10), dec!(100), dec!(10));
        let v = view(&store, portfolio.meta.id).unwrap();
        assert_eq!(v.positions[0].price, Some(dec!(101))); // (10 x 100 + 10) / 10
        assert_eq!(v.total_value, dec!(1010));
        assert_eq!(v.positions[0].gain, Some(dec!(0)));
        // It is an estimate and says so.
        assert_eq!(v.estimated_prices, ["EDO1136"]);
    }

    #[test]
    fn an_entered_price_removes_the_estimate_flag() {
        let (store, portfolio, bond) = bond_in_portfolio("PLN", dec!(10), dec!(100), dec!(0));
        assert_eq!(
            view(&store, portfolio.meta.id).unwrap().estimated_prices,
            ["EDO1136"]
        );
        store
            .set_quote(&quote(bond.meta.id, dec!(102), "PLN"))
            .unwrap();
        assert!(
            view(&store, portfolio.meta.id)
                .unwrap()
                .estimated_prices
                .is_empty()
        );
    }

    #[test]
    fn a_foreign_currency_bond_needs_an_exchange_rate() {
        let (store, portfolio, _) = bond_in_portfolio("EUR", dec!(10), dec!(100), dec!(0));
        let v = view(&store, portfolio.meta.id).unwrap();
        assert_eq!(v.missing_rates, ["EUR"]);
        assert_eq!(v.total_value, dec!(0));
        assert!(v.kind_shares.is_empty());
        store
            .set_fx_rate(&FxRate {
                currency: Currency::new("EUR").unwrap(),
                rate_pln: dec!(4),
                source: "manual".into(),
                fetched_at: Utc::now(),
            })
            .unwrap();
        let v = view(&store, portfolio.meta.id).unwrap();
        assert_eq!(v.total_value, dec!(4000)); // 10 x 100 EUR x 4
        assert_eq!(v.kind_shares.len(), 1);
        assert_eq!(
            (v.kind_shares[0].kind, v.kind_shares[0].percent),
            (InstrumentKind::Bond, dec!(100))
        );
    }

    #[test]
    fn a_partly_sold_bond_keeps_its_average_cost() {
        let (store, portfolio, bond) = bond_in_portfolio("PLN", dec!(30), dec!(100), dec!(0));
        let mut sell = trade(bond.meta.id, 2, TradeSide::Sell, dec!(10), dec!(120));
        sell.portfolio_id = portfolio.meta.id;
        store.save_trade(sell).unwrap();
        let v = view(&store, portfolio.meta.id).unwrap();
        assert_eq!(v.positions[0].quantity, dec!(20));
        assert_eq!(v.positions[0].price, Some(dec!(100)));
        assert_eq!(v.total_value, dec!(2000));
        assert_eq!(v.positions[0].realized, dec!(200)); // 10 x (120 - 100)
    }

    #[test]
    fn a_held_but_unpriced_position_is_left_out_of_the_split() {
        let f = fixture(); // 10 VWCE (ETF, EUR); BTC is only a target
        f.store
            .set_quote(&quote(f.etf.meta.id, dec!(110), "EUR"))
            .unwrap();
        f.store
            .set_fx_rate(&FxRate {
                currency: Currency::new("EUR").unwrap(),
                rate_pln: dec!(4),
                source: "manual".into(),
                fetched_at: Utc::now(),
            })
            .unwrap();
        let mut held = trade(f.btc.meta.id, 5, TradeSide::Buy, dec!(1), dec!(250000));
        held.portfolio_id = f.portfolio.meta.id;
        f.store.save_trade(held).unwrap(); // BTC is now held but has no price
        let v = view(&f.store, f.portfolio.meta.id).unwrap();
        assert_eq!(v.missing_prices, ["BTC"]);
        let kinds: Vec<_> = v.kind_shares.iter().map(|s| (s.kind, s.percent)).collect();
        // The percentages are of the valued part only.
        assert_eq!(kinds, [(InstrumentKind::Etf, dec!(100))]);
    }

    #[test]
    fn three_equal_kinds_show_the_rounded_shares_not_a_forced_hundred() {
        let store = Store::open_in_memory().unwrap();
        let portfolio = store
            .save(Portfolio {
                meta: Meta::new(),
                name: "Trzy".into(),
                base_currency: Currency::new("PLN").unwrap(),
                tolerance_percent: dec!(5),
                account_type: AccountType::default(),
            })
            .unwrap();
        for (kind, symbol) in [
            (InstrumentKind::Cash, "PLN"),
            (InstrumentKind::Deposit, "LOK"),
            (InstrumentKind::Bond, "EDO"),
        ] {
            let i = store.save(plain_instrument(kind, symbol, "PLN")).unwrap();
            let mut t = trade(i.meta.id, 1, TradeSide::Buy, dec!(1000), dec!(1));
            t.portfolio_id = portfolio.meta.id;
            store.save_trade(t).unwrap();
        }
        let v = view(&store, portfolio.meta.id).unwrap();
        let shares: Vec<_> = v.kind_shares.iter().map(|s| s.percent).collect();
        assert_eq!(shares, [dec!(33.33), dec!(33.33), dec!(33.33)]); // 99.99 in total, by design
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
    fn sales_are_taxed_on_an_ordinary_account_only() {
        let f = fixture();
        let id = f.portfolio.meta.id;
        f.store
            .set_fx_rate(&FxRate {
                currency: Currency::new("EUR").unwrap(),
                rate_pln: dec!(4),
                source: "manual".into(),
                fetched_at: Utc::now(),
            })
            .unwrap();
        // ETF bought at 100 EUR, now 150 EUR: 10 units cost 4000 PLN, worth 6000 PLN.
        f.store
            .set_quote(&quote(f.etf.meta.id, dec!(150), "EUR"))
            .unwrap();
        f.store
            .set_quote(&quote(f.btc.meta.id, dec!(400000), "PLN"))
            .unwrap();
        f.store
            .set_targets(id, &[(f.etf.meta.id, dec!(50)), (f.btc.meta.id, dec!(50))])
            .unwrap();
        // Selling 5 units (3000 PLN) realizes 5 x (600 - 400) = 1000 PLN: 19% is 190.
        let t = rebalance(&f.store, id, dec!(0), Mode::Full)
            .unwrap()
            .tax
            .unwrap();
        assert_eq!(
            (t.realized_gain, t.rate_percent, t.tax),
            (dec!(1000), dec!(19), dec!(190))
        );

        let mut ike = f.portfolio.clone();
        ike.account_type = AccountType::Ike;
        f.store.save(ike).unwrap();
        let t = rebalance(&f.store, id, dec!(0), Mode::Full)
            .unwrap()
            .tax
            .unwrap();
        assert_eq!((t.realized_gain, t.tax), (dec!(1000), dec!(0)));
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

    #[test]
    fn set_targets_is_all_or_nothing() {
        let f = fixture();
        let id = f.portfolio.meta.id;
        f.store
            .set_targets(id, &[(f.etf.meta.id, dec!(60)), (f.btc.meta.id, dec!(40))])
            .unwrap();
        // The second entry is invalid: nothing may change, not even the first.
        let bad = f.store.set_targets(
            id,
            &[(f.etf.meta.id, dec!(100)), (f.btc.meta.id, dec!(150))],
        );
        assert!(matches!(bad, Err(CoreError::Validation(_))));
        let mut after: Vec<_> = f
            .store
            .targets_of(id)
            .unwrap()
            .into_iter()
            .map(|t| t.target_percent)
            .collect();
        after.sort();
        assert_eq!(after, [dec!(40), dec!(60)]);
        // The same instrument twice is refused.
        let dup = f
            .store
            .set_targets(id, &[(f.etf.meta.id, dec!(50)), (f.etf.meta.id, dec!(50))]);
        assert!(matches!(dup, Err(CoreError::Validation(_))));
    }

    #[test]
    fn import_rejects_records_a_normal_save_would_refuse() {
        let f = fixture();
        let mut data = f.store.export_data().unwrap();
        data.trades.push(Trade {
            meta: Meta::new(),
            portfolio_id: f.portfolio.meta.id,
            instrument_id: f.etf.meta.id,
            date: chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
            side: TradeSide::Buy,
            quantity: MAX_AMOUNT + dec!(1),
            price: dec!(1),
            fee: dec!(0),
        });
        let mut target = Store::open_in_memory().unwrap();
        let err = target.import_data(&data, crate::store::ImportMode::Replace);
        assert!(matches!(err, Err(CoreError::Validation(_))));
        assert!(
            target.list::<Portfolio>().unwrap().is_empty(),
            "nothing imported"
        );
    }

    #[test]
    fn holdings_refuses_a_stored_trade_with_absurd_values() {
        let f = fixture();
        let mut t = Trade {
            meta: Meta::new(),
            portfolio_id: f.portfolio.meta.id,
            instrument_id: f.etf.meta.id,
            date: chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
            side: TradeSide::Buy,
            quantity: MAX_AMOUNT * dec!(1000),
            price: MAX_AMOUNT * dec!(1000),
            fee: dec!(0),
        };
        assert!(matches!(
            holdings(&[t.clone()]),
            Err(CoreError::Validation(_))
        ));
        t.quantity = dec!(1);
        t.price = dec!(1);
        assert!(holdings(&[t]).is_ok());
    }

    #[test]
    fn a_broken_history_gives_an_empty_view_with_a_problem_not_an_error() {
        let f = fixture();
        // A sale without a purchase can only arrive through an import or merge, never through save_trade.
        let mut data = f.store.export_data().unwrap();
        data.trades.push(Trade {
            meta: Meta::new(),
            portfolio_id: f.portfolio.meta.id,
            instrument_id: f.etf.meta.id,
            date: chrono::NaiveDate::from_ymd_opt(2000, 1, 1).unwrap(),
            side: TradeSide::Sell,
            quantity: dec!(5),
            price: dec!(100),
            fee: dec!(0),
        });
        let mut store = f.store;
        store
            .import_data(&data, crate::store::ImportMode::Merge)
            .unwrap();

        let v = view(&store, f.portfolio.meta.id).unwrap();
        assert!(v.positions.is_empty());
        assert_eq!(v.problem.as_ref().unwrap().code, "oversold");
        // The dashboard keeps working and flags the portfolio.
        let dash = crate::dashboard::build(&store, &Currency::new("PLN").unwrap()).unwrap();
        assert!(dash.portfolios[0].has_problem);
        // An unknown portfolio is still an error.
        assert!(matches!(
            view(&store, Uuid::now_v7()),
            Err(CoreError::NotFound)
        ));
    }
}
