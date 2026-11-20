//! Persistent entities. Every record has a UUIDv7 id and timestamps
//! (`created_at`, `updated_at`, optional `deleted_at` tombstone) so a future
//! sync can merge by last-write-wins without a schema rewrite.

use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::money::Currency;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Meta {
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<DateTime<Utc>>,
}

impl Meta {
    pub fn new() -> Self {
        let now = Utc::now();
        Meta {
            id: Uuid::now_v7(),
            created_at: now,
            updated_at: now,
            deleted_at: None,
        }
    }

    pub fn touch(&mut self) {
        self.updated_at = Utc::now();
    }
}

impl Default for Meta {
    fn default() -> Self {
        Self::new()
    }
}

// ---------- Portfolio ----------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum InstrumentKind {
    Stock,
    Etf,
    Bond,
    Crypto,
    Cash,
    /// Bank deposit: valued at its principal, the rate is entered by hand.
    Deposit,
    Other,
}

/// How often interest is added to the principal. `None` on an instrument means no capitalization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Capitalization {
    Monthly,
    Quarterly,
    Annual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum QuoteSource {
    /// Yahoo Finance: WSE (`PKN.WA`) and foreign exchanges (`VWCE.DE`, `AAPL`).
    /// Older data files used "stooq", which no longer serves automated requests.
    #[serde(alias = "stooq")]
    Yahoo,
    /// Crypto.
    CoinGecko,
    /// User enters the price.
    Manual,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Instrument {
    #[serde(flatten)]
    pub meta: Meta,
    /// Symbol at the quote source, e.g. `ETFBW20TR` (GPW), `VWCE.DE`, `bitcoin`.
    pub symbol: String,
    pub name: String,
    pub kind: InstrumentKind,
    pub currency: Currency,
    pub quote_source: QuoteSource,
    /// Whether fractional units can be bought (crypto: yes, GPW: no).
    pub fractional: bool,
    /// Annual interest rate in percent, entered by hand (bonds, cash). Market-quoted
    /// instruments get their rates from the exchange instead (see `Quote`).
    #[serde(default)]
    pub interest_percent: Option<Decimal>,
    /// Interest capitalization of a bond or deposit (`None`: no capitalization).
    #[serde(default)]
    pub capitalization: Option<Capitalization>,
    /// Bond purchase/start date; the maturity is this date plus the term.
    #[serde(default)]
    pub start_date: Option<NaiveDate>,
    /// Maturity date of a bond.
    #[serde(default)]
    pub maturity_date: Option<NaiveDate>,
}

/// Tax wrapper of the account the portfolio lives in: decides whether selling is taxed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AccountType {
    /// Ordinary brokerage account: 19% tax on realized gains.
    #[default]
    Regular,
    /// IKE: no tax on selling inside the account.
    Ike,
    /// IKZE: no tax on selling inside the account (tax is due on withdrawal, not modelled).
    Ikze,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Portfolio {
    #[serde(flatten)]
    pub meta: Meta,
    pub name: String,
    pub base_currency: Currency,
    /// Allowed drift in percentage points before a rebalance is suggested.
    pub tolerance_percent: Decimal,
    /// Missing in data saved before 0.5.1: an ordinary account.
    #[serde(default)]
    pub account_type: AccountType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AllocationTarget {
    #[serde(flatten)]
    pub meta: Meta,
    pub portfolio_id: Uuid,
    pub instrument_id: Uuid,
    pub target_percent: Decimal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TradeSide {
    Buy,
    Sell,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Trade {
    #[serde(flatten)]
    pub meta: Meta,
    pub portfolio_id: Uuid,
    pub instrument_id: Uuid,
    pub date: NaiveDate,
    pub side: TradeSide,
    pub quantity: Decimal,
    /// Price per unit in the instrument currency.
    pub price: Decimal,
    #[serde(default)]
    pub fee: Decimal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Quote {
    pub instrument_id: Uuid,
    /// Price per unit in `currency`.
    pub price: Decimal,
    pub currency: Currency,
    /// "manual", "yahoo", "coingecko", ...
    #[serde(default = "manual")]
    pub source: String,
    pub fetched_at: DateTime<Utc>,
    /// Price change over the last 12 months in percent (from the exchange), if known.
    #[serde(default)]
    pub return_12m_percent: Option<Decimal>,
    /// Dividends paid over the last 12 months as a percent of the price (ETFs), if known.
    #[serde(default)]
    pub dividend_yield_percent: Option<Decimal>,
}

/// Exchange rate: how many PLN one unit of `currency` is worth (NBP convention).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FxRate {
    pub currency: Currency,
    pub rate_pln: Decimal,
    #[serde(default = "manual")]
    pub source: String,
    pub fetched_at: DateTime<Utc>,
}

fn manual() -> String {
    "manual".into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn bond() -> Instrument {
        Instrument {
            meta: Meta::new(),
            symbol: "EDO0436".into(),
            name: "Obligacje 10-letnie".into(),
            kind: InstrumentKind::Bond,
            currency: Currency::new("PLN").unwrap(),
            quote_source: QuoteSource::Manual,
            fractional: false,
            interest_percent: Some(dec!(5.75)),
            capitalization: Some(Capitalization::Annual),
            start_date: NaiveDate::from_ymd_opt(2026, 4, 1),
            maturity_date: NaiveDate::from_ymd_opt(2036, 4, 1),
        }
    }

    #[test]
    fn manual_rate_and_maturity_round_trip() {
        let b = bond();
        let back: Instrument = serde_json::from_str(&serde_json::to_string(&b).unwrap()).unwrap();
        assert_eq!(back.interest_percent, Some(dec!(5.75)));
        assert_eq!(back.maturity_date, b.maturity_date);
        assert_eq!(back.start_date, NaiveDate::from_ymd_opt(2026, 4, 1));
        assert_eq!(back.capitalization, Some(Capitalization::Annual));
    }

    #[test]
    fn data_saved_before_rates_existed_still_loads() {
        let mut v = serde_json::to_value(bond()).unwrap();
        let o = v.as_object_mut().unwrap();
        o.remove("interestPercent");
        o.remove("maturityDate");
        o.remove("startDate");
        o.remove("capitalization");
        let old: Instrument = serde_json::from_value(v).unwrap();
        assert_eq!(
            (
                old.interest_percent,
                old.start_date,
                old.maturity_date,
                old.capitalization
            ),
            (None, None, None, None)
        );

        let quote = r#"{"instrumentId":"018f0000-0000-7000-8000-000000000000","price":"10","currency":"PLN",
            "source":"yahoo","fetchedAt":"2026-11-19T10:00:00Z"}"#;
        let q: Quote = serde_json::from_str(quote).unwrap();
        assert_eq!(
            (q.return_12m_percent, q.dividend_yield_percent),
            (None, None)
        );
    }

    #[test]
    fn capitalization_wire_format_is_camel_case() {
        for (c, text) in [
            (Capitalization::Monthly, "\"monthly\""),
            (Capitalization::Quarterly, "\"quarterly\""),
            (Capitalization::Annual, "\"annual\""),
        ] {
            assert_eq!(serde_json::to_string(&c).unwrap(), text);
        }
        let mut b = bond();
        b.capitalization = None;
        let v = serde_json::to_value(&b).unwrap();
        assert!(
            v["capitalization"].is_null(),
            "the frontend reads null as 'none'"
        );
    }

    #[test]
    fn portfolio_saved_before_account_types_is_an_ordinary_account() {
        let json = serde_json::json!({
            "id": Uuid::nil(), "createdAt": "2026-11-01T00:00:00Z", "updatedAt": "2026-11-01T00:00:00Z",
            "name": "Stary", "baseCurrency": "PLN", "tolerancePercent": "5"
        });
        let p: Portfolio = serde_json::from_value(json).unwrap();
        assert_eq!(p.account_type, AccountType::Regular);
    }
}
