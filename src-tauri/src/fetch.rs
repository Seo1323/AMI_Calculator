//! Fetches live prices (Yahoo Finance, CoinGecko) and PLN exchange rates (NBP).
//!
//! The database lock is never held across a network call: everything is
//! downloaded first, then written in one go.

use std::collections::{BTreeMap, HashMap};
use std::time::Duration;

use ami_core::model::{FxRate, Instrument, Quote, QuoteSource};
use ami_core::money::Currency;
use ami_core::providers;
use ami_core::store::Store;
use chrono::Utc;
use futures::future::join_all;
use rust_decimal::Decimal;
use serde::Serialize;

const USER_AGENT: &str = concat!("AMI_Calculator/", env!("CARGO_PKG_VERSION"));

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Failure {
    pub symbol: String,
    pub reason: String,
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RefreshReport {
    pub fx_updated: usize,
    pub quotes_updated: usize,
    pub failures: Vec<Failure>,
}

fn client() -> reqwest::Result<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(Duration::from_secs(15))
        .build()
}

/// Largest response body accepted; real pages and API answers are tens of kilobytes.
const MAX_BODY: usize = 4 * 1024 * 1024;

async fn get(client: &reqwest::Client, url: &str) -> Result<String, String> {
    let mut resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| e.without_url().to_string())?;
    let status = resp.status();
    let too_large = || "response too large".to_string();
    if resp.content_length().is_some_and(|n| n > MAX_BODY as u64) {
        return Err(too_large());
    }
    let mut body: Vec<u8> = Vec::new();
    while let Some(chunk) = resp
        .chunk()
        .await
        .map_err(|e| e.without_url().to_string())?
    {
        if body.len() + chunk.len() > MAX_BODY {
            return Err(too_large());
        }
        body.extend_from_slice(&chunk);
    }
    let body = String::from_utf8_lossy(&body).into_owned();
    // Yahoo answers 404 with a JSON error body worth parsing; other errors are just HTTP.
    if status.is_success() || status == reqwest::StatusCode::NOT_FOUND {
        Ok(body)
    } else {
        Err(format!("HTTP {status}"))
    }
}

/// Snapshot of what needs fetching, taken under the lock.
pub struct Plan {
    yahoo: Vec<Instrument>,
    coingecko: Vec<Instrument>,
}

pub fn plan(store: &Store) -> ami_core::Result<Plan> {
    let instruments = store.list::<Instrument>()?;
    Ok(Plan {
        yahoo: instruments
            .iter()
            .filter(|i| i.quote_source == QuoteSource::Yahoo)
            .cloned()
            .collect(),
        coingecko: instruments
            .iter()
            .filter(|i| i.quote_source == QuoteSource::CoinGecko)
            .cloned()
            .collect(),
    })
}

/// Coin ids per quote currency: the markets endpoint takes one `vs_currency` per request.
fn coingecko_groups(instruments: &[Instrument]) -> BTreeMap<String, Vec<String>> {
    let mut groups: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for i in instruments {
        groups
            .entry(i.currency.code().to_string())
            .or_default()
            .push(i.symbol.clone());
    }
    groups
}

/// Response body of `coins/markets` per currency; a failed request is an empty body, which
/// parses to "no figure" (best effort, the price is fetched separately).
async fn fetch_coingecko_returns(
    client: &reqwest::Client,
    instruments: &[Instrument],
) -> HashMap<String, String> {
    join_all(
        coingecko_groups(instruments)
            .into_iter()
            .map(|(cur, ids)| async move {
                let ids: Vec<&str> = ids.iter().map(String::as_str).collect();
                let body = get(client, &providers::coingecko_markets_url(&ids, &cur)).await;
                (cur, body.unwrap_or_default())
            }),
    )
    .await
    .into_iter()
    .collect()
}

/// Client for the bond pages: redirects are followed only within the same host (and at most
/// three times), so the page cannot send the app to another site.
fn bond_client() -> reqwest::Result<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            let same_host =
                attempt.previous().first().and_then(|u| u.host_str()) == attempt.url().host_str();
            if same_host && attempt.previous().len() < 3 {
                attempt.follow()
            } else {
                attempt.stop()
            }
        }))
        .build()
}

/// Symbol (`EDO1136`) from a series page path (`/oferta-obligacji/obligacje-10-letnie-edo/edo1136/`).
fn symbol_of_path(path: &str) -> String {
    path.trim_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or_default()
        .to_uppercase()
}

