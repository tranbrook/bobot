//! Risk management data structures.
//!
//! This module defines types for risk configuration and position monitoring.

use serde::{Deserialize, Serialize};

/// Risk configuration for the Brain module.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskConfig {
    /// Maximum allowed ingestion latency in milliseconds (default: 500ms)
    pub max_latency_ms: i64,
    /// Maximum drawdown percentage before emergency action (default: 5%)
    pub max_drawdown_percent: f64,
    /// Emergency PnL threshold - if unrealized PnL drops below this, close all positions
    pub emergency_pnl_threshold: f64,
    /// Maximum risk per trade as percentage of account (default: 2%)
    pub max_risk_per_trade: f64,
    /// Maximum leverage allowed (default: 10x)
    pub max_leverage: u32,
}

impl Default for RiskConfig {
    fn default() -> Self {
        Self {
            max_latency_ms: 500,
            max_drawdown_percent: 5.0,
            emergency_pnl_threshold: -5.0,
            max_risk_per_trade: 0.02,
            max_leverage: 10,
        }
    }
}

impl RiskConfig {
    /// Check if PnL has breached the emergency threshold.
    pub fn is_emergency(&self, unrealized_pnl_percent: f64) -> bool {
        unrealized_pnl_percent < self.emergency_pnl_threshold
    }

    /// Calculate position size based on risk percentage.
    pub fn calculate_position_size(&self, account_balance: f64, stop_loss_distance: f64) -> f64 {
        if stop_loss_distance <= 0.0 {
            return 0.0;
        }
        
        let risk_amount = account_balance * self.max_risk_per_trade;
        risk_amount / stop_loss_distance
    }
}

/// Active position information.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PositionInfo {
    /// Trading pair symbol
    pub symbol: String,
    /// Position side: "LONG" or "SHORT"
    pub side: String,
    /// Position quantity
    pub quantity: f64,
    /// Entry price
    pub entry_price: f64,
    /// Current market price
    pub current_price: f64,
    /// Unrealized PnL in quote currency
    pub unrealized_pnl: f64,
    /// Unrealized PnL as percentage
    pub unrealized_pnl_percent: f64,
    /// Stop loss price
    pub stop_loss: Option<f64>,
    /// Take profit price
    pub take_profit: Option<f64>,
    /// Position opened timestamp
    pub opened_at_ms: i64,
}

impl PositionInfo {
    /// Check if position is long.
    pub fn is_long(&self) -> bool {
        self.side == "LONG"
    }

    /// Check if position is short.
    pub fn is_short(&self) -> bool {
        self.side == "SHORT"
    }

    /// Update position with current price and recalculate PnL.
    pub fn update_price(&mut self, current_price: f64) {
        self.current_price = current_price;
        
        if self.is_long() {
            self.unrealized_pnl = (current_price - self.entry_price) * self.quantity;
            self.unrealized_pnl_percent = (current_price - self.entry_price) / self.entry_price * 100.0;
        } else {
            self.unrealized_pnl = (self.entry_price - current_price) * self.quantity;
            self.unrealized_pnl_percent = (self.entry_price - current_price) / self.entry_price * 100.0;
        }
    }

    /// Check if stop loss has been hit.
    pub fn is_stop_loss_hit(&self) -> bool {
        if let Some(sl) = self.stop_loss {
            if self.is_long() {
                return self.current_price <= sl;
            } else {
                return self.current_price >= sl;
            }
        }
        false
    }

    /// Check if take profit has been hit.
    pub fn is_take_profit_hit(&self) -> bool {
        if let Some(tp) = self.take_profit {
            if self.is_long() {
                return self.current_price >= tp;
            } else {
                return self.current_price <= tp;
            }
        }
        false
    }

    /// Deserialize from JSON string.
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    /// Serialize to JSON string.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }
}

/// Account balance information.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AccountBalance {
    /// Total balance in quote currency
    pub total: f64,
    /// Available balance (not in positions)
    pub available: f64,
    /// Balance in positions
    pub in_positions: f64,
    /// Unrealized PnL from all positions
    pub unrealized_pnl: f64,
    /// Timestamp
    pub timestamp_ms: i64,
}

impl AccountBalance {
    /// Deserialize from JSON string.
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    /// Serialize to JSON string.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_risk_config_is_emergency() {
        let config = RiskConfig::default();
        assert!(config.is_emergency(-6.0));
        assert!(!config.is_emergency(-4.0));
    }

    #[test]
    fn test_risk_config_position_size() {
        let config = RiskConfig::default();
        let size = config.calculate_position_size(10000.0, 100.0);
        assert!((size - 2.0).abs() < 0.01); // 2% of 10000 / 100 = 2
    }

    #[test]
    fn test_position_info_pnl_long() {
        let mut pos = PositionInfo {
            symbol: "BTCUSDT".to_string(),
            side: "LONG".to_string(),
            quantity: 0.1,
            entry_price: 50000.0,
            current_price: 50000.0,
            unrealized_pnl: 0.0,
            unrealized_pnl_percent: 0.0,
            stop_loss: Some(49000.0),
            take_profit: Some(52000.0),
            opened_at_ms: 1000,
        };

        pos.update_price(51000.0);
        assert!((pos.unrealized_pnl - 100.0).abs() < 0.01); // (51000 - 50000) * 0.1
        assert!((pos.unrealized_pnl_percent - 2.0).abs() < 0.01); // 2%
    }

    #[test]
    fn test_position_info_pnl_short() {
        let mut pos = PositionInfo {
            symbol: "BTCUSDT".to_string(),
            side: "SHORT".to_string(),
            quantity: 0.1,
            entry_price: 50000.0,
            current_price: 50000.0,
            unrealized_pnl: 0.0,
            unrealized_pnl_percent: 0.0,
            stop_loss: Some(51000.0),
            take_profit: Some(48000.0),
            opened_at_ms: 1000,
        };

        pos.update_price(49000.0);
        assert!((pos.unrealized_pnl - 100.0).abs() < 0.01); // (50000 - 49000) * 0.1
        assert!((pos.unrealized_pnl_percent - 2.0).abs() < 0.01); // 2%
    }

    #[test]
    fn test_position_stop_loss_hit() {
        let mut pos = PositionInfo {
            symbol: "BTCUSDT".to_string(),
            side: "LONG".to_string(),
            quantity: 0.1,
            entry_price: 50000.0,
            current_price: 50000.0,
            unrealized_pnl: 0.0,
            unrealized_pnl_percent: 0.0,
            stop_loss: Some(49000.0),
            take_profit: Some(52000.0),
            opened_at_ms: 1000,
        };

        pos.update_price(48900.0);
        assert!(pos.is_stop_loss_hit());
        assert!(!pos.is_take_profit_hit());
    }

    #[test]
    fn test_position_serialization() {
        let pos = PositionInfo {
            symbol: "BTCUSDT".to_string(),
            side: "LONG".to_string(),
            quantity: 0.1,
            entry_price: 50000.0,
            current_price: 51000.0,
            unrealized_pnl: 100.0,
            unrealized_pnl_percent: 2.0,
            stop_loss: Some(49000.0),
            take_profit: Some(52000.0),
            opened_at_ms: 1000,
        };

        let json = pos.to_json().unwrap();
        let parsed = PositionInfo::from_json(&json).unwrap();

        assert_eq!(parsed.symbol, "BTCUSDT");
        assert_eq!(parsed.quantity, 0.1);
    }
}
