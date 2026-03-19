//! Binance REST API client for historical klines fetch.
//!
//! This module implements the bootstrap data loader that fetches
//! historical candlestick data from Binance to warm up indicators.
//!
//! **Key Features:**
//! - Fetches 1100 candles (100 warm-up + 1000 working window)
//! - Sets `received_at_ms` for latency tracking on each kline
//! - Handles rate limiting and retries
//! - Normalizes data to f64 with zero-null guarantee

use super::models::{Kline, KlineInterval};
use anyhow::{Context, Result};
use reqwest::{Client, StatusCode};
use serde::Deserialize;
use std::time::Duration;
use tracing::{debug, info, warn};

/// Binance API base URL for market data
const BINANCE_API_BASE: &str = "https://api.binance.com";

/// Default number of candles to fetch for bootstrap
/// 1100 = 100 warm-up + 1000 working window
const DEFAULT_LIMIT: usize = 1100;

/// Maximum retries for failed requests
const MAX_RETRIES: u32 = 3;

/// Base delay between retries (exponential backoff)
const RETRY_BASE_DELAY_MS: u64 = 1000;

/// Binance returns klines as arrays, not objects
/// We'll deserialize directly from the array
type RawKlineArray = [serde_json::Value; 12];

/// Binance REST client for fetching historical klines
pub struct BinanceRestClient {
    client: Client,
    base_url: String,
}

impl BinanceRestClient {
    /// Create a new Binance REST client with default settings.
    pub fn new() -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .user_agent("zeroclaw-muscle/0.1.0")
            .build()
            .context("Failed to create HTTP client")?;

        Ok(Self {
            client,
            base_url: BINANCE_API_BASE.to_string(),
        })
    }

    /// Create a new client with custom base URL (for testing or alternative endpoints).
    pub fn with_base_url(base_url: String) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .user_agent("zeroclaw-muscle/0.1.0")
            .build()
            .context("Failed to create HTTP client")?;

        Ok(Self { client, base_url })
    }

    /// Fetch historical klines from Binance.
    ///
    /// # Arguments
    /// * `symbol` - Trading pair symbol (e.g., "BTCUSDT")
    /// * `interval` - Candle interval
    /// * `limit` - Number of candles to fetch (default: 1100)
    ///
    /// # Returns
    /// Vector of Kline structs with `received_at_ms` set for latency tracking
    ///
    /// # Notes
    /// - Fetches 1100 candles by default (100 warm-up + 1000 working)
    /// - Each kline has `received_at_ms` set to current system time
    /// - Data is normalized to f64 and guaranteed zero-null
    pub async fn fetch_klines(
        &self,
        symbol: &str,
        interval: KlineInterval,
        limit: Option<usize>,
    ) -> Result<Vec<Kline>> {
        let limit = limit.unwrap_or(DEFAULT_LIMIT);
        let url = format!(
            "{}/api/v3/klines?symbol={}&interval={}&limit={}",
            self.base_url,
            symbol.to_uppercase(),
            interval.as_str(),
            limit
        );

        debug!("Fetching klines from: {}", url);

        // Retry loop with exponential backoff
        let mut last_error = None;
        for attempt in 0..MAX_RETRIES {
            match self.fetch_klines_inner(&url).await {
                Ok(klines) => {
                    info!(
                        "Successfully fetched {} klines for {} {} (attempt {})",
                        klines.len(),
                        symbol,
                        interval,
                        attempt + 1
                    );
                    return Ok(klines);
                }
                Err(e) => {
                    warn!(
                        "Failed to fetch klines (attempt {}/{}): {}",
                        attempt + 1,
                        MAX_RETRIES,
                        e
                    );
                    last_error = Some(e);

                    if attempt < MAX_RETRIES - 1 {
                        let delay_ms = RETRY_BASE_DELAY_MS * 2u64.pow(attempt);
                        debug!("Retrying in {}ms...", delay_ms);
                        tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                    }
                }
            }
        }

        Err(last_error.unwrap_or_else(|| anyhow::anyhow!("Unknown error fetching klines")))
    }

    /// Inner fetch implementation
    async fn fetch_klines_inner(&self, url: &str) -> Result<Vec<Kline>> {
        let response = self
            .client
            .get(url)
            .send()
            .await
            .context("Failed to send HTTP request")?;

        // Handle rate limiting
        if response.status() == StatusCode::TOO_MANY_REQUESTS {
            let retry_after = response
                .headers()
                .get("Retry-After")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(5);

            warn!("Rate limited. Retry-After: {}s", retry_after);
            tokio::time::sleep(Duration::from_secs(retry_after)).await;
            return Err(anyhow::anyhow!("Rate limited"));
        }

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!("HTTP {}: {}", status, body));
        }

        // Get raw text first for debugging
        let raw_text = response.text().await.context("Failed to get response text")?;
        debug!("Raw response: {}", raw_text);

        // Parse JSON manually as array of arrays
        let raw_klines: Vec<RawKlineArray> = serde_json::from_str(&raw_text)
            .context("Failed to parse JSON response")?;

        // Convert to Kline structs with received_at_ms set
        let received_at_ms = Kline::current_time_ms();
        // Symbol is extracted from the URL - extract from query params
        let symbol = url.split("symbol=").nth(1).and_then(|s| s.split('&').next()).unwrap_or("UNKNOWN");
        let klines = raw_klines
            .iter()
            .map(|raw| self.raw_kline_to_kline(raw, symbol, received_at_ms))
            .collect::<Result<Vec<_>>>()?;

        Ok(klines)
    }

    /// Convert raw API response array to Kline struct
    fn raw_kline_to_kline(&self, raw: &RawKlineArray, symbol: &str, received_at_ms: i64) -> Result<Kline> {
        Ok(Kline {
            symbol: symbol.to_string(),
            timestamp_ms: raw[6].as_i64().ok_or_else(|| anyhow::anyhow!("Invalid close_time"))?,
            open: parse_f64(raw[1].as_str().unwrap_or(""), "open")?,
            high: parse_f64(raw[2].as_str().unwrap_or(""), "high")?,
            low: parse_f64(raw[3].as_str().unwrap_or(""), "low")?,
            close: parse_f64(raw[4].as_str().unwrap_or(""), "close")?,
            volume: parse_f64(raw[5].as_str().unwrap_or(""), "volume")?,
            is_closed: true, // Historical candles are always closed
            received_at_ms,
        })
    }

    /// Fetch klines with a specific end time (for backfilling).
    ///
    /// # Arguments
    /// * `symbol` - Trading pair symbol
    /// * `interval` - Candle interval
    /// * `end_time` - End time in milliseconds
    /// * `limit` - Number of candles to fetch
    pub async fn fetch_klines_with_end_time(
        &self,
        symbol: &str,
        interval: KlineInterval,
        end_time: i64,
        limit: Option<usize>,
    ) -> Result<Vec<Kline>> {
        let limit = limit.unwrap_or(DEFAULT_LIMIT);
        let url = format!(
            "{}/api/v3/klines?symbol={}&interval={}&endTime={}&limit={}",
            self.base_url,
            symbol.to_uppercase(),
            interval.as_str(),
            end_time,
            limit
        );

        debug!("Fetching klines with endTime from: {}", url);

        self.fetch_klines_inner(&url).await
    }

    /// Get the current server time from Binance.
    pub async fn get_server_time(&self) -> Result<i64> {
        let url = format!("{}/api/v3/time", self.base_url);

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .context("Failed to get server time")?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!("Failed to get server time: {}", response.status()));
        }

        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct TimeResponse {
            server_time: i64,
        }

        let time_response: TimeResponse = response
            .json()
            .await
            .context("Failed to parse time response")?;

        Ok(time_response.server_time)
    }

    /// Validate connectivity to Binance API.
    pub async fn ping(&self) -> Result<()> {
        let _ = self.get_server_time().await?;
        Ok(())
    }
}

