use std::fmt;

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};

/// Currency or asset code: ISO 4217 for fiat (PLN, EUR, USD) and ticker-like
/// codes for crypto (BTC, ETH, USDT). 2–10 uppercase letters or digits.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Currency(String);

impl Currency {
    pub fn new(code: &str) -> Result<Self> {
        let code = code.trim().to_ascii_uppercase();
        let ok = (2..=10).contains(&code.len())
            && code.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit());
        if ok { Ok(Currency(code)) } else { Err(CoreError::InvalidCurrency(code)) }
    }

    pub fn code(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for Currency {
    type Error = CoreError;
    fn try_from(s: String) -> Result<Self> {
        Currency::new(&s)
    }
}

impl From<Currency> for String {
    fn from(c: Currency) -> Self {
        c.0
    }
}

impl fmt::Display for Currency {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Money {
    pub amount: Decimal,
    pub currency: Currency,
}

impl Money {
    pub fn new(amount: Decimal, currency: Currency) -> Self {
        Money { amount, currency }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn currency_normalizes_and_validates() {
        assert_eq!(Currency::new(" pln ").unwrap().code(), "PLN");
        assert_eq!(Currency::new("btc").unwrap().code(), "BTC");
        assert!(Currency::new("P").is_err());
        assert!(Currency::new("PL-N").is_err());
    }
}