/// Pure part of the bond download: every page is either parsed into an offer or reported as
/// skipped. Series that cannot be read never vanish silently; only if none can be read it is an
/// error, and it carries the first reason.
fn offers_from_pages(
    pages: Vec<(String, Result<String, String>)>,
) -> Result<providers::BondOffers, String> {
    let mut out = providers::BondOffers::default();
    let mut first_error: Option<String> = None;
    for (symbol, page) in pages {
        match page.and_then(|body| providers::parse_bond_offer(&body).map_err(|e| e.to_string())) {
            Ok(offer) => out.offers.push(offer),
            Err(reason) => {
                first_error.get_or_insert(format!("{symbol}: {reason}"));
                out.skipped.push(symbol);
            }
        }
    }
    if out.offers.is_empty() {
        Err(format!(
            "obligacjeskarbowe.pl: could not read any series ({})",
            first_error.unwrap_or_else(|| "no series".into())
        ))
    } else {
        Ok(out)
    }
}

/// Series pages fetched per refresh; more than this is reported as skipped.
const MAX_SERIES: usize = 16;

/// The current monthly offer of Polish retail treasury bonds (public page of the Ministry of Finance).
pub async fn bond_offers() -> Result<providers::BondOffers, String> {
    let client = bond_client().map_err(|e| e.to_string())?;
    let overview = get(&client, providers::BOND_OFFER_URL).await?;
    let paths = providers::bond_series_paths(&overview);
    if paths.is_empty() {
        return Err("obligacjeskarbowe.pl: no series found".into());
    }
    let (fetch, extra) = paths.split_at(paths.len().min(MAX_SERIES));
    let bodies = join_all(fetch.iter().map(|path| {
        let client = &client;
        async move { get(client, &format!("{}{path}", providers::BOND_SITE)).await }
    }))
    .await;
    let mut result = offers_from_pages(
        fetch
            .iter()
            .map(|p| symbol_of_path(p))
            .zip(bodies)
            .collect(),
    )?;
    result
        .skipped
        .extend(extra.iter().map(|p| symbol_of_path(p)));
    Ok(result)
}

pub struct Downloaded {
    fx: Vec<FxRate>,
    quotes: Vec<Quote>,
    failures: Vec<Failure>,
}

/// Current price of one instrument, for prefilling a holding before it is saved.
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct QuotePreview {
    pub price: Decimal,
    /// Currency of the price: Yahoo's listing currency, or the one asked for from CoinGecko.
    pub currency: Currency,
    pub source: String,
}

/// Pure part of [`quote_preview`]: reads the price out of a provider response.
fn preview_from_body(
    source: QuoteSource,
    body: &str,
    symbol: &str,
    currency: &Currency,
) -> Result<QuotePreview, String> {
    match source {
        QuoteSource::Yahoo => {
            let q = providers::parse_yahoo(body).map_err(|e| e.to_string())?;
            Ok(QuotePreview {
                price: q.price,
                currency: q.currency,
                source: "yahoo".into(),
            })
        }
        QuoteSource::CoinGecko => providers::parse_coingecko(body, symbol, currency.code())
            .map_err(|e| e.to_string())?
            .map(|price| QuotePreview {
                price,
                currency: currency.clone(),
                source: "coingecko".into(),
            })
            .ok_or_else(|| "CoinGecko: unknown id".to_string()),
        QuoteSource::Manual => Err("no online source for a manually priced instrument".into()),
    }
}

/// Fetches the current price of a single instrument (no database access).
pub async fn quote_preview(
    source: QuoteSource,
    symbol: &str,
    currency: &Currency,
) -> Result<QuotePreview, String> {
    let url = match source {
        QuoteSource::Yahoo => providers::yahoo_url(symbol),
        QuoteSource::CoinGecko => providers::coingecko_url(&[symbol], &[currency.code()]),
        QuoteSource::Manual => {
            return Err("no online source for a manually priced instrument".into());
        }
    };
    let client = client().map_err(|e| e.to_string())?;
    let body = get(&client, &url).await?;
    preview_from_body(source, &body, symbol, currency)
}