impl Default for BinanceRestClient {
    fn default() -> Self {
        Self::new().expect("Failed to create default BinanceRestClient")
    }
}

/// Parse a string value to f64 with error context
fn parse_f64(value: &str, field: &str) -> Result<f64> {
    value
        .parse::<f64>()
        .with_context(|| format!("Failed to parse {} field: '{}'", field, value))
}

/// Bootstrap data loader that fetches and validates historical klines
pub struct BootstrapDataLoader {
    client: BinanceRestClient,
    warmup_candles: usize,
    working_candles: usize,
}

impl BootstrapDataLoader {
    /// Create a new bootstrap data loader.
    ///
    /// # Arguments
    /// * `warmup_candles` - Number of candles for indicator warm-up (default: 100)
    /// * `working_candles` - Number of candles for working window (default: 1000)
    pub fn new(warmup_candles: Option<usize>, working_candles: Option<usize>) -> Result<Self> {
        Ok(Self {
            client: BinanceRestClient::new()?,
            warmup_candles: warmup_candles.unwrap_or(100),
            working_candles: working_candles.unwrap_or(1000),
        })
    }

    /// Load bootstrap data for a symbol and interval.
    ///
    /// # Arguments
    /// * `symbol` - Trading pair symbol (e.g., "BTCUSDT")
    /// * `interval` - Candle interval
    ///
    /// # Returns
    /// Vector of Kline structs with total count = warmup + working
    pub async fn load(&self, symbol: &str, interval: KlineInterval) -> Result<Vec<Kline>> {
        let total_candles = self.warmup_candles + self.working_candles;

        info!(
            "Loading bootstrap data for {} {}: {} warmup + {} working = {} total",
            symbol,
            interval,
            self.warmup_candles,
            self.working_candles,
            total_candles
        );

        let klines = self.client.fetch_klines(symbol, interval, Some(total_candles)).await?;

        // Validate we got enough data
        if klines.len() < total_candles {
            warn!(
                "Received {} klines, expected {}. Indicators may not be fully converged.",
                klines.len(),
                total_candles
            );
        }

        // Validate data quality
        self.validate_klines(&klines)?;

        info!(
            "Bootstrap data loaded: {} candles ({} warmup, {} working)",
            klines.len(),
            self.warmup_candles,
            self.working_candles
        );

        Ok(klines)
    }

