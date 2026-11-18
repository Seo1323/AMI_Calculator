//! Quote and exchange-rate providers: URL builders and response parsers.
//!
//! Network access lives in the app crate; everything here is pure so it can be
//! tested against recorded responses.
//!
//! * NBP (Narodowy Bank Polski): official PLN mid rates, table A.
//! * Yahoo Finance chart API (unofficial): WSE tickers with `.WA` (e.g. `PKN.WA`),
//!   foreign ones with their exchange suffix (`VWCE.DE`) or none (`AAPL`).
//! * CoinGecko simple price API: crypto by CoinGecko id (`bitcoin`, `ethereum`).

use std::str::FromStr;

use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde_json::Value;

use crate::error::{CoreError, Result};
use crate::money::Currency;

fn provider(msg: impl Into<String>) -> CoreError {
    CoreError::Provider(msg.into())
}

/// JSON number -> exact Decimal (via its shortest decimal representation).
fn decimal(v: &Value) -> Option<Decimal> {
    match v {
        Value::Number(n) => Decimal::from_str(&n.to_string()).or_else(|_| Decimal::from_scientific(&n.to_string())).ok(),
        Value::String(s) => Decimal::from_str(s).ok(),
        _ => None,
    }
}

// ---------- NBP ----------

pub const NBP_TABLE_A_URL: &str = "https://api.nbp.pl/api/exchangerates/tables/A/?format=json";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NbpRate {
    pub currency: Currency,
    /// PLN per one unit.
    pub mid: Decimal,
    pub effective_date: NaiveDate,
}

pub fn parse_nbp_table(body: &str) -> Result<Vec<NbpRate>> {
    let v: Value = serde_json::from_str(body).map_err(|e| provider(format!("NBP: invalid response ({e})")))?;
    let table = v.get(0).ok_or_else(|| provider("NBP: empty response"))?;
    let date = table
        .get("effectiveDate")
        .and_then(Value::as_str)
        .and_then(|s| NaiveDate::from_str(s).ok())
        .ok_or_else(|| provider("NBP: missing effectiveDate"))?;
    let rates = table.get("rates").and_then(Value::as_array).ok_or_else(|| provider("NBP: missing rates"))?;
    Ok(rates
        .iter()
        .filter_map(|r| {
            let currency = Currency::new(r.get("code")?.as_str()?).ok()?;
            let mid = decimal(r.get("mid")?)?;
            (mid > Decimal::ZERO).then_some(NbpRate { currency, mid, effective_date: date })
        })
        .collect())
}

// ---------- Yahoo Finance ----------

