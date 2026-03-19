//! Redis client for signal export.
//!
//! This module implements the Redis client that publishes trading signals
//! to Redis hashes for downstream consumption.
//!
//! ## Redis Key Format
//!
//! - Signal Hash: `market:signal:{SYMBOL}` (e.g., `market:signal:BTCUSDT`)
//!
//! ## Data Format
//!
//! Each signal is stored as a JSON object with the following structure:
//! ```json
//! {
//!   "price": 95432.50,
//!   "timestamp_ms": 1710288000000,
//!   "market_regime": "TRENDING",
//!   "confluence_score": 7,
//!   "indicator_snapshot": { ... },
//!   "trade_advice": "LONG",
//!   "ingestion_latency_ms": 45,
//!   "override_reason": null
//! }
//! ```

use anyhow::{Context, Result};
use redis::{Client, Commands};
use serde_json;
use tracing::{debug, error};

use crate::data::models::SignalOutput;

/// Redis client wrapper for signal storage
pub struct RedisSignalStore {
    client: Client,
    key_prefix: String,
}

impl RedisSignalStore {
    /// Create a new Redis signal store
    ///
    /// # Arguments
    /// * `redis_url` - Redis connection URL (e.g., "redis://localhost:6379")
    /// * `key_prefix` - Prefix for Redis keys (default: "market:signal")
    pub fn new(redis_url: &str, key_prefix: Option<&str>) -> Result<Self> {
        let client = Client::open(redis_url)
            .with_context(|| format!("Failed to connect to Redis at {}", redis_url))?;

        Ok(Self {
            client,
            key_prefix: key_prefix.unwrap_or("market:signal").to_string(),
        })
    }

    /// Get the Redis key for a symbol
    fn get_signal_key(&self, symbol: &str) -> String {
        format!("{}:{}", self.key_prefix, symbol.to_uppercase())
    }

    /// Store a signal output to Redis
    ///
    /// # Arguments
    /// * `symbol` - Trading pair symbol (e.g., "BTCUSDT")
    /// * `signal` - SignalOutput to store
    ///
    /// # Returns
    /// true if successful, false otherwise
    pub fn store_signal(&self, symbol: &str, signal: &SignalOutput) -> Result<bool> {
        let key = self.get_signal_key(symbol);

        // Serialize signal to JSON
        let json = serde_json::to_string(signal)
            .context("Failed to serialize signal to JSON")?;

        // Get connection and store
        let mut conn = self.client.get_connection()
            .context("Failed to get Redis connection")?;

        // Store as hash for better field-level access
        conn.set::<_, _, ()>(&key, json)
            .context("Failed to store signal in Redis")?;

        debug!("Stored signal for {} at key {}", symbol, key);
        Ok(true)
    }

    /// Store a signal output to Redis asynchronously
    ///
    /// # Arguments
    /// * `symbol` - Trading pair symbol
    /// * `signal` - SignalOutput to store
    pub async fn store_signal_async(&self, symbol: &str, signal: &SignalOutput) -> Result<bool> {
        let key = self.get_signal_key(symbol);
        let json = serde_json::to_string(signal)
            .context("Failed to serialize signal to JSON")?;

        let mut conn = self.client.get_multiplexed_async_connection().await
            .context("Failed to get async Redis connection")?;

        let _: () = redis::cmd("SET")
            .arg(&key)
            .arg(&json)
            .query_async(&mut conn)
            .await
            .context("Failed to store signal in Redis")?;

        debug!("Stored async signal for {} at key {}", symbol, key);
        Ok(true)
    }

    /// Get the latest signal for a symbol
    ///
    /// # Arguments
    /// * `symbol` - Trading pair symbol
    ///
    /// # Returns
    /// SignalOutput if found, None otherwise
    pub fn get_signal(&self, symbol: &str) -> Result<Option<SignalOutput>> {
        let key = self.get_signal_key(symbol);

        let mut conn = self.client.get_connection()
            .context("Failed to get Redis connection")?;

        let json: Option<String> = conn.get(&key)
            .context("Failed to get signal from Redis")?;

        match json {
            Some(json_str) => {
                let signal: SignalOutput = serde_json::from_str(&json_str)
                    .context("Failed to deserialize signal from JSON")?;
                Ok(Some(signal))
            }
            None => Ok(None),
        }
    }

