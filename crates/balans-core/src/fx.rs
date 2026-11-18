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
        FxTable { base, rates: HashMap::new() }
    }

    pub fn set(&mut self, currency: Currency, rate_in_base: Decimal) {
        self.rates.insert(currency, rate_in_base);
    }

    fn rate(&self, c: &Currency) -> Result<Decimal> {
        if *c == self.base {
            return Ok(Decimal::ONE);
        }
        self.rates.get(c).copied().ok_or_else(|| CoreError::MissingRate(c.to_string()))
    }

    pub fn convert(&self, money: &Money, to: &Currency) -> Result<Money> {
        let in_base = money.amount * self.rate(&money.currency)?;
        let amount = in_base / self.rate(to)?;
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

        let m = fx.convert(&Money::new(dec!(100), eur.clone()), &pln).unwrap();
        assert_eq!(m.amount, dec!(430));
        let m = fx.convert(&Money::new(dec!(100), eur), &usd).unwrap();
        assert_eq!(m.amount, dec!(107.5));
        assert!(fx.convert(&Money::new(dec!(1), Currency::new("GBP").unwrap()), &pln).is_err());
    }
}
