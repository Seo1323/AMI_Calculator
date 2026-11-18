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
        Meta { id: Uuid::now_v7(), created_at: now, updated_at: now, deleted_at: None }
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

// ---------- Budget ----------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AccountKind {
    Cash,
    Checking,
    Savings,
    CreditCard,
    Brokerage,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    #[serde(flatten)]
    pub meta: Meta,
    pub name: String,
    pub kind: AccountKind,
    pub currency: Currency,
    pub opening_balance: Decimal,
    #[serde(default)]
    pub archived: bool,
    #[serde(default)]
    pub color: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CategoryKind {
    Income,
    Expense,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Category {
    #[serde(flatten)]
    pub meta: Meta,
    pub name: String,
    pub kind: CategoryKind,
    #[serde(default)]
    pub parent_id: Option<Uuid>,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub icon: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Transaction {
    #[serde(flatten)]
    pub meta: Meta,
    pub account_id: Uuid,
    pub date: NaiveDate,
    /// Signed: positive = money in, negative = money out (account currency).
    pub amount: Decimal,
    #[serde(default)]
    pub category_id: Option<Uuid>,
    /// Set for transfers: id of the paired transaction on the other account.
    #[serde(default)]
    pub transfer_pair_id: Option<Uuid>,
    #[serde(default)]
    pub note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BudgetLimit {
    #[serde(flatten)]
    pub meta: Meta,
    pub category_id: Uuid,
    /// First day of the month this limit applies to.
    pub month: NaiveDate,
    pub amount: Decimal,
    pub currency: Currency,
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
    Other,
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
