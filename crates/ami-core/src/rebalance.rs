//! Portfolio rebalancing calculator.
//!
//! All values are expressed in one currency (the portfolio base currency);
//! conversion happens before calling [`compute`].
//!
//! Modes:
//! * [`Mode::BuyOnly`]: spend the contribution only on underweight positions
//!   ("water-filling"), never sell. Avoids realising gains (tax).
//! * [`Mode::Full`]: buy and sell to hit the targets as closely as possible.
//!   A negative contribution means a withdrawal.
//!
//! Non-fractional positions are traded in whole units; any cash left after
//! rounding is spent greedily on the most underweight positions that fit.

use rust_decimal::{Decimal, RoundingStrategy};
use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};

const HUNDRED: Decimal = Decimal::ONE_HUNDRED;
const MAX_GREEDY_STEPS: usize = 100_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Position {
    pub id: String,
    pub quantity: Decimal,
    /// Price per unit in the base currency.
    pub price: Decimal,
    /// Target weight in percent (0–100).
    pub target_percent: Decimal,
    #[serde(default)]
    pub fractional: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Mode {
    BuyOnly,
    Full,
}

fn default_decimals() -> u32 {
    8
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RebalanceInput {
    pub positions: Vec<Position>,
    #[serde(default)]
    pub contribution: Decimal,
    pub mode: Mode,
    /// Allowed drift in percentage points.
    #[serde(default)]
    pub tolerance_percent: Decimal,
    /// Decimal places for fractional positions.
    #[serde(default = "default_decimals")]
    pub fraction_decimals: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PositionResult {
    pub id: String,
    pub current_value: Decimal,
    pub current_percent: Decimal,
    pub target_percent: Decimal,
    /// current_percent - target_percent, in percentage points.
    pub deviation_before: Decimal,
    pub target_value: Decimal,
    /// Signed: positive = buy, negative = sell.
    pub units_to_trade: Decimal,
    pub trade_value: Decimal,
    pub value_after: Decimal,
    pub percent_after: Decimal,
    pub deviation_after: Decimal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RebalanceResult {
    pub total_before: Decimal,
    pub contribution: Decimal,
    pub total_after: Decimal,
    /// Cash not invested because of whole-unit rounding.
    pub cash_left: Decimal,
    pub max_deviation_before: Decimal,
    pub needs_rebalance: bool,
    pub positions: Vec<PositionResult>,
}

fn invalid(id: &str, reason: &str) -> CoreError {
    CoreError::InvalidPosition {
        id: id.to_string(),
        reason: reason.to_string(),
    }
}

fn validate(input: &RebalanceInput) -> Result<()> {
    if input.positions.is_empty() {
        return Err(CoreError::NoPositions);
    }
    for p in &input.positions {
        if p.price <= Decimal::ZERO {
            return Err(invalid(&p.id, "price must be positive"));
        }
        if p.quantity < Decimal::ZERO {
            return Err(invalid(&p.id, "quantity must not be negative"));
        }
        if p.target_percent < Decimal::ZERO || p.target_percent > HUNDRED {
            return Err(invalid(&p.id, "target must be between 0 and 100"));
        }
    }
    let sum: Decimal = input.positions.iter().map(|p| p.target_percent).sum();
    if (sum - HUNDRED).abs() > Decimal::new(1, 2) {
        return Err(CoreError::WeightsSum(sum.normalize().to_string()));
    }
    if input.mode == Mode::BuyOnly && input.contribution < Decimal::ZERO {
        return Err(CoreError::NegativeContribution);
    }
    Ok(())
}

/// Ideal (unrounded) amount to add to each position in buy-only mode.
///
/// Raises the most underweight positions to a common level `L` of
/// value/weight until the contribution is used up.
fn water_fill(values: &[Decimal], weights: &[Decimal], contribution: Decimal) -> Vec<Decimal> {
    let mut adds = vec![Decimal::ZERO; values.len()];
    if contribution <= Decimal::ZERO {
        return adds;
    }
    let mut order: Vec<usize> = (0..values.len())
        .filter(|&i| weights[i] > Decimal::ZERO)
        .collect();
    order.sort_by(|&a, &b| (values[a] / weights[a]).cmp(&(values[b] / weights[b])));

    let mut sum_v = Decimal::ZERO;
    let mut sum_w = Decimal::ZERO;
    for (k, &i) in order.iter().enumerate() {
        sum_v += values[i];
        sum_w += weights[i];
        let level = (contribution + sum_v) / sum_w;
        let next_ratio = order.get(k + 1).map(|&j| values[j] / weights[j]);
        if next_ratio.is_none_or(|r| level <= r) {
            for &j in &order[..=k] {
                adds[j] = level * weights[j] - values[j];
            }
            break;
        }
    }
    adds
}

pub fn compute(input: &RebalanceInput) -> Result<RebalanceResult> {
    validate(input)?;
    let ps = &input.positions;
    let n = ps.len();
    let values: Vec<Decimal> = ps.iter().map(|p| p.quantity * p.price).collect();
    let weights: Vec<Decimal> = ps.iter().map(|p| p.target_percent / HUNDRED).collect();
    let total_before: Decimal = values.iter().sum();
    let total_target = total_before + input.contribution;
    if total_target <= Decimal::ZERO {
        return Err(CoreError::EmptyPortfolio);
    }
    let targets: Vec<Decimal> = weights.iter().map(|w| total_target * w).collect();

    let ideal: Vec<Decimal> = match input.mode {
        Mode::Full => (0..n).map(|i| targets[i] - values[i]).collect(),
        Mode::BuyOnly => water_fill(&values, &weights, input.contribution),
    };

    let step = |p: &Position| {
        if p.fractional {
            Decimal::new(1, input.fraction_decimals)
        } else {
            Decimal::ONE
        }
    };
    let round_units = |p: &Position, units: Decimal| {
        let dp = if p.fractional {
            input.fraction_decimals
        } else {
            0
        };
        units.round_dp_with_strategy(dp, RoundingStrategy::ToZero)
    };

    // 1. Round ideal trades toward zero.
    let mut units: Vec<Decimal> = (0..n)
        .map(|i| round_units(&ps[i], ideal[i] / ps[i].price))
        .collect();
    let spent = |units: &[Decimal]| -> Decimal { (0..n).map(|i| units[i] * ps[i].price).sum() };
    let mut cash = input.contribution - spent(&units);
    let after = |units: &[Decimal], i: usize| (ps[i].quantity + units[i]) * ps[i].price;

    // 2. Rounded sells may leave us short of cash: cut buys / sell a bit more
    //    on the most overweight position until cash >= 0.
    let mut guard = 0;
    while cash < Decimal::ZERO && guard < MAX_GREEDY_STEPS {
        guard += 1;
        let candidate = (0..n)
            .filter(|&i| {
                let s = step(&ps[i]);
                let can_reduce_buy = units[i] > Decimal::ZERO;
                let can_sell_more = input.mode == Mode::Full && ps[i].quantity + units[i] >= s;
                can_reduce_buy || can_sell_more
            })
            .max_by(|&a, &b| (after(&units, a) - targets[a]).cmp(&(after(&units, b) - targets[b])));
        let Some(i) = candidate else { break };
        let p = &ps[i];
        let delta = if p.fractional {
            // Exactly the shortfall, rounded up to the allowed precision.
            let needed = (-cash / p.price)
                .round_dp_with_strategy(input.fraction_decimals, RoundingStrategy::AwayFromZero);
            let limit = if units[i] > Decimal::ZERO {
                units[i]
            } else {
                p.quantity + units[i]
            };
            needed.min(limit).max(step(p))
        } else {
            Decimal::ONE
        };
        units[i] -= delta;
        cash += delta * p.price;
    }

    // 3. Spend leftover cash on whole units of the most underweight positions.
    let mut guard = 0;
    while guard < MAX_GREEDY_STEPS {
        guard += 1;
        let candidate = (0..n)
            .filter(|&i| {
                weights[i] > Decimal::ZERO
                    && ps[i].price <= cash
                    && targets[i] - after(&units, i) > Decimal::ZERO
            })
            .max_by(|&a, &b| (targets[a] - after(&units, a)).cmp(&(targets[b] - after(&units, b))));
        let Some(i) = candidate else { break };
        let p = &ps[i];
        let delta = if p.fractional {
            let room = (targets[i] - after(&units, i)).min(cash);
            let d = (room / p.price)
                .round_dp_with_strategy(input.fraction_decimals, RoundingStrategy::ToZero);
            if d <= Decimal::ZERO {
                break;
            }
            d
        } else {
            Decimal::ONE
        };
        units[i] += delta;
        cash -= delta * p.price;
    }

    let values_after: Vec<Decimal> = (0..n).map(|i| after(&units, i)).collect();
    let total_after: Decimal = values_after.iter().sum();
    let pct = |v: Decimal, total: Decimal| {
        if total > Decimal::ZERO {
            (v / total * HUNDRED).round_dp(4)
        } else {
            Decimal::ZERO
        }
    };

    let positions: Vec<PositionResult> = (0..n)
        .map(|i| {
            let current_percent = pct(values[i], total_before);
            let percent_after = pct(values_after[i], total_after);
            PositionResult {
                id: ps[i].id.clone(),
                current_value: values[i],
                current_percent,
                target_percent: ps[i].target_percent,
                deviation_before: current_percent - ps[i].target_percent,
                target_value: targets[i].round_dp(2),
                units_to_trade: units[i].normalize(),
                trade_value: units[i] * ps[i].price,
                value_after: values_after[i],
                percent_after,
                deviation_after: percent_after - ps[i].target_percent,
            }
        })
        .collect();

    let max_deviation_before = positions
        .iter()
        .map(|p| p.deviation_before.abs())
        .max()
        .unwrap_or_default();

    Ok(RebalanceResult {
        total_before,
        contribution: input.contribution,
        total_after,
        cash_left: cash,
        needs_rebalance: total_before > Decimal::ZERO
            && max_deviation_before > input.tolerance_percent,
        max_deviation_before,
        positions,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn pos(id: &str, quantity: Decimal, price: Decimal, target: Decimal) -> Position {
        Position {
            id: id.into(),
            quantity,
            price,
            target_percent: target,
            fractional: false,
        }
    }

    fn input(positions: Vec<Position>, contribution: Decimal, mode: Mode) -> RebalanceInput {
        RebalanceInput {
            positions,
            contribution,
            mode,
            tolerance_percent: dec!(5),
            fraction_decimals: 8,
        }
    }

    fn units(r: &RebalanceResult) -> Vec<Decimal> {
        r.positions.iter().map(|p| p.units_to_trade).collect()
    }

    #[test]
    fn full_rebalance_sells_and_buys() {
        let r = compute(&input(
            vec![
                pos("A", dec!(10), dec!(100), dec!(50)),
                pos("B", dec!(0), dec!(50), dec!(50)),
            ],
            dec!(0),
            Mode::Full,
        ))
        .unwrap();
        assert_eq!(units(&r), vec![dec!(-5), dec!(10)]);
        assert_eq!(r.cash_left, dec!(0));
        assert!(r.needs_rebalance);
    }

    #[test]
    fn buy_only_fills_underweight_first() {
        let r = compute(&input(
            vec![
                pos("A", dec!(10), dec!(100), dec!(50)),
                pos("B", dec!(0), dec!(100), dec!(50)),
            ],
            dec!(500),
            Mode::BuyOnly,
        ))
        .unwrap();
        assert_eq!(units(&r), vec![dec!(0), dec!(5)]);
    }

    #[test]
    fn buy_only_spreads_surplus_by_weight() {
        let r = compute(&input(
            vec![
                pos("A", dec!(10), dec!(100), dec!(50)),
                pos("B", dec!(0), dec!(100), dec!(50)),
            ],
            dec!(3000),
            Mode::BuyOnly,
        ))
        .unwrap();
        assert_eq!(units(&r), vec![dec!(10), dec!(20)]);
        assert_eq!(r.cash_left, dec!(0));
    }

    #[test]
    fn leftover_cash_is_spent_greedily() {
        let r = compute(&input(
            vec![
                pos("A", dec!(0), dec!(30), dec!(50)),
                pos("B", dec!(0), dec!(70), dec!(50)),
            ],
            dec!(100),
            Mode::Full,
        ))
        .unwrap();
        assert_eq!(units(&r), vec![dec!(1), dec!(1)]);
        assert_eq!(r.cash_left, dec!(0));
    }

    #[test]
    fn rounding_never_leaves_negative_cash() {
        let r = compute(&input(
            vec![
                pos("A", dec!(3), dec!(100), dec!(50)),
                pos("B", dec!(0), dec!(70), dec!(50)),
            ],
            dec!(0),
            Mode::Full,
        ))
        .unwrap();
        assert!(r.cash_left >= dec!(0), "cash {}", r.cash_left);
        assert_eq!(units(&r), vec![dec!(-2), dec!(2)]);
        assert_eq!(r.cash_left, dec!(60));
    }

    #[test]
    fn fractional_crypto() {
        let mut btc = pos("BTC", dec!(0), dec!(400000), dec!(100));
        btc.fractional = true;
        let r = compute(&input(vec![btc], dec!(1000), Mode::BuyOnly)).unwrap();
        assert_eq!(units(&r), vec![dec!(0.0025)]);
        assert_eq!(r.cash_left, dec!(0));
    }

    #[test]
    fn withdrawal_in_full_mode() {
        let r = compute(&input(
            vec![
                pos("A", dec!(10), dec!(100), dec!(50)),
                pos("B", dec!(10), dec!(100), dec!(50)),
            ],
            dec!(-400),
            Mode::Full,
        ))
        .unwrap();
        assert_eq!(units(&r), vec![dec!(-2), dec!(-2)]);
        assert_eq!(r.cash_left, dec!(0));
    }

    #[test]
    fn within_tolerance_needs_no_rebalance() {
        let r = compute(&input(
            vec![
                pos("A", dec!(52), dec!(1), dec!(50)),
                pos("B", dec!(48), dec!(1), dec!(50)),
            ],
            dec!(0),
            Mode::Full,
        ))
        .unwrap();
        assert!(!r.needs_rebalance);
    }

    #[test]
    fn validation_errors() {
        let bad_sum = input(
            vec![pos("A", dec!(1), dec!(1), dec!(60))],
            dec!(0),
            Mode::Full,
        );
        assert!(matches!(compute(&bad_sum), Err(CoreError::WeightsSum(_))));
        let bad_price = input(
            vec![pos("A", dec!(1), dec!(0), dec!(100))],
            dec!(0),
            Mode::Full,
        );
        assert!(matches!(
            compute(&bad_price),
            Err(CoreError::InvalidPosition { .. })
        ));
        let neg = input(
            vec![pos("A", dec!(1), dec!(1), dec!(100))],
            dec!(-1),
            Mode::BuyOnly,
        );
        assert!(matches!(
            compute(&neg),
            Err(CoreError::NegativeContribution)
        ));
        assert!(matches!(
            compute(&input(vec![], dec!(0), Mode::Full)),
            Err(CoreError::NoPositions)
        ));
    }
}
