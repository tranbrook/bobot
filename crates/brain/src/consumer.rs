//! Signal Consumer - reads trading signals from Redis.

use anyhow::{Context, Result};
use redis::aio::ConnectionManager;
use tracing::{debug, error, info};
use zeroclaw_common::SignalOutput;

/// Signal Consumer that reads from Redis market:signal:{SYMBOL}
pub struct SignalConsumer {
    redis: ConnectionManager,
    key_prefix: String,
}

impl SignalConsumer {
    /// Create a new Signal Consumer
    pub fn new(redis: ConnectionManager, key_prefix: Option<&str>) -> Self {
        Self {
            redis,
            key_prefix: key_prefix.unwrap_or("market:signal").to_string(),
        }
    }

    /// Get the Redis key for a symbol
    fn get_signal_key(&self, symbol: &str) -> String {
        format!("{}:{}", self.key_prefix, symbol.to_uppercase())
    }

    /// Read signal for a symbol from Redis
    pub async fn read_signal(&mut self, symbol: &str) -> Result<Option<SignalOutput>> {
        let key = self.get_signal_key(symbol);
        
        let json: Option<String> = redis::cmd("GET")
            .arg(&key)
            .query_async(&mut self.redis)
            .await
            .context("Failed to read signal from Redis")?;

        match json {
            Some(json_str) => {
                let signal: SignalOutput = serde_json::from_str(&json_str)
                    .context("Failed to deserialize SignalOutput")?;
                debug!("Read signal for {}: {:?}", symbol, signal.market_regime);
                Ok(Some(signal))
            }
            None => {
                debug!("No signal found for {}", symbol);
                Ok(None)
            }
        }
    }

    /// Subscribe to signal updates for multiple symbols
    pub async fn subscribe_signals(
        &mut self,
        symbols: &[&str],
    ) -> Result<Vec<SignalOutput>> {
        let mut signals = Vec::new();
        
        for &symbol in symbols {
            match self.read_signal(symbol).await {
                Ok(Some(signal)) => signals.push(signal),
                Ok(None) => {}
                Err(e) => {
                    error!("Failed to read signal for {}: {}", symbol, e);
                }
            }
        }

        Ok(signals)
    }

    /// Wait for signal update with timeout
    pub async fn wait_for_signal(
        &mut self,
        symbol: &str,
        timeout_ms: u64,
    ) -> Result<Option<SignalOutput>> {
        let key = self.get_signal_key(symbol);
        
        // Poll for signal update
        let start = std::time::Instant::now();
        let timeout = std::time::Duration::from_millis(timeout_ms);
        
        while start.elapsed() < timeout {
            let json: Option<String> = redis::cmd("GET")
                .arg(&key)
                .query_async(&mut self.redis)
                .await
                .ok()
                .flatten();

            if let Some(json_str) = json {
                if let Ok(signal) = serde_json::from_str::<SignalOutput>(&json_str) {
                    info!("Received updated signal for {}", symbol);
                    return Ok(Some(signal));
                }
            }

            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }

        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_signal_key() {
        let consumer = SignalConsumer::new(
            ConnectionManager::ignore(), // Mock - won't actually connect
            Some("market:signal"),
        );
        
        // This test just verifies the key format logic
        assert_eq!(consumer.get_signal_key("BTCUSDT"), "market:signal:BTCUSDT");
        assert_eq!(consumer.get_signal_key("ethusdt"), "market:signal:ETHUSDT");
    }
}
