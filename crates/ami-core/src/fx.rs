use std::collections::HashMap;

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};
use crate::money::{Currency, Money};

/// Exchange rates relative to a base currency:
/// `rates[X]` = how many units of `base` one unit of `X` is worth.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FxTable {
    pub base: Currency,
    pub rates: HashMap<Currency, Decimal>,
}

impl FxTable {
    pub fn new(base: Currency) -> Self {
        FxTable {
            base,
            rates: HashMap::new(),
        }
    }

    pub fn set(&mut self, currency: Currency, rate_in_base: Decimal) {
        self.rates.insert(currency, rate_in_base);
    }

    fn rate(&self, c: &Currency) -> Result<Decimal> {
        if *c == self.base {
            return Ok(Decimal::ONE);
        }
        self.rates
            .get(c)
            .copied()
            .ok_or_else(|| CoreError::MissingRate(c.to_string()))
    }

    pub fn convert(&self, money: &Money, to: &Currency) -> Result<Money> {
        let (from_rate, to_rate) = (self.rate(&money.currency)?, self.rate(to)?);
        let amount = money
            .amount
            .checked_mul(from_rate)
            .and_then(|in_base| in_base.checked_div(to_rate))
            .ok_or_else(|| CoreError::Validation("amount is out of range".into()))?;
        Ok(Money::new(amount, to.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn converts_through_base() {
        let pln = Currency::new("PLN").unwrap();
        let eur = Currency::new("EUR").unwrap();
        let usd = Currency::new("USD").unwrap();
        let mut fx = FxTable::new(pln.clone());
        fx.set(eur.clone(), dec!(4.30));
        fx.set(usd.clone(), dec!(4.00));

        let m = fx
            .convert(&Money::new(dec!(100), eur.clone()), &pln)
            .unwrap();
        assert_eq!(m.amount, dec!(430));
        let m = fx.convert(&Money::new(dec!(100), eur), &usd).unwrap();
        assert_eq!(m.amount, dec!(107.5));
        assert!(
            fx.convert(&Money::new(dec!(1), Currency::new("GBP").unwrap()), &pln)
                .is_err()
        );
    }

    #[test]
    fn overflow_is_an_error_not_a_panic() {
        let pln = Currency::new("PLN").unwrap();
        let eur = Currency::new("EUR").unwrap();
        let mut fx = FxTable::new(pln.clone());
        fx.set(eur.clone(), dec!(0.0000000000000000000001));
        // Dividing by a tiny rate overflows; a missing rate is still reported as such.
        let big = Money::new(Decimal::MAX / dec!(2), pln.clone());
        assert!(matches!(fx.convert(&big, &eur), Err(CoreError::Validation(_))));
        let gbp = Currency::new("GBP").unwrap();
        assert!(matches!(fx.convert(&big, &gbp), Err(CoreError::MissingRate(_))));
    }
}
