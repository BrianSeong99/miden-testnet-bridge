use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

use async_trait::async_trait;
use reqwest::Url;
use rust_decimal::Decimal;
use serde::Deserialize;
use tokio::sync::Mutex;

pub type DynPricer = Arc<dyn Pricer>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PriceQuote {
    pub input_usd: String,
    pub output_usd: String,
    pub output_amount: String,
}

#[derive(Debug, thiserror::Error)]
pub enum PricerError {
    #[error("unsupported asset symbol {0}")]
    UnsupportedAssetSymbol(String),
    #[error("invalid amount {0}")]
    InvalidAmount(String),
    #[error("CoinGecko request failed: {0}")]
    Request(#[from] reqwest::Error),
    #[error("CoinGecko returned no USD price for {0}")]
    MissingPrice(String),
}

#[async_trait]
pub trait Pricer: Send + Sync {
    async fn quote(
        &self,
        in_asset_symbol: &str,
        out_asset_symbol: &str,
        amount: &str,
    ) -> Result<PriceQuote, PricerError>;
}

#[derive(Clone)]
pub struct CoinGeckoPricer {
    client: reqwest::Client,
    base_url: Url,
    ttl: Duration,
    cache: Arc<Mutex<HashMap<String, CachedPrice>>>,
}

#[derive(Debug, Clone)]
struct CachedPrice {
    usd: Decimal,
    fetched_at: Instant,
}

#[derive(Debug, Deserialize)]
struct CoinGeckoPrice {
    usd: Decimal,
}

pub struct MockPricer;

impl CoinGeckoPricer {
    const DEFAULT_BASE_URL: &'static str = "https://api.coingecko.com/api/v3";
    const DEFAULT_TTL: Duration = Duration::from_secs(15);

    pub fn new() -> Self {
        Self::with_client_and_config(
            reqwest::Client::new(),
            Url::parse(Self::DEFAULT_BASE_URL).expect("valid CoinGecko base URL"),
            Self::DEFAULT_TTL,
        )
    }