pub async fn download(plan: Plan) -> Downloaded {
    let mut out = Downloaded {
        fx: Vec::new(),
        quotes: Vec::new(),
        failures: Vec::new(),
    };
    let client = match client() {
        Ok(c) => c,
        Err(e) => {
            out.failures.push(Failure {
                symbol: "HTTP".into(),
                reason: e.to_string(),
            });
            return out;
        }
    };
    let now = Utc::now();

    let nbp = async {
        let body = get(&client, providers::NBP_TABLE_A_URL).await?;
        providers::parse_nbp_table(&body).map_err(|e| e.to_string())
    };
    let yahoo = join_all(plan.yahoo.iter().map(|i| {
        let client = &client;
        async move {
            let body = get(client, &providers::yahoo_url(&i.symbol)).await?;
            providers::parse_yahoo(&body).map_err(|e| e.to_string())
        }
    }));
    let coingecko = async {
        if plan.coingecko.is_empty() {
            return Ok(String::new());
        }
        let ids: Vec<&str> = plan.coingecko.iter().map(|i| i.symbol.as_str()).collect();
        let vs: Vec<&str> = plan.coingecko.iter().map(|i| i.currency.code()).collect();
        get(&client, &providers::coingecko_url(&ids, &vs)).await
    };
    let (nbp, yahoo, coingecko) = futures::join!(nbp, yahoo, coingecko);
    // Only after the price call succeeded: CoinGecko's free tier rate-limits hard, and an
    // optional 12-month figure must never be the reason a price is missing.
    let coingecko_returns = if matches!(&coingecko, Ok(body) if !body.is_empty()) {
        fetch_coingecko_returns(&client, &plan.coingecko).await
    } else {
        HashMap::new()
    };

    match nbp {
        Ok(rates) => {
            out.fx = rates
                .into_iter()
                .map(|r| FxRate {
                    currency: r.currency,
                    rate_pln: r.mid,
                    source: "nbp".into(),
                    fetched_at: now,
                })
                .collect()
        }
        Err(reason) => out.failures.push(Failure {
            symbol: "NBP".into(),
            reason,
        }),
    }

    for (instrument, result) in plan.yahoo.iter().zip(yahoo) {
        match result {
            Ok(q) => out.quotes.push(Quote {
                instrument_id: instrument.meta.id,
                price: q.price,
                currency: q.currency,
                source: "yahoo".into(),
                fetched_at: now,
                return_12m_percent: q.return_12m_percent,
                dividend_yield_percent: q.dividend_yield_percent,
            }),
            Err(reason) => out.failures.push(Failure {
                symbol: instrument.symbol.clone(),
                reason,
            }),
        }
    }

    match coingecko {
        Ok(body) if !body.is_empty() => {
            for i in &plan.coingecko {
                match providers::parse_coingecko(&body, &i.symbol, i.currency.code()) {
                    Ok(Some(price)) => out.quotes.push(Quote {
                        instrument_id: i.meta.id,
                        price,
                        currency: i.currency.clone(),
                        source: "coingecko".into(),
                        fetched_at: now,
                        return_12m_percent: coingecko_returns.get(i.currency.code()).and_then(
                            |b| {
                                providers::parse_coingecko_return(b, &i.symbol)
                                    .ok()
                                    .flatten()
                            },
                        ),
                        dividend_yield_percent: None,
                    }),
                    Ok(None) => out.failures.push(Failure {
                        symbol: i.symbol.clone(),
                        reason: "CoinGecko: unknown id".into(),
                    }),
                    Err(e) => out.failures.push(Failure {
                        symbol: i.symbol.clone(),
                        reason: e.to_string(),
                    }),
                }
            }
        }
        Ok(_) => {}
        Err(reason) => {
            for i in &plan.coingecko {
                out.failures.push(Failure {
                    symbol: i.symbol.clone(),
                    reason: reason.clone(),
                });
            }
        }
    }
    out
}

pub fn save(store: &Store, data: Downloaded) -> ami_core::Result<RefreshReport> {
    let mut report = RefreshReport {
        failures: data.failures,
        ..Default::default()
    };
    for r in &data.fx {
        store.set_fx_rate(r)?;
        report.fx_updated += 1;
    }
    let previous: HashMap<uuid::Uuid, Quote> = store
        .quotes()?
        .into_iter()
        .map(|q| (q.instrument_id, q))
        .collect();
    let mut seen = std::collections::HashSet::new();
    for mut q in data.quotes {
        let known = previous.get(&q.instrument_id);
        keep_known_figures(&mut q, known);
        // The instrument may have been deleted while we were downloading.
        if seen.insert(q.instrument_id) && store.set_quote(&q).is_ok() {
            report.quotes_updated += 1;
        }
    }
    Ok(report)
}