pub fn yahoo_url(symbol: &str) -> String {
    let encoded: String = symbol
        .trim()
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'.' | b'-' | b'^' | b'=' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect();
    format!("https://query1.finance.yahoo.com/v8/finance/chart/{encoded}?range=1d&interval=1d")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct YahooQuote {
    pub price: Decimal,
    pub currency: Currency,
    pub time: DateTime<Utc>,
    pub name: Option<String>,
}

pub fn parse_yahoo(body: &str) -> Result<YahooQuote> {
    let v: Value = serde_json::from_str(body).map_err(|e| provider(format!("Yahoo: invalid response ({e})")))?;
    let chart = v.get("chart").ok_or_else(|| provider("Yahoo: invalid response"))?;
    if let Some(desc) = chart.get("error").and_then(|e| e.get("description")).and_then(Value::as_str) {
        return Err(provider(format!("Yahoo: {desc}")));
    }
    let meta = chart
        .get("result")
        .and_then(|r| r.get(0))
        .and_then(|r| r.get("meta"))
        .ok_or_else(|| provider("Yahoo: symbol not found"))?;
    let mut price = meta.get("regularMarketPrice").and_then(decimal).ok_or_else(|| provider("Yahoo: no price"))?;
    let raw_currency = meta.get("currency").and_then(Value::as_str).ok_or_else(|| provider("Yahoo: no currency"))?;
    // London quotes come in pence ("GBp"); some others use minor units too.
    let currency = match raw_currency {
        "GBp" | "GBX" => {
            price /= Decimal::ONE_HUNDRED;
            Currency::new("GBP")?
        }
        "ZAc" => {
            price /= Decimal::ONE_HUNDRED;
            Currency::new("ZAR")?
        }
        "ILA" => {
            price /= Decimal::ONE_HUNDRED;
            Currency::new("ILS")?
        }
        other => Currency::new(other)?,
    };
    let time = meta
        .get("regularMarketTime")
        .and_then(Value::as_i64)
        .and_then(|t| DateTime::from_timestamp(t, 0))
        .unwrap_or_else(Utc::now);
    let name = ["longName", "shortName"].iter().find_map(|k| meta.get(*k).and_then(Value::as_str)).map(str::to_string);
    Ok(YahooQuote { price: price.normalize(), currency, time, name })
}

// ---------- CoinGecko ----------

pub fn coingecko_url(ids: &[&str], vs_currencies: &[&str]) -> String {
    let clean = |xs: &[&str]| {
        let mut v: Vec<String> = xs
            .iter()
            .map(|x| x.trim().to_lowercase().chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-').collect())
            .filter(|x: &String| !x.is_empty())
            .collect();
        v.sort();
        v.dedup();
        v.join(",")
    };
    format!(
        "https://api.coingecko.com/api/v3/simple/price?ids={}&vs_currencies={}",
        clean(ids),
        clean(vs_currencies)
    )
}

/// Price of `id` in `vs` (e.g. "bitcoin", "pln") from a simple/price response.
pub fn parse_coingecko(body: &str, id: &str, vs: &str) -> Result<Option<Decimal>> {
    let v: Value = serde_json::from_str(body).map_err(|e| provider(format!("CoinGecko: invalid response ({e})")))?;
    if let Some(status) = v.get("status").and_then(|s| s.get("error_message")).and_then(Value::as_str) {
        return Err(provider(format!("CoinGecko: {status}")));
    }
    Ok(v.get(id.trim().to_lowercase()).and_then(|c| c.get(vs.to_lowercase())).and_then(decimal))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn nbp_table() {
        let body = r#"[{"table":"A","no":"224/A/NBP/2026","effectiveDate":"2026-11-18","rates":[
            {"currency":"dolar amerykański","code":"USD","mid":4.0170},
            {"currency":"euro","code":"EUR","mid":4.2731},
            {"currency":"bad","code":"X","mid":1}]}]"#;
        let rates = parse_nbp_table(body).unwrap();
        assert_eq!(rates.len(), 2);
        assert_eq!(rates[0].currency.code(), "USD");
        assert_eq!(rates[0].mid, dec!(4.017));
        assert_eq!(rates[1].mid, dec!(4.2731));
        assert_eq!(rates[1].effective_date, NaiveDate::from_ymd_opt(2026, 11, 18).unwrap());
        assert!(parse_nbp_table("<html>").is_err());
    }

    #[test]
    fn yahoo_quote_and_errors() {
        let ok = r#"{"chart":{"result":[{"meta":{"currency":"PLN","symbol":"PKN.WA","regularMarketPrice":128.96,
            "regularMarketTime":1795017871,"longName":"Orlen S.A."}}],"error":null}}"#;
        let q = parse_yahoo(ok).unwrap();
        assert_eq!(q.price, dec!(128.96));
        assert_eq!(q.currency.code(), "PLN");
        assert_eq!(q.name.as_deref(), Some("Orlen S.A."));

        let pence = r#"{"chart":{"result":[{"meta":{"currency":"GBp","regularMarketPrice":8123.5}}],"error":null}}"#;
        let q = parse_yahoo(pence).unwrap();
        assert_eq!(q.price, dec!(81.235));
        assert_eq!(q.currency.code(), "GBP");

        let missing = r#"{"chart":{"result":null,"error":{"code":"Not Found","description":"No data found, symbol may be delisted"}}}"#;
        let err = parse_yahoo(missing).unwrap_err();
        assert!(err.to_string().contains("No data found"));
    }

    #[test]
    fn yahoo_url_encodes() {
        assert_eq!(yahoo_url(" PKN.WA "), "https://query1.finance.yahoo.com/v8/finance/chart/PKN.WA?range=1d&interval=1d");
        assert!(yahoo_url("A B/C").contains("A%20B%2FC"));
    }

    #[test]
    fn coingecko() {
        let url = coingecko_url(&["Bitcoin", "ethereum", "bitcoin", "x&y"], &["PLN"]);
        assert_eq!(url, "https://api.coingecko.com/api/v3/simple/price?ids=bitcoin,ethereum,xy&vs_currencies=pln");
        let body = r#"{"bitcoin":{"usd":62763,"pln":252506},"ethereum":{"pln":7061.53}}"#;
        assert_eq!(parse_coingecko(body, "bitcoin", "PLN").unwrap(), Some(dec!(252506)));
        assert_eq!(parse_coingecko(body, "ethereum", "pln").unwrap(), Some(dec!(7061.53)));
        assert_eq!(parse_coingecko(body, "dogecoin", "pln").unwrap(), None);
        let limited = r#"{"status":{"error_code":429,"error_message":"You've exceeded the Rate Limit."}}"#;
        assert!(parse_coingecko(limited, "bitcoin", "pln").is_err());
    }
}