    fn with_client_and_config(client: reqwest::Client, base_url: Url, ttl: Duration) -> Self {
        Self {
            client,
            base_url,
            ttl,
            cache: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    async fn usd_price_for_coin_ids(&self, coin_ids: &[String]) -> Result<(), PricerError> {
        let now = Instant::now();
        let unique_coin_ids = coin_ids.iter().fold(Vec::new(), |mut ids, coin_id| {
            if !ids.contains(coin_id) {
                ids.push(coin_id.clone());
            }
            ids
        });
        let missing_ids = {
            let cache = self.cache.lock().await;
            unique_coin_ids
                .iter()
                .filter(|coin_id| {
                    cache
                        .get(*coin_id)
                        .is_none_or(|entry| now.duration_since(entry.fetched_at) >= self.ttl)
                })
                .cloned()
                .collect::<Vec<_>>()
        };

        if missing_ids.is_empty() {
            return Ok(());
        }

        let mut url = self
            .base_url
            .join("simple/price")
            .expect("simple price endpoint should join");
        {
            let mut query = url.query_pairs_mut();
            query.append_pair("ids", &missing_ids.join(","));
            query.append_pair("vs_currencies", "usd");
        }

        let response = self.client.get(url).send().await?.error_for_status()?;
        let prices = response.json::<HashMap<String, CoinGeckoPrice>>().await?;

        let mut cache = self.cache.lock().await;
        for coin_id in missing_ids {
            let price = prices
                .get(&coin_id)
                .map(|entry| entry.usd)
                .ok_or_else(|| PricerError::MissingPrice(coin_id.clone()))?;
            cache.insert(
                coin_id,
                CachedPrice {
                    usd: price,
                    fetched_at: now,
                },
            );
        }

        Ok(())
    }
}

impl Default for CoinGeckoPricer {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Pricer for CoinGeckoPricer {
    async fn quote(
        &self,
        in_asset_symbol: &str,
        out_asset_symbol: &str,
        amount: &str,
    ) -> Result<PriceQuote, PricerError> {
        let amount_raw = Decimal::from_str_exact(amount)
            .map_err(|_| PricerError::InvalidAmount(amount.into()))?;
        let in_decimals = asset_decimals_for_symbol(in_asset_symbol)?;
        let out_decimals = asset_decimals_for_symbol(out_asset_symbol)?;

        let input_coin_id = coin_id_for_symbol(in_asset_symbol)?;
        let output_coin_id = coin_id_for_symbol(out_asset_symbol)?;
        self.usd_price_for_coin_ids(&[input_coin_id.clone(), output_coin_id.clone()])
            .await?;

        let cache = self.cache.lock().await;
        let input_price = cache
            .get(&input_coin_id)
            .map(|entry| entry.usd)
            .ok_or_else(|| PricerError::MissingPrice(input_coin_id.clone()))?;
        let output_price = cache
            .get(&output_coin_id)
            .map(|entry| entry.usd)
            .ok_or_else(|| PricerError::MissingPrice(output_coin_id.clone()))?;
        drop(cache);

        let input_amount = amount_raw / decimal_scale(in_decimals);
        let input_usd = input_amount * input_price;
        let output_amount = if output_price.is_zero() {
            Decimal::ZERO
        } else {
            (input_usd / output_price) * decimal_scale(out_decimals)
        }
        .round_dp(0);

        Ok(PriceQuote {
            input_usd: decimal_to_string(input_usd),
            output_usd: decimal_to_string(input_usd),
            output_amount: decimal_to_string(output_amount),
        })
    }
}

#[async_trait]
impl Pricer for MockPricer {
    async fn quote(
        &self,
        _in_asset_symbol: &str,
        _out_asset_symbol: &str,
        amount: &str,
    ) -> Result<PriceQuote, PricerError> {
        Ok(PriceQuote {
            input_usd: "1.0".to_owned(),
            output_usd: "1.0".to_owned(),
            output_amount: amount.to_owned(),
        })
    }
}

fn coin_id_for_symbol(symbol: &str) -> Result<String, PricerError> {
    match symbol.to_ascii_lowercase().as_str() {
        "eth" => Ok("ethereum".to_owned()),
        "usdc" => Ok("usd-coin".to_owned()),
        "usdt" => Ok("tether".to_owned()),
        "btc" => Ok("bitcoin".to_owned()),
        _ => Err(PricerError::UnsupportedAssetSymbol(symbol.to_owned())),
    }
}

fn asset_decimals_for_symbol(symbol: &str) -> Result<u32, PricerError> {
    match symbol.to_ascii_lowercase().as_str() {
        "eth" => Ok(18),
        "usdc" | "usdt" => Ok(6),
        "btc" => Ok(8),
        _ => Err(PricerError::UnsupportedAssetSymbol(symbol.to_owned())),
    }
}

fn decimal_scale(decimals: u32) -> Decimal {
    Decimal::from(10u64.pow(decimals))
}

fn decimal_to_string(value: Decimal) -> String {
    let normalized = value.normalize();
    if normalized.fract().is_zero() {
        format!("{normalized}.0")
    } else {
        normalized.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{method, path, query_param},
    };

    fn test_pricer(base_url: &str, ttl: Duration) -> CoinGeckoPricer {
        CoinGeckoPricer::with_client_and_config(
            reqwest::Client::new(),
            Url::parse(base_url).expect("wiremock base URL"),
            ttl,
        )
    }

    #[tokio::test]
    async fn caches_prices_within_ttl() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/simple/price"))
            .and(query_param("ids", "ethereum"))
            .and(query_param("vs_currencies", "usd"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "ethereum": { "usd": "2500.0" }
            })))
            .mount(&server)
            .await;

        let pricer = test_pricer(&format!("{}/", server.uri()), Duration::from_secs(15));

        let first = pricer.quote("eth", "eth", "1000000000000000000").await;
        let second = pricer.quote("eth", "eth", "1000000000000000000").await;

        assert!(first.is_ok());
        assert!(second.is_ok());
        assert_eq!(
            server
                .received_requests()
                .await
                .expect("received requests")
                .len(),
            1
        );
    }

    #[tokio::test]
    async fn refreshes_cache_after_ttl_expires() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/simple/price"))
            .and(query_param("ids", "ethereum"))
            .and(query_param("vs_currencies", "usd"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "ethereum": { "usd": "2500.0" }
            })))
            .mount(&server)
            .await;

        let pricer = test_pricer(&format!("{}/", server.uri()), Duration::from_millis(20));

        pricer
            .quote("eth", "eth", "1000000000000000000")
            .await
            .expect("first quote");
        tokio::time::sleep(Duration::from_millis(30)).await;
        pricer
            .quote("eth", "eth", "1000000000000000000")
            .await
            .expect("second quote");

        assert_eq!(
            server
                .received_requests()
                .await
                .expect("received requests")
                .len(),
            2
        );
    }

    #[tokio::test]
    async fn returns_error_for_failed_http_response() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/simple/price"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&server)
            .await;

        let pricer = test_pricer(&format!("{}/", server.uri()), Duration::from_secs(15));
        let error = pricer
            .quote("eth", "eth", "1000000000000000000")
            .await
            .expect_err("http error");

        assert!(matches!(error, PricerError::Request(_)));
    }

    #[tokio::test]
    async fn returns_error_for_missing_price_payload() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/simple/price"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({})))
            .mount(&server)
            .await;

        let pricer = test_pricer(&format!("{}/", server.uri()), Duration::from_secs(15));
        let error = pricer
            .quote("eth", "eth", "1000000000000000000")
            .await
            .expect_err("missing price");

        assert!(matches!(error, PricerError::MissingPrice(_)));
    }
}
