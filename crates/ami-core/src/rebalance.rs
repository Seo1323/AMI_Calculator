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
use crate::money::MAX_AMOUNT;

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

/// Estimated tax on the sales in a result (filled in for stored portfolios only).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaxEstimate {
    /// Gain (or loss) realized by the sales, in the base currency, losses netted against gains.
    pub realized_gain: Decimal,
    pub rate_percent: Decimal,
    pub tax: Decimal,
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
    #[serde(default)]
    pub tax: Option<TaxEstimate>,
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
        if p.price > MAX_AMOUNT || p.quantity > MAX_AMOUNT {
            return Err(invalid(&p.id, "value is too large"));
        }
        if p.target_percent < Decimal::ZERO || p.target_percent > HUNDRED {
            return Err(invalid(&p.id, "target must be between 0 and 100"));
        }
    }
    let sum: Decimal = input.positions.iter().map(|p| p.target_percent).sum();
    if (sum - HUNDRED).abs() > Decimal::new(1, 2) {
        return Err(CoreError::WeightsSum(sum.normalize().to_string()));
    }
    if input.contribution.abs() > MAX_AMOUNT {
        return Err(CoreError::Validation("contribution is too large".into()));
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
        tax: None,
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

    #[test]
    fn absurd_input_is_an_error_not_a_panic() {
        let huge = input(
            vec![pos("A", Decimal::MAX, dec!(1000), dec!(100))],
            dec!(100),
            Mode::BuyOnly,
        );
        assert!(compute(&huge).is_err());
        assert_eq!(MAX_AMOUNT, dec!(1000000000000));
        // The largest accepted values must not overflow when multiplied and summed.
        let edge = input(
            vec![pos("A", MAX_AMOUNT, MAX_AMOUNT, dec!(100))],
            MAX_AMOUNT,
            Mode::BuyOnly,
        );
        assert!(compute(&edge).is_ok());
    }

    /// Deterministic pseudo-random inputs (no extra crate): every one must keep the invariants.
    #[test]
    fn invariants_hold_over_many_inputs() {
        let mut seed = 0x2545_F491_4F6C_DD1D_u64;
        let mut next = move |m: u64| {
            seed = seed
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            (seed >> 33) % m
        };
        let mut checked = 0;
        for _ in 0..400 {
            let n = 2 + next(5) as usize;
            let w: Vec<u64> = (0..n).map(|_| 1 + next(9)).collect();
            let sum: u64 = w.iter().sum();
            let mut pct: Vec<u64> = w.iter().map(|x| 100 * x / sum).collect();
            pct[0] += 100 - pct.iter().sum::<u64>();
            let positions: Vec<Position> = (0..n)
                .map(|i| {
                    let fractional = next(4) == 0;
                    Position {
                        id: format!("P{i}"),
                        quantity: if fractional {
                            Decimal::new(next(40_000) as i64, 3)
                        } else {
                            Decimal::from(next(40))
                        },
                        price: Decimal::new(1 + next(50_000) as i64, 2),
                        target_percent: Decimal::from(pct[i]),
                        fractional,
                    }
                })
                .collect();
            let contribution = Decimal::from(next(5500) as i64 - 500);
            let mode = if next(2) == 0 {
                Mode::BuyOnly
            } else {
                Mode::Full
            };
            let case = RebalanceInput {
                positions: positions.clone(),
                contribution,
                mode,
                tolerance_percent: dec!(5),
                fraction_decimals: 3,
            };
            let Ok(r) = compute(&case) else { continue }; // e.g. buy-only with a withdrawal
            checked += 1;
            let spent: Decimal = r.positions.iter().map(|p| p.trade_value).sum();
            assert_eq!(r.cash_left, contribution - spent, "cash adds up: {case:?}");
            if contribution >= Decimal::ZERO {
                assert!(r.cash_left >= Decimal::ZERO, "no overspending: {case:?}");
            }
            for (p, res) in positions.iter().zip(&r.positions) {
                assert!(
                    p.quantity + res.units_to_trade >= Decimal::ZERO,
                    "sold more than held: {case:?}"
                );
                if mode == Mode::BuyOnly {
                    assert!(
                        res.units_to_trade >= Decimal::ZERO,
                        "buy-only sold: {case:?}"
                    );
                }
            }
        }
        assert!(
            checked >= 150,
            "the sweep must reach enough valid inputs, got {checked}"
        );
    }

    #[test]
    fn the_tolerance_edge_is_exclusive() {
        let at = |a: i64, b: i64| {
            compute(&input(
                vec![
                    pos("A", Decimal::from(a), dec!(1), dec!(50)),
                    pos("B", Decimal::from(b), dec!(1), dec!(50)),
                ],
                dec!(0),
                Mode::Full,
            ))
            .unwrap()
        };
        let exactly = at(550_000, 450_000); // 55.0000% against 50%: deviation exactly 5
        assert_eq!(exactly.max_deviation_before, dec!(5));
        assert!(
            !exactly.needs_rebalance,
            "a deviation equal to the tolerance is still within it"
        );
        assert!(at(550_001, 449_999).needs_rebalance, "5.0001 is over");
    }
}

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    /// Decimal with up to 4 decimal places and magnitude up to `max_units` (whole units).
    fn money(max_units: i64) -> impl Strategy<Value = Decimal> {
        (0..=max_units * 10_000).prop_map(|m| Decimal::new(m, 4))
    }

    /// `n` target weights in whole hundredths of a percent that sum to exactly 100.
    fn weights(n: usize) -> impl Strategy<Value = Vec<Decimal>> {
        proptest::collection::vec(0..=10_000i64, n - 1).prop_map(move |mut cuts| {
            cuts.sort_unstable();
            let mut prev = 0;
            let mut w: Vec<Decimal> = cuts
                .iter()
                .map(|&c| {
                    let d = Decimal::new(c - prev, 2);
                    prev = c;
                    d
                })
                .collect();
            w.push(Decimal::new(10_000 - prev, 2));
            w
        })
    }

    /// Valid input; `max_units` bounds price, quantity and contribution.
    fn valid_input(max_units: i64, mode: Mode) -> impl Strategy<Value = RebalanceInput> {
        (1usize..=6).prop_flat_map(move |n| {
            let positions = (
                weights(n),
                proptest::collection::vec(
                    (money(max_units), 1..=max_units * 10_000, any::<bool>()),
                    n,
                ),
            )
                .prop_map(|(w, rest)| {
                    rest.into_iter()
                        .zip(w)
                        .enumerate()
                        .map(
                            |(i, ((quantity, price, fractional), target_percent))| Position {
                                id: format!("p{i}"),
                                quantity,
                                price: Decimal::new(price, 4),
                                target_percent,
                                fractional,
                            },
                        )
                        .collect::<Vec<_>>()
                });
            let contribution = match mode {
                Mode::BuyOnly => (0..=max_units * 10_000).boxed(),
                Mode::Full => (-max_units * 10_000..=max_units * 10_000).boxed(),
            }
            .prop_map(|c| Decimal::new(c, 4));
            (positions, contribution, 0u32..=8, 0..=10_000i64).prop_map(
                move |(positions, contribution, dp, tol)| RebalanceInput {
                    positions,
                    contribution,
                    mode,
                    tolerance_percent: Decimal::new(tol, 2),
                    fraction_decimals: dp,
                },
            )
        })
    }

    fn p_zero_weight(input: &RebalanceInput, id: &str) -> bool {
        input
            .positions
            .iter()
            .any(|p| p.id == id && p.target_percent.is_zero())
    }

    fn any_mode() -> impl Strategy<Value = Mode> {
        prop_oneof![Just(Mode::BuyOnly), Just(Mode::Full)]
    }

    proptest! {
        // compute() must never panic (a release build aborts on panic), at any size up to MAX_AMOUNT.
        #[test]
        fn never_panics_even_at_max_amount(
            input in any_mode().prop_flat_map(|m| valid_input(1_000_000_000_000, m))
        ) {
            let _ = compute(&input);
        }

        #[test]
        fn spending_and_quantities_stay_within_bounds(
            input in any_mode().prop_flat_map(|m| valid_input(1_000_000, m))
        ) {
            let r = match compute(&input) {
                Ok(r) => r,
                Err(CoreError::EmptyPortfolio) => return Ok(()),
                Err(e) => return Err(TestCaseError::fail(format!("unexpected error: {e}"))),
            };
            let spent: Decimal = r.positions.iter().map(|p| p.trade_value).sum();
            // purchases never exceed the deposit; the bookkeeping adds up
            prop_assert!(r.cash_left >= Decimal::ZERO, "cash_left {}", r.cash_left);
            prop_assert_eq!(r.cash_left, input.contribution - spent);
            prop_assert!(spent <= input.contribution);
            prop_assert_eq!(r.total_after, r.total_before + spent);
            for (p, res) in input.positions.iter().zip(&r.positions) {
                prop_assert!(p.quantity + res.units_to_trade >= Decimal::ZERO, "oversold {}", p.id);
                prop_assert!(res.value_after >= Decimal::ZERO);
                prop_assert_eq!(res.value_after, (p.quantity + res.units_to_trade) * p.price);
                if p.fractional {
                    prop_assert!(res.units_to_trade.scale() <= input.fraction_decimals);
                } else {
                    prop_assert!(res.units_to_trade.fract().is_zero(), "fractional units for {}", p.id);
                }
            }
        }

        #[test]
        fn buy_only_never_sells(input in valid_input(1_000_000, Mode::BuyOnly)) {
            let r = match compute(&input) {
                Ok(r) => r,
                Err(CoreError::EmptyPortfolio) => return Ok(()),
                Err(e) => return Err(TestCaseError::fail(format!("unexpected error: {e}"))),
            };
            for res in &r.positions {
                prop_assert!(res.units_to_trade >= Decimal::ZERO, "sold {}", res.id);
                prop_assert!(res.trade_value >= Decimal::ZERO);
                if p_zero_weight(&input, &res.id) {
                    prop_assert!(res.units_to_trade.is_zero(), "bought zero-weight {}", res.id);
                }
            }
        }

        #[test]
        fn result_does_not_depend_on_position_order(
            input in any_mode().prop_flat_map(|m| valid_input(1_000_000, m))
        ) {
            let key = |p: &Position| (p.quantity, p.price, p.target_percent, p.fractional);
            for (i, a) in input.positions.iter().enumerate() {
                // shortcut: exact ties are resolved by position index, so they are excluded here
                prop_assume!(input.positions[i + 1..].iter().all(|b| key(a) != key(b)));
            }
            let mut reversed = input.clone();
            reversed.positions.reverse();
            let (a, b) = (compute(&input), compute(&reversed));
            let (Ok(a), Ok(b)) = (a, b) else { return Ok(()) };
            prop_assert_eq!(a.total_before, b.total_before);
            prop_assert_eq!(a.needs_rebalance, b.needs_rebalance);
            for res in &a.positions {
                let other = b.positions.iter().find(|x| x.id == res.id).unwrap();
                prop_assert_eq!(res.units_to_trade, other.units_to_trade, "id {}", res.id);
                prop_assert_eq!(res.trade_value, other.trade_value);
                prop_assert_eq!(res.value_after, other.value_after);
            }
            prop_assert_eq!(a.cash_left, b.cash_left);
        }
    }
}
