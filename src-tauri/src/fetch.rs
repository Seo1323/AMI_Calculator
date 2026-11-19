//! Fetches live prices (Yahoo Finance, CoinGecko) and PLN exchange rates (NBP).
//!
//! The database lock is never held across a network call: everything is
//! downloaded first, then written in one go.

use std::collections::HashMap;
use std::time::Duration;

use ami_core::model::{FxRate, Instrument, Quote, QuoteSource};
use ami_core::providers;
use ami_core::store::Store;
use chrono::Utc;
use futures::future::join_all;
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

async fn get(client: &reqwest::Client, url: &str) -> Result<String, String> {
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| e.without_url().to_string())?;
    let status = resp.status();
    let body = resp.text().await.map_err(|e| e.without_url().to_string())?;
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

pub struct Downloaded {
    fx: Vec<FxRate>,
    quotes: Vec<Quote>,
    failures: Vec<Failure>,
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
    let coingecko_returns = async {
        let mut currencies: Vec<&str> = plan.coingecko.iter().map(|i| i.currency.code()).collect();
        currencies.sort_unstable();
        currencies.dedup();
        join_all(currencies.into_iter().map(|cur| {
            let client = &client;
            let ids: Vec<&str> = plan
                .coingecko
                .iter()
                .filter(|i| i.currency.code() == cur)
                .map(|i| i.symbol.as_str())
                .collect();
            async move {
                // A failure here only means "no 12-month figure"; the price is fetched separately.
                let body = get(client, &providers::coingecko_markets_url(&ids, cur)).await;
                (cur.to_string(), body.unwrap_or_default())
            }
        }))
        .await
        .into_iter()
        .collect::<HashMap<String, String>>()
    };
    let (nbp, yahoo, coingecko, coingecko_returns) =
        futures::join!(nbp, yahoo, coingecko, coingecko_returns);

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
    let mut seen: HashMap<uuid::Uuid, ()> = HashMap::new();
    for q in &data.quotes {
        // The instrument may have been deleted while we were downloading.
        if seen.insert(q.instrument_id, ()).is_none() && store.set_quote(q).is_ok() {
            report.quotes_updated += 1;
        }
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ami_core::model::{InstrumentKind, Meta};
    use ami_core::money::Currency;

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
            maturity_date: None,
        }
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