/// The 12-month figures are optional extras that a provider sometimes leaves out (rate limit, short
/// history): a fresh price without them must not erase the ones we already know.
fn keep_known_figures(q: &mut Quote, previous: Option<&Quote>) {
    let Some(prev) = previous else { return };
    q.return_12m_percent = q.return_12m_percent.or(prev.return_12m_percent);
    q.dividend_yield_percent = q.dividend_yield_percent.or(prev.dividend_yield_percent);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ami_core::model::{InstrumentKind, Meta};
    use ami_core::money::Currency;

    /// Serves canned HTTP responses on localhost, chosen by request path; returns the base URL.
    async fn serve(routes: Vec<(&'static str, Vec<u8>)>) -> String {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move {
            loop {
                let Ok((mut sock, _)) = listener.accept().await else {
                    return;
                };
                let mut buf = vec![0u8; 4096];
                let n = sock.read(&mut buf).await.unwrap_or(0);
                let head = String::from_utf8_lossy(&buf[..n]).into_owned();
                let path = head.split_whitespace().nth(1).unwrap_or("/");
                let reply = routes
                    .iter()
                    .find(|(p, _)| *p == path)
                    .map(|(_, r)| r.clone());
                let _ = sock.write_all(&reply.unwrap_or_default()).await;
            }
        });
        base
    }

    fn http(status: &str, extra: &str, body: &[u8]) -> Vec<u8> {
        let mut r = format!(
            "HTTP/1.1 {status}\r\n{extra}Content-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        )
        .into_bytes();
        r.extend_from_slice(body);
        r
    }

    #[tokio::test]
    async fn get_returns_the_body_and_maps_http_errors() {
        let base = serve(vec![
            ("/ok", http("200 OK", "", b"hello")),
            ("/missing", http("404 Not Found", "", b"{\"error\":1}")),
            ("/boom", http("500 Internal Server Error", "", b"oops")),
        ])
        .await;
        let c = client().unwrap();
        assert_eq!(get(&c, &format!("{base}/ok")).await.unwrap(), "hello");
        // Yahoo's 404 carries a JSON error worth parsing.
        assert_eq!(
            get(&c, &format!("{base}/missing")).await.unwrap(),
            "{\"error\":1}"
        );
        assert!(
            get(&c, &format!("{base}/boom"))
                .await
                .unwrap_err()
                .contains("500")
        );
    }

    #[tokio::test]
    async fn get_refuses_a_response_over_the_size_limit() {
        let big = vec![b'x'; MAX_BODY + 1];
        let base = serve(vec![("/big", http("200 OK", "", &big))]).await;
        let err = get(&client().unwrap(), &format!("{base}/big"))
            .await
            .unwrap_err();
        assert_eq!(err, "response too large");
    }

    #[tokio::test]
    async fn bond_client_follows_redirects_on_the_same_host_only() {
        let base = serve(vec![
            ("/start", http("302 Found", "Location: /final\r\n", b"")),
            ("/final", http("200 OK", "", b"arrived")),
            // Same server, but reached through another host name.
            (
                "/away",
                http("302 Found", "Location: http://localhost:1/x\r\n", b""),
            ),
        ])
        .await;
        let c = bond_client().unwrap();
        assert_eq!(get(&c, &format!("{base}/start")).await.unwrap(), "arrived");
        let err = get(&c, &format!("{base}/away")).await.unwrap_err();
        assert!(err.contains("302"), "{err}");
    }

    #[test]
    fn a_fresh_price_without_figures_keeps_the_known_ones() {
        use rust_decimal_macros::dec;
        let quote = |price, ret, div| Quote {
            instrument_id: uuid::Uuid::now_v7(),
            price,
            currency: Currency::new("PLN").unwrap(),
            source: "yahoo".into(),
            fetched_at: Utc::now(),
            return_12m_percent: ret,
            dividend_yield_percent: div,
        };
        let old = quote(dec!(100), Some(dec!(12.5)), Some(dec!(2)));
        let mut fresh = quote(dec!(101), None, None);
        keep_known_figures(&mut fresh, Some(&old));
        assert_eq!(fresh.price, dec!(101));
        assert_eq!(fresh.return_12m_percent, Some(dec!(12.5)));
        assert_eq!(fresh.dividend_yield_percent, Some(dec!(2)));
        // New figures win over old ones.
        let mut newer = quote(dec!(101), Some(dec!(3)), None);
        keep_known_figures(&mut newer, Some(&old));
        assert_eq!(newer.return_12m_percent, Some(dec!(3)));
    }

    fn instrument(symbol: &str, source: QuoteSource, currency: &str) -> Instrument {
        Instrument {
            meta: Meta::new(),
            symbol: symbol.into(),
            name: symbol.into(),
            kind: InstrumentKind::Etf,
            currency: Currency::new(currency).unwrap(),
            quote_source: source,
            fractional: false,
            interest_percent: None,
            capitalization: None,
            start_date: None,
            maturity_date: None,
        }
    }

    #[test]
    fn coingecko_ids_are_grouped_by_quote_currency() {
        let list = [
            instrument("bitcoin", QuoteSource::CoinGecko, "PLN"),
            instrument("ethereum", QuoteSource::CoinGecko, "PLN"),
            instrument("solana", QuoteSource::CoinGecko, "USD"),
        ];
        let groups = coingecko_groups(&list);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups["PLN"], ["bitcoin", "ethereum"]);
        assert_eq!(groups["USD"], ["solana"]);
        assert!(coingecko_groups(&[]).is_empty());
    }

    const REAL_EDO: &str =
        include_str!("../../crates/ami-core/tests/fixtures/obligacjeskarbowe_edo1136.html");
    const REAL_COI: &str =
        include_str!("../../crates/ami-core/tests/fixtures/obligacjeskarbowe_coi1130.html");

    #[test]
    fn unreadable_series_are_reported_not_dropped() {
        let pages = vec![
            ("EDO1136".to_string(), Ok(REAL_EDO.to_string())),
            ("COI1130".to_string(), Err("HTTP 500".to_string())),
            (
                "ROR1127".to_string(),
                Ok("<html>cookie wall</html>".to_string()),
            ),
            (
                "TOS1129".to_string(),
                Ok(REAL_COI.replace("brak", "półroczna")),
            ),
        ];
        let out = offers_from_pages(pages).unwrap();
        assert_eq!(out.offers.len(), 1);
        assert_eq!(out.offers[0].symbol, "EDO1136");
        assert_eq!(out.skipped, ["COI1130", "ROR1127", "TOS1129"]);
    }

    #[test]
    fn no_readable_series_is_an_error_with_the_first_reason() {
        let pages = vec![
            ("COI1130".to_string(), Err("HTTP 500".to_string())),
            ("ROR1127".to_string(), Ok("<html></html>".to_string())),
        ];
        let err = offers_from_pages(pages).unwrap_err();
        assert!(err.contains("COI1130: HTTP 500"), "{err}");
        assert!(offers_from_pages(vec![]).unwrap_err().contains("no series"));
    }

    #[test]
    fn a_preview_reads_yahoo_and_coingecko_bodies() {
        let pln = Currency::new("PLN").unwrap();
        let yahoo = r#"{"chart":{"result":[{"meta":{"currency":"EUR","regularMarketPrice":166.88}}],"error":null}}"#;
        let p = preview_from_body(QuoteSource::Yahoo, yahoo, "VWCE.DE", &pln).unwrap();
        // The price keeps the listing currency Yahoo reports, not the one asked for.
        assert_eq!(
            (p.price.to_string(), p.currency.code(), p.source.as_str()),
            ("166.88".into(), "EUR", "yahoo")
        );

        let pence = r#"{"chart":{"result":[{"meta":{"currency":"GBp","regularMarketPrice":8123.5}}],"error":null}}"#;
        let p = preview_from_body(QuoteSource::Yahoo, pence, "X.L", &pln).unwrap();
        assert_eq!(
            (p.price.to_string(), p.currency.code()),
            ("81.235".into(), "GBP")
        );

        let gecko = r#"{"bitcoin":{"pln":256088}}"#;
        let p = preview_from_body(QuoteSource::CoinGecko, gecko, "bitcoin", &pln).unwrap();
        assert_eq!(
            (p.price.to_string(), p.currency.code(), p.source.as_str()),
            ("256088".into(), "PLN", "coingecko")
        );
    }

    #[test]
    fn a_preview_fails_clearly_when_there_is_no_price() {
        let pln = Currency::new("PLN").unwrap();
        let missing = r#"{"chart":{"result":null,"error":{"code":"Not Found","description":"No data found"}}}"#;
        assert!(
            preview_from_body(QuoteSource::Yahoo, missing, "NOPE", &pln)
                .unwrap_err()
                .contains("No data found")
        );
        assert!(
            preview_from_body(QuoteSource::CoinGecko, "{}", "nope", &pln)
                .unwrap_err()
                .contains("unknown id")
        );
        let limited = r#"{"status":{"error_message":"rate limit"}}"#;
        assert!(
            preview_from_body(QuoteSource::CoinGecko, limited, "bitcoin", &pln)
                .unwrap_err()
                .contains("rate limit")
        );
        assert!(preview_from_body(QuoteSource::Manual, "{}", "x", &pln).is_err());
        assert!(preview_from_body(QuoteSource::Yahoo, "<html>", "x", &pln).is_err());
    }

    #[test]
    fn symbols_come_from_the_page_path() {
        assert_eq!(
            symbol_of_path("/oferta-obligacji/obligacje-10-letnie-edo/edo1136/"),
            "EDO1136"
        );
        assert_eq!(symbol_of_path(""), "");
    }

    /// Reads real prices: `cargo test -p ami-calculator -- --ignored live_preview --nocapture`
    #[tokio::test]
    #[ignore = "network: Yahoo Finance, CoinGecko"]
    async fn live_preview() {
        let pln = Currency::new("PLN").unwrap();
        let etf = quote_preview(QuoteSource::Yahoo, "VWCE.DE", &pln)
            .await
            .unwrap();
        let stock = quote_preview(QuoteSource::Yahoo, "PKN.WA", &pln)
            .await
            .unwrap();
        let coin = quote_preview(QuoteSource::CoinGecko, "bitcoin", &pln)
            .await
            .unwrap();
        println!(
            "VWCE.DE {} {} | PKN.WA {} {} | bitcoin {} {}",
            etf.price, etf.currency, stock.price, stock.currency, coin.price, coin.currency
        );
        assert!(
            etf.price > Decimal::ZERO && stock.price > Decimal::ZERO && coin.price > Decimal::ZERO
        );
        assert!(
            quote_preview(QuoteSource::Yahoo, "NOPE123.WA", &pln)
                .await
                .is_err()
        );
    }

    /// Reads the real bond offer: `cargo test -p ami-calculator -- --ignored live_bond --nocapture`
    #[tokio::test]
    #[ignore = "network: obligacjeskarbowe.pl"]
    async fn live_bond_offers() {
        let set = bond_offers().await.unwrap();
        for o in &set.offers {
            println!(
                "{} {} rate={} cap={:?} price={}",
                o.series, o.symbol, o.first_period_rate_percent, o.capitalization, o.price
            );
            assert_eq!(o.price, rust_decimal::Decimal::ONE_HUNDRED);
            assert!(
                o.first_period_rate_percent > rust_decimal::Decimal::ZERO
                    && o.first_period_rate_percent < rust_decimal::Decimal::from(20)
            );
            assert_eq!(o.symbol.len(), 7);
        }
        println!("skipped: {:?}", set.skipped);
        assert!(
            set.offers.len() >= 6,
            "expected most series, got {}",
            set.offers.len()
        );
    }

    /// Hits the real services: `cargo test -p ami-calculator -- --ignored live`
    #[tokio::test]
    #[ignore]
    async fn live_download() {
        let plan = Plan {
            yahoo: vec![
                instrument("PKN.WA", QuoteSource::Yahoo, "PLN"),
                instrument("VWCE.DE", QuoteSource::Yahoo, "EUR"),
                instrument("NOPE123.WA", QuoteSource::Yahoo, "PLN"),
            ],
            coingecko: vec![instrument("bitcoin", QuoteSource::CoinGecko, "PLN")],
        };
        let d = download(plan).await;
        for q in &d.quotes {
            println!(
                "quote {} {} {} 12m={:?} div={:?}",
                q.source, q.price, q.currency, q.return_12m_percent, q.dividend_yield_percent
            );
        }
        for f in &d.failures {
            println!("failure {}: {}", f.symbol, f.reason);
        }
        println!(
            "fx rates: {} (EUR = {:?})",
            d.fx.len(),
            d.fx.iter()
                .find(|r| r.currency.code() == "EUR")
                .map(|r| r.rate_pln)
        );
        assert!(d.fx.len() > 20);
        assert_eq!(d.quotes.len(), 3);
        assert_eq!(d.failures.len(), 1);
    }
}
