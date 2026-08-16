use crate::config::cache;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;

const TTL: Duration = Duration::from_secs(6 * 3600);

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FxRate {
    pub pair: String, // "USDT/CZK"
    pub rate: f64,
    pub as_of: String, // human-readable, e.g. "08.08 09:00"
}

#[derive(Deserialize)]
struct ExchangerateHostResponse {
    rates: RatesField,
}

#[derive(Deserialize)]
struct RatesField {
    #[serde(rename = "CZK")]
    czk: f64,
}

pub struct FxProvider {
    client: reqwest::Client,
    cache_path: PathBuf,
}

impl FxProvider {
    pub fn new(cache_dir: PathBuf) -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(8))
                .build()
                .expect("reqwest client"),
            cache_path: cache_dir.join("fx.json"),
        }
    }

    /// Prices in the widget are quoted in USDT, so the fiat column converts USDT → CZK. No FX
    /// desk quotes USDT, and the peg holds to a fraction of a percent, so the rate itself is
    /// still fetched as USD → CZK — only the label says what it is actually applied to.
    pub async fn usdt_to_czk(&self) -> Option<FxRate> {
        if let Some(cached) = cache::read_if_fresh::<FxRate>(&self.cache_path, TTL) {
            return Some(cached);
        }
        match self.fetch_primary().await {
            Some(rate) => {
                cache::write(&self.cache_path, &rate);
                Some(rate)
            }
            None => match self.fetch_fallback().await {
                Some(rate) => {
                    cache::write(&self.cache_path, &rate);
                    Some(rate)
                }
                None => cache::read_stale::<FxRate>(&self.cache_path),
            },
        }
    }

    async fn fetch_primary(&self) -> Option<FxRate> {
        let url = "https://api.exchangerate.host/latest?base=USD&symbols=CZK";
        let resp = self.client.get(url).send().await.ok()?.error_for_status().ok()?;
        let parsed: ExchangerateHostResponse = resp.json().await.ok()?;
        Some(FxRate {
            pair: "USDT/CZK".into(),
            rate: parsed.rates.czk,
            as_of: now_label(),
        })
    }

    /// Czech National Bank daily rate list as a fallback: plain-text, one currency per line,
    /// format "country|currency|amount|code|rate".
    async fn fetch_fallback(&self) -> Option<FxRate> {
        let url = "https://www.cnb.cz/en/financial_markets/foreign_exchange_market/exchange_rate_fixing/daily.txt";
        let resp = self.client.get(url).send().await.ok()?.error_for_status().ok()?;
        let text = resp.text().await.ok()?;
        for line in text.lines() {
            let cols: Vec<&str> = line.split('|').collect();
            if cols.len() == 5 && cols[3].trim() == "USD" {
                let amount: f64 = cols[2].trim().parse().ok()?;
                let czk_per_usd: f64 = cols[4].trim().replace(',', ".").parse().ok()?;
                return Some(FxRate {
                    pair: "USDT/CZK".into(),
                    rate: czk_per_usd / amount,
                    as_of: now_label(),
                });
            }
        }
        None
    }
}

fn now_label() -> String {
    chrono::Local::now().format("%d.%m %H:%M").to_string()
}
