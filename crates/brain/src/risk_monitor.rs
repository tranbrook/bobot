//! Risk Monitor - PnL monitoring and emergency close functionality.

use anyhow::{Context, Result};
use redis::aio::ConnectionManager;
use tracing::{error, info, warn};
use zeroclaw_common::{PositionInfo, RiskConfig};

/// Risk Monitor for position PnL tracking and emergency actions
pub struct RiskMonitor {
    redis: ConnectionManager,
    config: RiskConfig,
}

impl RiskMonitor {
    /// Create a new Risk Monitor
    pub fn new(redis: ConnectionManager, config: RiskConfig) -> Self {
        Self { redis, config }
    }

    /// Read active position from Redis
    pub async fn get_position(&mut self, symbol: &str) -> Result<Option<PositionInfo>> {
        let key = format!("market:position:{}", symbol.to_uppercase());
        
        let json: Option<String> = redis::cmd("GET")
            .arg(&key)
            .query_async(&mut self.redis)
            .await
            .context("Failed to read position from Redis")?;

        match json {
            Some(json_str) => {
                let position: PositionInfo = serde_json::from_str(&json_str)
                    .context("Failed to deserialize PositionInfo")?;
                Ok(Some(position))
            }
            None => Ok(None),
        }
    }

    /// Check if emergency close is needed based on PnL
    pub fn check_emergency(&self, position: &PositionInfo) -> bool {
        self.config.is_emergency(position.unrealized_pnl_percent)
    }

    /// Monitor all active positions and return symbols needing emergency close
    pub async fn check_all_positions(&mut self, symbols: &[&str]) -> Result<Vec<String>> {
        let mut emergency_symbols = Vec::new();

        for &symbol in symbols {
            match self.get_position(symbol).await {
                Ok(Some(position)) => {
                    if self.check_emergency(&position) {
                        warn!(
                            "Emergency PnL threshold breached for {}: {:.2}%",
                            symbol, position.unrealized_pnl_percent
                        );
                        emergency_symbols.push(symbol.to_string());
                    }
                }
                Ok(None) => {}
                Err(e) => {
                    error!("Failed to get position for {}: {}", symbol, e);
                }
            }
        }

        Ok(emergency_symbols)
    }

    /// Get account balance from Redis
    pub async fn get_balance(&mut self) -> Result<Option<zeroclaw_common::AccountBalance>> {
        let json: Option<String> = redis::cmd("GET")
            .arg("market:account:balance")
            .query_async(&mut self.redis)
            .await
            .context("Failed to read balance from Redis")?;

        match json {
            Some(json_str) => {
                let balance: zeroclaw_common::AccountBalance = serde_json::from_str(&json_str)
                    .context("Failed to deserialize AccountBalance")?;
                Ok(Some(balance))
            }
            None => Ok(None),
        }
    }

    /// Get the config
    pub fn config(&self) -> &RiskConfig {
        &self.config
    }

    /// Update the config
    pub fn set_config(&mut self, config: RiskConfig) {
        self.config = config;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_check_emergency() {
        let config = RiskConfig::default();
        let monitor = RiskMonitor::new(
            ConnectionManager::ignore(),
            config.clone(),
        );

        let position_safe = PositionInfo {
            unrealized_pnl_percent: -3.0,
            ..Default::default()
        };
        assert!(!monitor.check_emergency(&position_safe));

        let position_emergency = PositionInfo {
            unrealized_pnl_percent: -6.0,
            ..Default::default()
        };
        assert!(monitor.check_emergency(&position_emergency));
    }
}
