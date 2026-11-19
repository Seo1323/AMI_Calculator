//! Quote and exchange-rate providers: URL builders and response parsers.
//!
//! Network access lives in the app crate; everything here is pure so it can be
//! tested against recorded responses.
//!
//! * NBP (Narodowy Bank Polski): official PLN mid rates, table A.
//! * Yahoo Finance chart API (unofficial): WSE tickers with `.WA` (e.g. `PKN.WA`),
//!   foreign ones with their exchange suffix (`VWCE.DE`) or none (`AAPL`).
//! * CoinGecko simple price API: crypto by CoinGecko id (`bitcoin`, `ethereum`).
//! * obligacjeskarbowe.pl (Ministry of Finance): the current offer of Polish retail treasury bonds.

use std::str::FromStr;

use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde_json::Value;

use crate::error::{CoreError, Result};
use crate::model::Capitalization;
use crate::money::Currency;

fn provider(msg: impl Into<String>) -> CoreError {
    CoreError::Provider(msg.into())
}

/// JSON number -> exact Decimal (via its shortest decimal representation).
fn decimal(v: &Value) -> Option<Decimal> {
    match v {
        Value::Number(n) => Decimal::from_str(&n.to_string())
            .or_else(|_| Decimal::from_scientific(&n.to_string()))
            .ok(),
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
    let v: Value =
        serde_json::from_str(body).map_err(|e| provider(format!("NBP: invalid response ({e})")))?;
    let table = v.get(0).ok_or_else(|| provider("NBP: empty response"))?;
    let date = table
        .get("effectiveDate")
        .and_then(Value::as_str)
        .and_then(|s| NaiveDate::from_str(s).ok())
        .ok_or_else(|| provider("NBP: missing effectiveDate"))?;
    let rates = table
        .get("rates")
        .and_then(Value::as_array)
        .ok_or_else(|| provider("NBP: missing rates"))?;
    Ok(rates
        .iter()
        .filter_map(|r| {
            let currency = Currency::new(r.get("code")?.as_str()?).ok()?;
            let mid = decimal(r.get("mid")?)?;
            (mid > Decimal::ZERO).then_some(NbpRate {
                currency,
                mid,
                effective_date: date,
            })
        })
        .collect())
}

// ---------- Yahoo Finance ----------

pub fn yahoo_url(symbol: &str) -> String {
    let encoded: String = symbol
        .trim()
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'.' | b'-' | b'^' | b'=' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect();
    format!(
        "https://query1.finance.yahoo.com/v8/finance/chart/{encoded}?range=1y&interval=1d&events=div"
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct YahooQuote {
    pub price: Decimal,
    pub currency: Currency,
    pub time: DateTime<Utc>,
    pub name: Option<String>,
    /// Price change over ~12 months in percent; `None` when the history is shorter.
    pub return_12m_percent: Option<Decimal>,
    /// Dividends of the last 12 months as a percent of the price; `None` when none were paid.
    pub dividend_yield_percent: Option<Decimal>,
}

pub fn parse_yahoo(body: &str) -> Result<YahooQuote> {
    let v: Value = serde_json::from_str(body)
        .map_err(|e| provider(format!("Yahoo: invalid response ({e})")))?;
    let chart = v
        .get("chart")
        .ok_or_else(|| provider("Yahoo: invalid response"))?;
    if let Some(desc) = chart
        .get("error")
        .and_then(|e| e.get("description"))
        .and_then(Value::as_str)
    {
        return Err(provider(format!("Yahoo: {desc}")));
    }
    let result = chart
        .get("result")
        .and_then(|r| r.get(0))
        .ok_or_else(|| provider("Yahoo: symbol not found"))?;
    let meta = result
        .get("meta")
        .ok_or_else(|| provider("Yahoo: symbol not found"))?;
    let mut price = meta
        .get("regularMarketPrice")
        .and_then(decimal)
        .ok_or_else(|| provider("Yahoo: no price"))?;
    // Rates are ratios, so they are computed before the minor-unit conversion below.
    let raw_price = price;
    let raw_currency = meta
        .get("currency")
        .and_then(Value::as_str)
        .ok_or_else(|| provider("Yahoo: no currency"))?;
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
    let name = ["longName", "shortName"]
        .iter()
        .find_map(|k| meta.get(*k).and_then(Value::as_str))
        .map(str::to_string);
    let (return_12m_percent, dividend_yield_percent) = yahoo_rates(result, raw_price, time);
    Ok(YahooQuote {
        price: price.normalize(),
        currency,
        time,
        name,
        return_12m_percent,
        dividend_yield_percent,
    })
}

/// Seconds in the window the rates cover.
const YEAR_SECS: i64 = 365 * 86_400;
/// A history whose first bar is younger than this is too short to call a "12-month" figure.
const MIN_HISTORY_SECS: i64 = 350 * 86_400;

/// 12-month return and trailing dividend yield from a `range=1y&interval=1d&events=div` chart.
///
/// The input is remote data, so every operation is checked: garbage such as a 1e-28 close must
/// give `None`, not a panic inside the refresh.
fn yahoo_rates(
    result: &Value,
    price: Decimal,
    now: DateTime<Utc>,
) -> (Option<Decimal>, Option<Decimal>) {
    if price <= Decimal::ZERO {
        return (None, None);
    }
    let first = result
        .get("timestamp")
        .and_then(Value::as_array)
        .zip(
            result
                .get("indicators")
                .and_then(|i| i.get("quote"))
                .and_then(|q| q.get(0))
                .and_then(|q| q.get("close"))
                .and_then(Value::as_array),
        )
        .and_then(|(ts, closes)| {
            ts.iter()
                .zip(closes)
                .find_map(|(t, c)| Some((t.as_i64()?, decimal(c)?)))
        });
    let return_12m = first
        .filter(|(t, close)| *t <= now.timestamp() - MIN_HISTORY_SECS && *close > Decimal::ZERO)
        .and_then(|(_, close)| {
            price
                .checked_div(close)?
                .checked_sub(Decimal::ONE)?
                .checked_mul(Decimal::ONE_HUNDRED)
        })
        .map(|p| p.round_dp(2));
    let since = now.timestamp() - YEAR_SECS;
    let paid = result
        .get("events")
        .and_then(|e| e.get("dividends"))
        .and_then(Value::as_object)
        .and_then(|d| {
            d.values()
                // A dividend without a date is trusted to be inside the requested range.
                .filter(|x| {
                    x.get("date")
                        .and_then(Value::as_i64)
                        .is_none_or(|t| t >= since)
                })
                .filter_map(|x| x.get("amount").and_then(decimal))
                .try_fold(Decimal::ZERO, |sum, amount| sum.checked_add(amount))
        })
        .filter(|paid| *paid > Decimal::ZERO);
    let dividend_yield = paid
        .and_then(|paid| paid.checked_div(price)?.checked_mul(Decimal::ONE_HUNDRED))
        .map(|p| p.round_dp(2));
    (return_12m, dividend_yield)
}

// ---------- CoinGecko ----------

fn clean_list(xs: &[&str]) -> String {
    let mut v: Vec<String> = xs
        .iter()
        .map(|x| {
            x.trim()
                .to_lowercase()
                .chars()
                .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
                .collect()
        })
        .filter(|x: &String| !x.is_empty())
        .collect();
    v.sort();
    v.dedup();
    v.join(",")
}

pub fn coingecko_url(ids: &[&str], vs_currencies: &[&str]) -> String {
    format!(
        "https://api.coingecko.com/api/v3/simple/price?ids={}&vs_currencies={}",
        clean_list(ids),
        clean_list(vs_currencies)
    )
}

/// One currency per request: the market endpoint takes a single `vs_currency`.
pub fn coingecko_markets_url(ids: &[&str], vs_currency: &str) -> String {
    format!(
        "https://api.coingecko.com/api/v3/coins/markets?vs_currency={}&ids={}&price_change_percentage=1y",
        clean_list(&[vs_currency]),
        clean_list(ids)
    )
}

/// 12-month price change in percent of `id` from a coins/markets response.
pub fn parse_coingecko_return(body: &str, id: &str) -> Result<Option<Decimal>> {
    let v: Value = serde_json::from_str(body)
        .map_err(|e| provider(format!("CoinGecko: invalid response ({e})")))?;
    if let Some(status) = v
        .get("status")
        .and_then(|s| s.get("error_message"))
        .and_then(Value::as_str)
    {
        return Err(provider(format!("CoinGecko: {status}")));
    }
    let id = id.trim().to_lowercase();
    Ok(v.as_array()
        .and_then(|coins| {
            coins
                .iter()
                .find(|c| c.get("id").and_then(Value::as_str) == Some(id.as_str()))
        })
        .and_then(|c| c.get("price_change_percentage_1y_in_currency"))
        .and_then(decimal)
        // Below -100 % is impossible and beyond a million percent is garbage.
        .filter(|p| *p >= Decimal::from(-100) && *p < Decimal::from(1_000_000))
        .map(|p| p.round_dp(2)))
}

/// Price of `id` in `vs` (e.g. "bitcoin", "pln") from a simple/price response.
pub fn parse_coingecko(body: &str, id: &str, vs: &str) -> Result<Option<Decimal>> {
    let v: Value = serde_json::from_str(body)
        .map_err(|e| provider(format!("CoinGecko: invalid response ({e})")))?;
    if let Some(status) = v
        .get("status")
        .and_then(|s| s.get("error_message"))
        .and_then(Value::as_str)
    {
        return Err(provider(format!("CoinGecko: {status}")));
    }
    Ok(v.get(id.trim().to_lowercase())
        .and_then(|c| c.get(vs.to_lowercase()))
        .and_then(decimal))
}

// ---------- Polish retail treasury bonds (obligacjeskarbowe.pl) ----------

pub const BOND_SITE: &str = "https://www.obligacjeskarbowe.pl";
pub const BOND_OFFER_URL: &str = "https://www.obligacjeskarbowe.pl/oferta-obligacji/";

/// One series of the current monthly offer, e.g. `EDO1136`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BondOffer {
    /// Series code without the maturity part: `EDO`.
    pub series: String,
    /// Full symbol: series + maturity month and year, `EDO1136`.
    pub symbol: String,
    /// Annual rate of the first interest period in percent. Later periods of floating and
    /// inflation-indexed bonds depend on inflation or the NBP rate and are not known upfront.
    pub first_period_rate_percent: Decimal,
    pub capitalization: Option<Capitalization>,
    /// Sale price of one bond in PLN (nominal 100).
    pub price: Decimal,
}

/// The offer as far as it could be read: series whose page changed or could not be parsed are
/// listed in `skipped` instead of silently disappearing.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BondOffers {
    pub offers: Vec<BondOffer>,
    /// Symbols (`EDO1136`) of series that were found but not understood.
    pub skipped: Vec<String>,
}

/// Text of an HTML fragment: tags dropped, the few entities that occur decoded, spaces collapsed.
fn html_text(fragment: &str) -> String {
    let mut out = String::with_capacity(fragment.len());
    let mut in_tag = false;
    for c in fragment.chars() {
        match c {
            '<' => in_tag = true,
            '>' => {
                in_tag = false;
                out.push(' ');
            }
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.replace("&#160;", " ")
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Value of a `<strong class="product-details__list-label">Label:</strong>` row: the text of the
/// `list-value` element of the SAME `<li>` row. The first row with that label wins. A row without
/// a value yields `None` rather than the value of the next row.
fn offer_field(html: &str, label: &str) -> Option<String> {
    let needle = format!(">{label}");
    let label_end = html.match_indices(&needle).find_map(|(i, _)| {
        let rest = &html[i + needle.len()..];
        // `Label :` and `Label:` are the same label; `Label odsetek:` is a different one.
        let colon = rest.trim_start().strip_prefix(':')?;
        colon
            .trim_start()
            .starts_with("</strong>")
            .then(|| html.len() - colon.len())
    })?;
    let row = &html[label_end..];
    let row = &row[..row.find("</li>").unwrap_or(row.len())];
    let value = &row[row.find("list-value\"")?..];
    Some(html_text(&value[value.find('>')? + 1..]))
}

/// First decimal number in `s`, accepting a comma: `"100,00 zł"` gives 100.00.
fn first_number(s: &str) -> Option<Decimal> {
    let start = s.find(|c: char| c.is_ascii_digit())?;
    let number: String = s[start..]
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == ',' || *c == '.')
        .collect();
    Decimal::from_str(
        number
            .trim_end_matches([',', '.'])
            .replace(',', ".")
            .as_str(),
    )
    .ok()
}

/// First number that is followed by `%`: `"w pierwszym 12-miesięcznym okresie 5,35%"` gives 5.35,
/// not 12.
fn first_percent(s: &str) -> Option<Decimal> {
    let mut rest = s;
    while let Some(start) = rest.find(|c: char| c.is_ascii_digit()) {
        let tail = &rest[start..];
        let len = tail
            .find(|c: char| !(c.is_ascii_digit() || c == ',' || c == '.'))
            .unwrap_or(tail.len());
        if tail[len..].trim_start().starts_with('%') {
            return first_number(&tail[..len]);
        }
        rest = &tail[len..];
    }
    None
}

fn valid_series_symbol(s: &str) -> bool {
    // `is_ascii` first: the text comes from the network and byte slicing a multibyte character panics.
    s.len() == 7
        && s.is_ascii()
        && s[..3].chars().all(|c| c.is_ascii_uppercase())
        && s[3..].chars().all(|c| c.is_ascii_digit())
}

/// Paths of the series pages linked from the offer overview, e.g.
/// `/oferta-obligacji/obligacje-10-letnie-edo/edo1136/`. Only plain lowercase paths of this
/// shape are returned, so the result can be appended to [`BOND_SITE`] safely.
pub fn bond_series_paths(overview: &str) -> Vec<String> {
    const PREFIX: &str = "href=\"/oferta-obligacji/obligacje-";
    let mut paths: Vec<String> = Vec::new();
    for (i, _) in overview.match_indices(PREFIX) {
        let start = i + "href=\"".len();
        let Some(len) = overview[start..].find('"') else {
            continue;
        };
        let path = &overview[start..start + len];
        let parts: Vec<&str> = path.trim_matches('/').split('/').collect();
        let plain = path
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '/');
        let code = parts.get(2).copied().unwrap_or_default();
        if plain
            && parts.len() == 3
            && code.len() == 7
            && code[..3].chars().all(|c| c.is_ascii_lowercase())
            && code[3..].chars().all(|c| c.is_ascii_digit())
            && !paths.iter().any(|p| p == path)
        {
            paths.push(path.to_string());
        }
    }
    paths
}

/// Parses one series page of obligacjeskarbowe.pl.
pub fn parse_bond_offer(html: &str) -> Result<BondOffer> {
    let missing = |what: &str| provider(format!("obligacjeskarbowe.pl: no {what}"));
    let symbol = offer_field(html, "Seria")
        .and_then(|v| v.split_whitespace().next().map(str::to_string))
        .filter(|s| valid_series_symbol(s))
        .ok_or_else(|| missing("series"))?;
    // A rate outside 0-100 % or an odd price means the page changed; do not present it as the offer.
    let rate = offer_field(html, "Oprocentowanie")
        .and_then(|v| first_percent(&v))
        .filter(|r| *r >= Decimal::ZERO && *r <= Decimal::ONE_HUNDRED)
        .ok_or_else(|| missing("rate"))?;
    let capitalization = match offer_field(html, "Kapitalizacja odsetek")
        .ok_or_else(|| missing("capitalization"))?
        .to_lowercase()
        .as_str()
    {
        "brak" => None,
        "roczna" => Some(Capitalization::Annual),
        "kwartalna" => Some(Capitalization::Quarterly),
        "miesięczna" => Some(Capitalization::Monthly),
        other => {
            return Err(provider(format!(
                "obligacjeskarbowe.pl: unknown capitalization '{other}'"
            )));
        }
    };
    let price = offer_field(html, "Cena sprzedaży jednej obligacji")
        .and_then(|v| first_number(&v))
        .filter(|p| *p > Decimal::ZERO && *p <= Decimal::from(10_000))
        .ok_or_else(|| missing("price"))?;
    Ok(BondOffer {
        series: symbol[..3].to_string(),
        symbol,
        first_period_rate_percent: rate,
        capitalization,
        price,
    })
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
        assert_eq!(
            rates[1].effective_date,
            NaiveDate::from_ymd_opt(2026, 11, 18).unwrap()
        );
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
        assert_eq!(
            yahoo_url(" PKN.WA "),
            "https://query1.finance.yahoo.com/v8/finance/chart/PKN.WA?range=1y&interval=1d&events=div"
        );
        assert!(yahoo_url("A B/C").contains("A%20B%2FC"));
    }

    #[test]
    fn coingecko() {
        let url = coingecko_url(&["Bitcoin", "ethereum", "bitcoin", "x&y"], &["PLN"]);
        assert_eq!(
            url,
            "https://api.coingecko.com/api/v3/simple/price?ids=bitcoin,ethereum,xy&vs_currencies=pln"
        );
        let body = r#"{"bitcoin":{"usd":62763,"pln":252506},"ethereum":{"pln":7061.53}}"#;
        assert_eq!(
            parse_coingecko(body, "bitcoin", "PLN").unwrap(),
            Some(dec!(252506))
        );
        assert_eq!(
            parse_coingecko(body, "ethereum", "pln").unwrap(),
            Some(dec!(7061.53))
        );
        assert_eq!(parse_coingecko(body, "dogecoin", "pln").unwrap(), None);
        let limited =
            r#"{"status":{"error_code":429,"error_message":"You've exceeded the Rate Limit."}}"#;
        assert!(parse_coingecko(limited, "bitcoin", "pln").is_err());
    }

    const NOW: i64 = 1_795_035_600;
    const IN_WINDOW: i64 = NOW - 100 * 86_400;

    /// A `range=1y&interval=1d&events=div` chart: the first bar is `first_age_days` old.
    fn chart(price: &str, currency: &str, first_age_days: i64, closes: &str, divs: &str) -> String {
        let first = NOW - first_age_days * 86_400;
        let events = if divs.is_empty() {
            String::new()
        } else {
            format!(r#","events":{{"dividends":{{{divs}}}}}"#)
        };
        format!(
            r#"{{"chart":{{"result":[{{"meta":{{"currency":"{currency}","regularMarketPrice":{price},
            "regularMarketTime":{NOW}}},"timestamp":[{first},{}],"indicators":{{"quote":[{{"close":[{closes}]}}]}}{events}}}],"error":null}}}}"#,
            first + 86_400
        )
    }

    #[test]
    fn yahoo_return_and_dividend_yield() {
        let body = chart(
            "110",
            "USD",
            365,
            "100,105",
            &format!(
                r#""1":{{"amount":1.0,"date":{IN_WINDOW}}},"2":{{"amount":1.1,"date":{IN_WINDOW}}}"#
            ),
        );
        let q = parse_yahoo(&body).unwrap();
        assert_eq!(q.return_12m_percent, Some(dec!(10)));
        // (1.0 + 1.1) / 110 = 1.909...%
        assert_eq!(q.dividend_yield_percent, Some(dec!(1.91)));
    }

    #[test]
    fn yahoo_skips_missing_first_closes() {
        let body = chart("120", "EUR", 365, "null,100", "");
        let q = parse_yahoo(&body).unwrap();
        assert_eq!(q.return_12m_percent, Some(dec!(20)));
        assert_eq!(q.dividend_yield_percent, None);
    }

    #[test]
    fn yahoo_young_fund_has_no_12_month_return() {
        let body = chart("110", "EUR", 100, "100,105", "");
        let q = parse_yahoo(&body).unwrap();
        assert_eq!(q.return_12m_percent, None);
    }

    #[test]
    fn yahoo_rates_survive_minor_unit_conversion() {
        // London quotes in pence: the ratios must not be divided by 100 a second time.
        let body = chart(
            "11000",
            "GBp",
            365,
            "10000,10500",
            &format!(r#""1":{{"amount":200,"date":{IN_WINDOW}}}"#),
        );
        let q = parse_yahoo(&body).unwrap();
        assert_eq!(q.price, dec!(110));
        assert_eq!(q.return_12m_percent, Some(dec!(10)));
        assert_eq!(q.dividend_yield_percent, Some(dec!(1.82)));
    }

    #[test]
    fn yahoo_without_history_still_gives_a_price() {
        let body = r#"{"chart":{"result":[{"meta":{"currency":"PLN","regularMarketPrice":10}}],"error":null}}"#;
        let q = parse_yahoo(body).unwrap();
        assert_eq!(q.price, dec!(10));
        assert_eq!(
            (q.return_12m_percent, q.dividend_yield_percent),
            (None, None)
        );
    }

    #[test]
    fn coingecko_twelve_month_return() {
        assert_eq!(
            coingecko_markets_url(&["Bitcoin", "ethereum", "bitcoin"], " PLN "),
            "https://api.coingecko.com/api/v3/coins/markets?vs_currency=pln&ids=bitcoin,ethereum&price_change_percentage=1y"
        );
        let body = r#"[{"id":"bitcoin","current_price":256769,"price_change_percentage_1y_in_currency":-30.2925},
            {"id":"ethereum","current_price":7257.73,"price_change_percentage_1y_in_currency":null}]"#;
        assert_eq!(
            parse_coingecko_return(body, "Bitcoin").unwrap(),
            Some(dec!(-30.29))
        );
        assert_eq!(parse_coingecko_return(body, "ethereum").unwrap(), None);
        assert_eq!(parse_coingecko_return(body, "dogecoin").unwrap(), None);
        let limited =
            r#"{"status":{"error_code":429,"error_message":"You've exceeded the Rate Limit."}}"#;
        assert!(parse_coingecko_return(limited, "bitcoin").is_err());
    }

    #[test]
    fn full_year_boundary_is_350_days() {
        let at = |days| parse_yahoo(&chart("110", "EUR", days, "100,105", "")).unwrap();
        assert_eq!(at(350).return_12m_percent, Some(dec!(10)));
        assert_eq!(at(349).return_12m_percent, None);
    }

    #[test]
    fn yahoo_rates_need_a_positive_price_and_first_close() {
        for price in ["0", "-5"] {
            let q = parse_yahoo(&chart(
                price,
                "EUR",
                365,
                "100,105",
                r#""1":{"amount":1,"date":1795000000}"#,
            ))
            .unwrap();
            assert_eq!(
                (q.return_12m_percent, q.dividend_yield_percent),
                (None, None),
                "price {price}"
            );
        }
        let q = parse_yahoo(&chart("110", "EUR", 365, "0,105", "")).unwrap();
        assert_eq!(q.return_12m_percent, None);
    }

    #[test]
    fn yahoo_negative_return_and_odd_dividends() {
        let q = parse_yahoo(&chart("90", "EUR", 365, "100,95", "")).unwrap();
        assert_eq!(q.return_12m_percent, Some(dec!(-10)));
        // Empty events, an entry without an amount and one older than a year add nothing.
        let old = NOW - 400 * 86_400;
        let divs = format!(r#""1":{{"date":{IN_WINDOW}}},"2":{{"amount":5,"date":{old}}}"#);
        assert_eq!(
            parse_yahoo(&chart("90", "EUR", 365, "100,95", &divs))
                .unwrap()
                .dividend_yield_percent,
            None
        );
        assert_eq!(
            parse_yahoo(&chart("90", "EUR", 365, "100,95", ""))
                .unwrap()
                .dividend_yield_percent,
            None
        );
        // A dividend without a date is trusted to belong to the requested range.
        let q = parse_yahoo(&chart("100", "EUR", 365, "100,95", r#""1":{"amount":2}"#)).unwrap();
        assert_eq!(q.dividend_yield_percent, Some(dec!(2)));
    }

    #[test]
    fn hostile_numbers_give_none_instead_of_panicking() {
        // 5e27 / 1e-20 overflows Decimal; both figures must simply be missing.
        let divs = format!(
            r#""1":{{"amount":70000000000000000000000000000,"date":{IN_WINDOW}}},"2":{{"amount":70000000000000000000000000000,"date":{IN_WINDOW}}}"#
        );
        let q = parse_yahoo(&chart(
            "5000000000000000000000000000",
            "EUR",
            365,
            "0.0000000000000000001,1",
            &divs,
        ))
        .unwrap();
        assert_eq!(q.return_12m_percent, None);
        assert_eq!(q.dividend_yield_percent, None);
    }

    #[test]
    fn coingecko_return_rejects_impossible_values() {
        let body =
            |v: &str| format!(r#"[{{"id":"x","price_change_percentage_1y_in_currency":{v}}}]"#);
        assert_eq!(
            parse_coingecko_return(&body("-100"), "x").unwrap(),
            Some(dec!(-100))
        );
        assert_eq!(parse_coingecko_return(&body("-100.5"), "x").unwrap(), None);
        assert_eq!(parse_coingecko_return(&body("2000000"), "x").unwrap(), None);
    }

    /// A series page row in the markup of obligacjeskarbowe.pl.
    fn row(label: &str, value: &str) -> String {
        format!(
            r#"<li>
                <strong class="product-details__list-label">{label}:</strong>
                <span class="product-details__list-value">
                    {value}
                </span>
            </li>"#
        )
    }

    fn series_page(symbol: &str, rate: &str, capitalization: &str, price: &str) -> String {
        format!(
            "<html><ul>{}{}{}{}{}</ul></html>",
            row(
                "Seria",
                &format!(r#"{symbol} <a href="/x.pdf">Zobacz list emisyjny</a>&#160;"#)
            ),
            row("Oprocentowanie", rate),
            row("Kapitalizacja odsetek", capitalization),
            row("Wypłata odsetek", "przy wykupie obligacji"),
            row("Cena sprzedaży jednej obligacji", price),
        )
    }

    #[test]
    fn bond_offer_from_the_official_markup() {
        let edo = series_page(
            "EDO1136",
            "5,35% w pierwszym rocznym okresie odsetkowym, w kolejnych rocznych okresach odsetkowych: marża 2,00% + inflacja",
            "roczna",
            "100,00&nbsp;zł",
        );
        let o = parse_bond_offer(&edo).unwrap();
        assert_eq!(o.symbol, "EDO1136");
        assert_eq!(o.series, "EDO");
        assert_eq!(o.first_period_rate_percent, dec!(5.35));
        assert_eq!(o.capitalization, Some(Capitalization::Annual));
        assert_eq!(o.price, dec!(100));

        let coi = series_page(
            "COI1130",
            "4,75%, w skali roku, w pierwszym rocznym okresie odsetkowym",
            "brak",
            "100,00&nbsp;zł",
        );
        let o = parse_bond_offer(&coi).unwrap();
        assert_eq!(
            (
                o.series.as_str(),
                o.first_period_rate_percent,
                o.capitalization
            ),
            ("COI", dec!(4.75), None)
        );

        let ots = series_page(
            "OTS0227",
            "2,00% w skali roku, stałe przez cały 3-miesięczny okres oszczędzania",
            "brak",
            "100,00&nbsp;zł",
        );
        assert_eq!(
            parse_bond_offer(&ots).unwrap().first_period_rate_percent,
            dec!(2)
        );
    }

    #[test]
    fn bond_offer_rejects_pages_it_does_not_understand() {
        assert!(parse_bond_offer("<html>nothing here</html>").is_err());
        let bad_series = series_page("EDO", "5,35%", "roczna", "100,00 zł");
        assert!(parse_bond_offer(&bad_series).is_err());
        let no_rate = series_page("EDO1136", "w pierwszym okresie", "roczna", "100,00 zł");
        assert!(parse_bond_offer(&no_rate).is_err());
        // An unknown wording must not silently become "no capitalization".
        let odd = series_page("EDO1136", "5,35%", "półroczna", "100,00 zł");
        assert!(
            parse_bond_offer(&odd)
                .unwrap_err()
                .to_string()
                .contains("półroczna")
        );
        let free = series_page("EDO1136", "5,35%", "roczna", "0,00 zł");
        assert!(parse_bond_offer(&free).is_err());
    }

    #[test]
    fn a_multibyte_series_symbol_is_rejected_without_panicking() {
        // 7 bytes, but the 3rd character straddles byte 3: slicing it would panic.
        for symbol in ["AAŁ123", "ŁŁŁ12", "ED\u{307}1136"] {
            let page = series_page(symbol, "5,35%", "roczna", "100,00 zł");
            assert!(parse_bond_offer(&page).is_err(), "{symbol}");
        }
    }

    #[test]
    fn bond_series_paths_are_filtered_and_deduplicated() {
        let overview = r#"
            <a href="/oferta-obligacji/obligacje-10-letnie-edo/edo1136/">EDO</a>
            <a href="/oferta-obligacji/obligacje-10-letnie-edo/edo1136/">again</a>
            <a href="/oferta-obligacji/obligacje-4-letnie-coi/coi1130/">COI</a>
            <a href="/oferta-obligacji/obligacje-roczne-ror/ror1127/">ROR</a>
            <a href="/oferta-obligacji/obligacje-x/../../admin/edo1136/">dotdot</a>
            <a href="/oferta-obligacji/obligacje-y/ED1136/">uppercase</a>
            <a href="/oferta-obligacji/obligacje-z/">no code</a>
            <a href="/oferta-obligacji/obligacje-w/abc1234/extra/">too deep</a>"#;
        assert_eq!(
            bond_series_paths(overview),
            [
                "/oferta-obligacji/obligacje-10-letnie-edo/edo1136/",
                "/oferta-obligacji/obligacje-4-letnie-coi/coi1130/",
                "/oferta-obligacji/obligacje-roczne-ror/ror1127/",
            ]
        );
    }

    #[test]
    fn first_number_handles_commas_and_trailing_punctuation() {
        assert_eq!(first_number("4,75%, w skali roku"), Some(dec!(4.75)));
        assert_eq!(first_number("100,00 zł"), Some(dec!(100.00)));
        assert_eq!(first_number("od 2.5% do 3"), Some(dec!(2.5)));
        assert_eq!(first_number("brak"), None);
    }

    // Real blocks of the live site (captured 2026-11-19), so the markup assumptions are not only mine.
    const REAL_EDO: &str = include_str!("../tests/fixtures/obligacjeskarbowe_edo1136.html");
    const REAL_COI: &str = include_str!("../tests/fixtures/obligacjeskarbowe_coi1130.html");

    #[test]
    fn real_pages_of_the_official_site_parse() {
        let edo = parse_bond_offer(REAL_EDO).unwrap();
        assert_eq!(edo.symbol, "EDO1136");
        assert_eq!(edo.first_period_rate_percent, dec!(5.35));
        assert_eq!(edo.capitalization, Some(Capitalization::Annual));
        assert_eq!(edo.price, dec!(100));
        let coi = parse_bond_offer(REAL_COI).unwrap();
        assert_eq!(coi.symbol, "COI1130");
        assert_eq!(coi.first_period_rate_percent, dec!(4.75));
        assert_eq!(coi.capitalization, None);
    }

    #[test]
    fn every_capitalization_wording_maps_to_its_variant() {
        let cap = |w: &str| {
            parse_bond_offer(&series_page("EDO1136", "5,35%", w, "100,00 zł"))
                .map(|o| o.capitalization)
        };
        assert_eq!(cap("miesięczna").unwrap(), Some(Capitalization::Monthly));
        assert_eq!(cap("kwartalna").unwrap(), Some(Capitalization::Quarterly));
        assert_eq!(cap("roczna").unwrap(), Some(Capitalization::Annual));
        assert_eq!(cap("brak").unwrap(), None);
        // Case and surrounding whitespace / entities do not matter.
        assert_eq!(cap("Miesięczna").unwrap(), Some(Capitalization::Monthly));
        assert_eq!(
            cap("&nbsp; Kwartalna &#160;").unwrap(),
            Some(Capitalization::Quarterly)
        );
    }

    #[test]
    fn rejections_say_what_is_wrong() {
        let err = |html: &str| parse_bond_offer(html).unwrap_err().to_string();
        assert!(err("<html>nothing</html>").contains("no series"));
        assert!(err(&series_page("EDO", "5,35%", "roczna", "100 zł")).contains("no series"));
        assert!(
            err(&series_page(
                "EDO1136",
                "w pierwszym okresie",
                "roczna",
                "100 zł"
            ))
            .contains("no rate")
        );
        assert!(err(&series_page("EDO1136", "5,35%", "roczna", "0,00 zł")).contains("no price"));
        assert!(err(&series_page("EDO1136", "5,35%", "półroczna", "100 zł")).contains("półroczna"));
        // Implausible values mean the page changed.
        assert!(err(&series_page("EDO1136", "535%", "roczna", "100 zł")).contains("no rate"));
        assert!(err(&series_page("EDO1136", "5,35%", "roczna", "99999 zł")).contains("no price"));
    }

    #[test]
    fn a_rate_is_the_number_before_the_percent_sign() {
        assert_eq!(first_percent("4,75%, w skali roku"), Some(dec!(4.75)));
        assert_eq!(
            first_percent("w pierwszym 12-miesięcznym okresie 5,35%"),
            Some(dec!(5.35))
        );
        assert_eq!(first_percent("5,35 %"), Some(dec!(5.35)));
        assert_eq!(first_percent("marża 2,00% + inflacja"), Some(dec!(2.00)));
        assert_eq!(first_percent("12 miesięcy"), None);
        assert_eq!(first_percent(""), None);
        assert_eq!(
            first_number("4,75, w").map(|d| d.to_string()),
            Some("4.75".into())
        );
        assert_eq!(
            first_number("100. zł").map(|d| d.to_string()),
            Some("100".into())
        );
        assert_eq!(
            first_number("100,00 zł").map(|d| d.to_string()),
            Some("100.00".into())
        );
    }

    #[test]
    fn the_parsed_rate_is_the_percentage_not_the_first_number() {
        let page = series_page(
            "EDO1136",
            "w pierwszym 12-miesięcznym okresie 5,35%",
            "roczna",
            "100 zł",
        );
        assert_eq!(
            parse_bond_offer(&page).unwrap().first_period_rate_percent,
            dec!(5.35)
        );
    }

    #[test]
    fn a_row_never_borrows_the_value_of_another_row() {
        let row_with_value = |label: &str, value: &str| row(label, value);
        // "Seria" has a label but no value element; the next row's value must not leak into it.
        let html = format!(
            r#"<li><strong class="product-details__list-label">Seria:</strong></li>{}"#,
            row_with_value("Oprocentowanie", "5,35%")
        );
        assert_eq!(offer_field(&html, "Seria"), None);
        assert_eq!(
            offer_field(&html, "Oprocentowanie").as_deref(),
            Some("5,35%")
        );
        // A nested element inside the value does not cut it short.
        let nested = row(
            "Cena sprzedaży jednej obligacji",
            "<span>100,00</span>&nbsp;zł",
        );
        assert_eq!(
            offer_field(&nested, "Cena sprzedaży jednej obligacji").as_deref(),
            Some("100,00 zł")
        );
        // `Label :` is the same label, `Kapitalizacja odsetek:` is not `Kapitalizacja`.
        let spaced = r#"<li><strong class="product-details__list-label">Seria :</strong><span class="product-details__list-value">EDO1136</span></li>"#;
        assert_eq!(offer_field(spaced, "Seria").as_deref(), Some("EDO1136"));
        assert_eq!(
            offer_field(&row("Kapitalizacja odsetek", "roczna"), "Kapitalizacja"),
            None
        );
        // The first row with a label wins (documented; a sidebar would come later on the page).
        let twice = format!("{}{}", row("Seria", "EDO1136"), row("Seria", "COI1130"));
        assert_eq!(offer_field(&twice, "Seria").as_deref(), Some("EDO1136"));
        // Truncated markup must not panic.
        assert_eq!(offer_field(r#"<strong class="product-details__list-label">Seria:</strong><span class="product-details__list-value">EDO"#, "Seria").as_deref(), Some("EDO"));
        assert_eq!(offer_field("", "Seria"), None);
    }

    #[test]
    fn a_page_without_series_links_gives_no_paths() {
        assert!(bond_series_paths("<html>cookie wall</html>").is_empty());
        assert!(bond_series_paths(r#"<a href="/oferta-obligacji/obligacje-x/edo1136"#).is_empty());
        assert!(
            bond_series_paths(r#"<a href='/oferta-obligacji/obligacje-x/y/edo1136/'>"#).is_empty()
        );
    }
}