    /// Delete a signal from Redis
    ///
    /// # Arguments
    /// * `symbol` - Trading pair symbol
    pub fn delete_signal(&self, symbol: &str) -> Result<bool> {
        let key = self.get_signal_key(symbol);

        let mut conn = self.client.get_connection()
            .context("Failed to get Redis connection")?;

        let deleted: bool = conn.del(&key)
            .context("Failed to delete signal from Redis")?;

        Ok(deleted)
    }

    /// Check if Redis connection is healthy
    pub fn health_check(&self) -> bool {
        let mut conn = match self.client.get_connection() {
            Ok(c) => c,
            Err(e) => {
                error!("Redis health check failed: {}", e);
                return false;
            }
        };

        let result: Result<(), _> = redis::cmd("PING").query(&mut conn);
        result.is_ok()
    }

    /// Get the key prefix
    pub fn key_prefix(&self) -> &str {
        &self.key_prefix
    }
}

/// Builder for RedisSignalStore with fluent API
pub struct RedisSignalStoreBuilder {
    redis_url: String,
    key_prefix: Option<String>,
    max_connections: Option<u32>,
}

impl RedisSignalStoreBuilder {
    /// Create a new builder
    pub fn new(redis_url: &str) -> Self {
        Self {
            redis_url: redis_url.to_string(),
            key_prefix: None,
            max_connections: None,
        }
    }

    /// Set the key prefix
    pub fn key_prefix(mut self, prefix: &str) -> Self {
        self.key_prefix = Some(prefix.to_string());
        self
    }

    /// Set maximum connections (for future connection pooling)
    pub fn max_connections(mut self, max: u32) -> Self {
        self.max_connections = Some(max);
        self
    }

    /// Build the RedisSignalStore
    pub fn build(self) -> Result<RedisSignalStore> {
        RedisSignalStore::new(&self.redis_url, self.key_prefix.as_deref())
    }
}

/// Create a RedisSignalStore with default settings
pub fn create_redis_store(redis_url: &str) -> Result<RedisSignalStore> {
    RedisSignalStore::new(redis_url, None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::models::{MarketRegime, Signal, IndicatorSnapshot};

    fn create_test_signal() -> SignalOutput {
        SignalOutput::new(
            50000.0,
            1234567890000,
            MarketRegime::Trending,
            7,
            IndicatorSnapshot::default(),
            Signal::long(0.8),
            1234567890000,
        )
    }

    #[test]
    fn test_redis_store_builder() {
        let store = RedisSignalStoreBuilder::new("redis://localhost:6379")
            .key_prefix("test:signal")
            .max_connections(10)
            .build();

        // Connection will fail if Redis isn't running, but builder should work
        assert!(store.is_err() || store.unwrap().key_prefix() == "test:signal");
    }

    #[test]
    fn test_get_signal_key() {
        let store = RedisSignalStore::new("redis://localhost:6379", Some("test")).unwrap_or_else(|_| {
            // Create a mock for testing without Redis
            RedisSignalStore {
                client: Client::open("redis://localhost:6379").unwrap(),
                key_prefix: "test".to_string(),
            }
        });

        assert_eq!(store.get_signal_key("BTCUSDT"), "test:BTCUSDT");
        assert_eq!(store.get_signal_key("ethusdt"), "test:ETHUSDT");
    }

    #[test]
    fn test_signal_serialization() {
        let signal = create_test_signal();
        let json = serde_json::to_string(&signal).unwrap();

        // Verify JSON structure
        assert!(json.contains("\"price\":50000.0"));
        assert!(json.contains("\"market_regime\":\"TRENDING\""));
        assert!(json.contains("\"confluence_score\":7"));
    }

    #[test]
    fn test_signal_deserialization() {
        // Create a signal and serialize it, then deserialize to verify round-trip
        let signal = SignalOutput::new(
            50000.0,
            1234567890000,
            MarketRegime::Ranging,
            -5,
            IndicatorSnapshot::default(),
            Signal::short(0.5),
            1234567890000,
        );

        let json = serde_json::to_string(&signal).unwrap();
        let deserialized: SignalOutput = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.price, 50000.0);
        assert_eq!(deserialized.confluence_score, -5);
        assert!(matches!(deserialized.trade_advice, Signal::Short { .. }));
    }
}
