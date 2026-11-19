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

    #[test]
    fn currency_length_bounds_are_inclusive() {
        assert_eq!(Currency::new("ab").unwrap().code(), "AB");
        assert_eq!(Currency::new("abcdefghij").unwrap().code(), "ABCDEFGHIJ");
        assert!(Currency::new("abcdefghijk").is_err());
    }

    #[test]
    fn currency_rejects_empty_blank_and_inner_whitespace() {
        assert!(Currency::new("").is_err());
        assert!(Currency::new("   ").is_err());
        assert!(Currency::new("PL N").is_err());
    }

    #[test]
    fn currency_rejects_non_ascii_letters() {
        // `to_ascii_uppercase` leaves these untouched and they are not ASCII, so they must fail.
        assert!(Currency::new("zł").is_err());
        assert!(Currency::new("€UR").is_err());
    }

    #[test]
    fn currency_accepts_digits_and_reports_the_normalized_code_on_error() {
        assert_eq!(Currency::new("1inch").unwrap().code(), "1INCH");
        match Currency::new(" pl-n ") {
            Err(CoreError::InvalidCurrency(code)) => assert_eq!(code, "PL-N"),
            other => panic!("expected InvalidCurrency(\"PL-N\"), got {other:?}"),
        }
    }

    #[test]
    fn currency_deserialization_applies_the_same_validation() {
        let ok: Currency = serde_json::from_str("\" eur \"").unwrap();
        assert_eq!(ok.code(), "EUR");
        assert!(serde_json::from_str::<Currency>("\"E\"").is_err());
        assert!(serde_json::from_str::<Currency>("\"E-R\"").is_err());
    }

    #[test]
    fn currency_serializes_as_the_normalized_plain_string() {
        let c = Currency::new("eur").unwrap();
        assert_eq!(serde_json::to_string(&c).unwrap(), "\"EUR\"");
        let back: Currency = serde_json::from_str(&serde_json::to_string(&c).unwrap()).unwrap();
        assert_eq!(back, c);
    }

    #[test]
    fn currency_trims_tabs_and_newlines() {
        assert_eq!(Currency::new("\tEUR\n").unwrap().code(), "EUR");
    }

    #[test]
    fn money_deserialization_rejects_an_invalid_nested_currency() {
        let ok: Money = serde_json::from_str(r#"{"amount":"1.50","currency":" pln "}"#).unwrap();
        assert_eq!(ok.currency.code(), "PLN");
        assert!(serde_json::from_str::<Money>(r#"{"amount":"1.50","currency":"x"}"#).is_err());
    }
}