    /// Validate kline data quality.
    fn validate_klines(&self, klines: &[Kline]) -> Result<()> {
        if klines.is_empty() {
            return Err(anyhow::anyhow!("No klines received"));
        }

        // Check for null/invalid prices
        for (i, kline) in klines.iter().enumerate() {
            if kline.open <= 0.0
                || kline.high <= 0.0
                || kline.low <= 0.0
                || kline.close <= 0.0
            {
                return Err(anyhow::anyhow!(
                    "Invalid price data at index {}: {:?}",
                    i,
                    kline
                ));
            }

            // High should be >= Low
            if kline.high < kline.low {
                return Err(anyhow::anyhow!(
                    "Invalid OHLC: high < low at index {}",
                    i
                ));
            }

            // Close should be within high/low range
            if kline.close > kline.high || kline.close < kline.low {
                return Err(anyhow::anyhow!(
                    "Invalid OHLC: close outside high/low range at index {}",
                    i
                ));
            }
        }

        // Check for chronological order
        for i in 1..klines.len() {
            if klines[i].timestamp_ms <= klines[i - 1].timestamp_ms {
                return Err(anyhow::anyhow!(
                    "Klines not in chronological order at index {}",
                    i
                ));
            }
        }

        Ok(())
    }

    /// Get the warmup candle count.
    pub fn warmup_candles(&self) -> usize {
        self.warmup_candles
    }

    /// Get the working candle count.
    pub fn working_candles(&self) -> usize {
        self.working_candles
    }
}

/// Fetch historical klines with default settings.
///
/// Convenience function for quick bootstrap data loading.
pub async fn fetch_bootstrap_klines(
    symbol: &str,
    interval: KlineInterval,
) -> Result<Vec<Kline>> {
    let client = BinanceRestClient::new()?;
    client.fetch_klines(symbol, interval, Some(1100)).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_f64_valid() {
        assert_eq!(parse_f64("100.50", "price").unwrap(), 100.50);
        assert_eq!(parse_f64("0.00123", "volume").unwrap(), 0.00123);
        assert_eq!(parse_f64("1000000", "volume").unwrap(), 1000000.0);
    }

    #[test]
    fn test_parse_f64_invalid() {
        assert!(parse_f64("not_a_number", "price").is_err());
        assert!(parse_f64("", "volume").is_err());
    }

    #[tokio::test]
    async fn test_binance_client_creation() {
        let client = BinanceRestClient::new();
        assert!(client.is_ok());
    }

    #[tokio::test]
    async fn test_binance_ping() {
        let client = BinanceRestClient::new().unwrap();
        let result = client.ping().await;
        // This test may fail due to network issues, so we just log
        if let Err(e) = &result {
            println!("Ping failed (network issue expected in test): {}", e);
        }
    }

    #[tokio::test]
    async fn test_fetch_klines_structure() {
        let client = BinanceRestClient::new().unwrap();
        
        // Fetch a small number for testing
        let result = client.fetch_klines("BTCUSDT", KlineInterval::Minute1, Some(10)).await;
        
        if let Ok(klines) = result {
            assert!(!klines.is_empty());
            assert_eq!(klines.len(), 10);
            
            // Verify structure
            for kline in &klines {
                assert!(kline.open > 0.0);
                assert!(kline.high > 0.0);
                assert!(kline.low > 0.0);
                assert!(kline.close > 0.0);
                assert!(kline.volume >= 0.0);
                assert!(kline.is_closed);
                assert!(kline.received_at_ms > 0);
                
                // Verify OHLC logic
                assert!(kline.high >= kline.low);
                assert!(kline.close >= kline.low && kline.close <= kline.high);
            }
            
            // Verify chronological order
            for i in 1..klines.len() {
                assert!(klines[i].timestamp_ms > klines[i - 1].timestamp_ms);
            }
        } else {
            println!("Fetch failed (network issue expected in test): {:?}", result);
        }
    }

    #[test]
    fn test_bootstrap_data_loader_creation() {
        let loader = BootstrapDataLoader::new(Some(100), Some(1000));
        assert!(loader.is_ok());
        
        let loader = loader.unwrap();
        assert_eq!(loader.warmup_candles(), 100);
        assert_eq!(loader.working_candles(), 1000);
    }

    #[test]
    fn test_bootstrap_data_loader_defaults() {
        let loader = BootstrapDataLoader::new(None, None).unwrap();
        assert_eq!(loader.warmup_candles(), 100);
        assert_eq!(loader.working_candles(), 1000);
    }
}
