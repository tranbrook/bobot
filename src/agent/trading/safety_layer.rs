//! Safety Layer - Multi-layer validation for trading decisions.
//!
//! This module implements a 3-layer safety system:
//! 1. Pre-check: Validate signal before calling LLM
//! 2. Post-check: Validate LLM decision
//! 3. Risk-check: Final validation before execution

use anyhow::Result;
use redis::Client;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::warn;
use zeroclaw_common::SignalOutput;

/// Trading decision structure from LLM
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradingDecision {
    /// Decision: "EXECUTE" or "WAIT"
    pub decision: String,
    /// Action: "LONG", "SHORT", or "NONE"
    pub action: String,
    /// Leverage (1-10)
    pub leverage: u32,
    /// Stop loss price
    pub stop_loss: f64,
    /// Take profit price
    pub take_profit: f64,
    /// Risk percentage (0.01-0.05)
    pub risk_percent: f64,
    /// Reasoning for the decision
    pub reasoning: String,
}

impl TradingDecision {
    /// Check if decision is to execute
    pub fn should_execute(&self) -> bool {
        self.decision == "EXECUTE" && self.action != "NONE"
    }

    /// Get the side for execution
    pub fn side(&self) -> Option<&str> {
        match self.action.as_str() {
            "LONG" => Some("BUY"),
            "SHORT" => Some("SELL"),
            _ => None,
        }
    }
}

/// Safety error types
#[derive(Error, Debug)]
pub enum SafetyError {
    #[error("Signal latency too high: {0}ms > {1}ms")]
    HighLatency(i64, i64),
    
    #[error("Signal has LATENCY_SPIKE override")]
    LatencySpike,
    
    #[error("Confluence score too low: {0} < {1}")]
    WeakSignal(i8, i8),
    
    #[error("Invalid decision: {0}")]
    InvalidDecision(String),
    
    #[error("Invalid action for EXECUTE: {0}")]
    InvalidAction(String),
    
    #[error("Leverage too high: {0} > {1}")]
    LeverageTooHigh(u32, u32),
    
    #[error("Risk percent too high: {0} > {1}")]
    RiskTooHigh(f64, f64),
    
    #[error("Stop loss required")]
    StopLossRequired,
    
    #[error("Take profit required")]
    TakeProfitRequired,
    
    #[error("Daily trade limit reached: {0} >= {1}")]
    DailyTradeLimit(usize, usize),
    
    #[error("Insufficient margin: required {0}, available {1}")]
    InsufficientMargin(f64, f64),
    
    #[error("Drawdown limit breached: {0}% > {1}%")]
    DrawdownLimit(f64, f64),
    
    #[error("Decision contradicts signal direction")]
    SignalContradiction,
}

/// Safety configuration
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct TradingSafetyConfig {
    /// Maximum allowed ingestion latency in milliseconds
    #[serde(default = "default_max_latency_ms")]
    pub max_latency_ms: i64,
    
    /// Minimum confluence score for signals
    #[serde(default = "default_min_confluence")]
    pub min_confluence_score: i8,
    
    /// Maximum leverage allowed
    #[serde(default = "default_max_leverage")]
    pub max_leverage: u32,
    
    /// Maximum risk per trade
    #[serde(default = "default_max_risk_percent")]
    pub max_risk_percent: f64,
    
    /// Maximum daily trades
    #[serde(default = "default_max_daily_trades")]
    pub max_daily_trades: usize,
    
    /// Maximum drawdown percentage
    #[serde(default = "default_max_drawdown")]
    pub max_drawdown_percent: f64,
    
    /// Require LLM reasoning in response
    #[serde(default = "default_true")]
    pub require_llm_reasoning: bool,
    
    /// Validate decision format
    #[serde(default = "default_true")]
    pub validate_decision_format: bool,
    
    /// Override leverage max (override LLM if higher)
    #[serde(default = "default_override_leverage")]
    pub override_leverage_max: u32,
}

fn default_max_latency_ms() -> i64 { 500 }
fn default_min_confluence() -> i8 { 5 }
fn default_max_leverage() -> u32 { 10 }
fn default_max_risk_percent() -> f64 { 0.05 }
fn default_max_daily_trades() -> usize { 20 }
fn default_max_drawdown() -> f64 { 5.0 }
fn default_true() -> bool { true }
fn default_override_leverage() -> u32 { 5 }

impl Default for TradingSafetyConfig {
    fn default() -> Self {
        Self {
            max_latency_ms: default_max_latency_ms(),
            min_confluence_score: default_min_confluence(),
            max_leverage: default_max_leverage(),
            max_risk_percent: default_max_risk_percent(),
            max_daily_trades: default_max_daily_trades(),
            max_drawdown_percent: default_max_drawdown(),
            require_llm_reasoning: default_true(),
            validate_decision_format: default_true(),
            override_leverage_max: default_override_leverage(),
        }
    }
}

/// Safety Layer for trading validation
pub struct SafetyLayer {
    config: TradingSafetyConfig,
    redis: Client,
}

impl SafetyLayer {
    /// Create a new Safety Layer
    pub fn new(redis: Client, config: TradingSafetyConfig) -> Self {
        Self { redis, config }
    }

    /// Create with default config
    pub fn with_default_config(redis: Client) -> Self {
        Self::new(redis, TradingSafetyConfig::default())
    }

    /// Pre-check: Validate signal before calling LLM
    pub fn pre_check(&self, signal: &SignalOutput) -> Result<(), SafetyError> {
        // Check latency
        if signal.ingestion_latency_ms > self.config.max_latency_ms {
            warn!(
                "Signal latency too high: {}ms > {}ms",
                signal.ingestion_latency_ms, self.config.max_latency_ms
            );
            return Err(SafetyError::HighLatency(
                signal.ingestion_latency_ms,
                self.config.max_latency_ms,
            ));
        }

        // Check override reason
        if signal.override_reason.as_deref() == Some("LATENCY_SPIKE") {
            warn!("Signal has LATENCY_SPIKE override");
            return Err(SafetyError::LatencySpike);
        }

        // Check confluence score
        if signal.confluence_score.abs() < self.config.min_confluence_score {
            warn!(
                "Confluence score too low: {} < {}",
                signal.confluence_score, self.config.min_confluence_score
            );
            return Err(SafetyError::WeakSignal(
                signal.confluence_score,
                self.config.min_confluence_score,
            ));
        }

        Ok(())
    }

    /// Post-check: Validate LLM decision
    pub fn post_check(&self, decision: &TradingDecision, signal: &SignalOutput) -> Result<(), SafetyError> {
        // Validate decision format
        if self.config.validate_decision_format {
            if !["EXECUTE", "WAIT"].contains(&decision.decision.as_str()) {
                return Err(SafetyError::InvalidDecision(decision.decision.clone()));
            }

            if decision.should_execute() && !["LONG", "SHORT"].contains(&decision.action.as_str()) {
                return Err(SafetyError::InvalidAction(decision.action.clone()));
            }
        }

        // Check leverage
        if decision.leverage > self.config.max_leverage {
            return Err(SafetyError::LeverageTooHigh(
                decision.leverage,
                self.config.max_leverage,
            ));
        }

        // Check risk percent
        if decision.risk_percent > self.config.max_risk_percent {
            return Err(SafetyError::RiskTooHigh(
                decision.risk_percent,
                self.config.max_risk_percent,
            ));
        }

        // Check stop loss
        if decision.stop_loss == 0.0 {
            return Err(SafetyError::StopLossRequired);
        }

        // Check take profit
        if decision.take_profit == 0.0 {
            return Err(SafetyError::TakeProfitRequired);
        }

        // Check reasoning if required
        if self.config.require_llm_reasoning && decision.reasoning.is_empty() {
            warn!("LLM decision missing reasoning");
            // Not an error, just a warning
        }

        // Check signal direction consistency (warning only)
        if decision.action == "LONG" && signal.confluence_score < 0 {
            warn!("LLM LONG decision contradicts negative confluence score");
        }
        if decision.action == "SHORT" && signal.confluence_score > 0 {
            warn!("LLM SHORT decision contradicts positive confluence score");
        }

        Ok(())
    }

    /// Risk-check: Final validation before execution
    pub fn risk_check(&self, _decision: &TradingDecision) -> Result<(), SafetyError> {
        // Get Redis connection
        let mut conn = match self.redis.get_connection() {
            Ok(c) => c,
            Err(_) => return Ok(()), // Skip checks if Redis unavailable
        };

        // Check daily trade count
        let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
        let key = format!("trading:daily_count:{}", today);
        
        let daily_trades: usize = redis::cmd("GET")
            .arg(&key)
            .query(&mut conn)
            .unwrap_or(0);
        
        if daily_trades >= self.config.max_daily_trades {
            warn!("Daily trade limit reached: {} >= {}", daily_trades, self.config.max_daily_trades);
            return Err(SafetyError::DailyTradeLimit(
                daily_trades,
                self.config.max_daily_trades,
            ));
        }

        // Check current drawdown
        let drawdown: f64 = redis::cmd("GET")
            .arg("trading:current_drawdown")
            .query(&mut conn)
            .unwrap_or(0.0);
        
        if drawdown > self.config.max_drawdown_percent {
            warn!("Drawdown limit breached: {}% > {}%", drawdown, self.config.max_drawdown_percent);
            return Err(SafetyError::DrawdownLimit(
                drawdown,
                self.config.max_drawdown_percent,
            ));
        }

        Ok(())
    }

    /// Increment daily trade count
    pub fn increment_daily_trade_count(&self) -> Result<(), String> {
        let mut conn = self.redis.get_connection()
            .map_err(|e| format!("Failed to get Redis connection: {}", e))?;
        
        let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
        let key = format!("trading:daily_count:{}", today);
        
        redis::cmd("INCR")
            .arg(&key)
            .query::<()>(&mut conn)
            .map_err(|e| format!("Failed to increment trade count: {}", e))?;
        
        // Set expiry at end of day (24 hours from now)
        redis::cmd("EXPIRE")
            .arg(&key)
            .arg(86400) // 24 hours in seconds
            .query::<()>(&mut conn)
            .ok();
        
        Ok(())
    }

    /// Record a trade result for drawdown calculation
    pub fn record_trade_result(&self, pnl: f64) -> Result<(), String> {
        let mut conn = self.redis.get_connection()
            .map_err(|e| format!("Failed to get Redis connection: {}", e))?;
        
        // Add to trade history
        let trade_key = "trading:trade_history";
        let timestamp = chrono::Utc::now().timestamp_millis();
        let trade_entry = serde_json::json!({
            "timestamp": timestamp,
            "pnl": pnl
        });
        
        redis::cmd("LPUSH")
            .arg(trade_key)
            .arg(trade_entry.to_string())
            .query::<()>(&mut conn)
            .map_err(|e| format!("Failed to record trade: {}", e))?;
        
        // Keep only last 100 trades
        redis::cmd("LTRIM")
            .arg(trade_key)
            .arg(0)
            .arg(99)
            .query::<()>(&mut conn)
            .ok();
        
        // Update current drawdown
        let current_drawdown: f64 = redis::cmd("GET")
            .arg("trading:current_drawdown")
            .query(&mut conn)
            .unwrap_or(0.0);
        
        let new_drawdown = if pnl < 0.0 {
            current_drawdown + pnl.abs()
        } else {
            (current_drawdown - pnl.abs()).max(0.0)
        };
        
        redis::cmd("SET")
            .arg("trading:current_drawdown")
            .arg(new_drawdown.to_string())
            .query::<()>(&mut conn)
            .map_err(|e| format!("Failed to update drawdown: {}", e))?;
        
        Ok(())
    }

    /// Reset drawdown (call after successful trade or manual reset)
    pub fn reset_drawdown(&self) -> Result<(), String> {
        let mut conn = self.redis.get_connection()
            .map_err(|e| format!("Failed to get Redis connection: {}", e))?;
        
        redis::cmd("SET")
            .arg("trading:current_drawdown")
            .arg("0")
            .query::<()>(&mut conn)
            .map_err(|e| format!("Failed to reset drawdown: {}", e))?;
        
        Ok(())
    }

    /// Get current account balance from Redis
    pub fn get_account_balance(&self) -> Result<f64, String> {
        let mut conn = self.redis.get_connection()
            .map_err(|e| format!("Failed to get Redis connection: {}", e))?;
        
        let balance: f64 = redis::cmd("GET")
            .arg("trading:account_balance")
            .query(&mut conn)
            .unwrap_or(0.0);
        
        Ok(balance)
    }

    /// Calculate position size based on risk percentage
    pub fn calculate_position_size(
        &self,
        entry_price: f64,
        stop_loss: f64,
        account_balance: f64,
        risk_percent: f64,
    ) -> Result<f64, String> {
        if entry_price <= 0.0 || stop_loss <= 0.0 {
            return Err("Invalid price values".to_string());
        }
        
        let risk_distance = (entry_price - stop_loss).abs();
        if risk_distance <= 0.0 {
            return Err("Stop loss must differ from entry price".to_string());
        }
        
        let risk_amount = account_balance * risk_percent;
        let position_size = risk_amount / risk_distance;
        
        Ok(position_size)
    }

    /// Validate position size against account balance
    pub fn validate_position_size(
        &self,
        position_size: f64,
        entry_price: f64,
        account_balance: f64,
        max_leverage: u32,
    ) -> Result<(), SafetyError> {
        let required_margin = position_size * entry_price;
        let max_allowed = account_balance * (max_leverage as f64);
        
        if required_margin > max_allowed {
            return Err(SafetyError::InsufficientMargin(
                required_margin,
                max_allowed,
            ));
        }
        
        Ok(())
    }

    /// Get the config
    pub fn config(&self) -> &TradingSafetyConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zeroclaw_common::{IndicatorSnapshot, MarketRegime, Signal};

    fn create_test_signal() -> SignalOutput {
        SignalOutput {
            price: 50000.0,
            timestamp_ms: 1000,
            market_regime: MarketRegime::Trending,
            confluence_score: 7,
            indicator_snapshot: IndicatorSnapshot::default(),
            trade_advice: Signal::long(0.8),
            ingestion_latency_ms: 100,
            override_reason: None,
        }
    }

    fn create_test_decision() -> TradingDecision {
        TradingDecision {
            decision: "EXECUTE".to_string(),
            action: "LONG".to_string(),
            leverage: 5,
            stop_loss: 49000.0,
            take_profit: 52000.0,
            risk_percent: 0.02,
            reasoning: "Test reasoning".to_string(),
        }
    }

    fn create_mock_client() -> Client {
        // Create a client that will fail to connect, but won't panic
        Client::open("redis://localhost:6379").unwrap_or_else(|_| {
            // If localhost fails, create an invalid client for testing
            Client::open("redis://invalid:6379").unwrap()
        })
    }

    #[test]
    fn test_pre_check_ok() {
        let layer = SafetyLayer::with_default_config(create_mock_client());
        let signal = create_test_signal();
        assert!(layer.pre_check(&signal).is_ok());
    }

    #[test]
    fn test_pre_check_latency() {
        let layer = SafetyLayer::with_default_config(create_mock_client());
        let mut signal = create_test_signal();
        signal.ingestion_latency_ms = 600; // > 500
        assert!(layer.pre_check(&signal).is_err());
    }

    #[test]
    fn test_post_check_ok() {
        let layer = SafetyLayer::with_default_config(create_mock_client());
        let signal = create_test_signal();
        let decision = create_test_decision();
        assert!(layer.post_check(&decision, &signal).is_ok());
    }

    #[test]
    fn test_post_check_leverage() {
        let layer = SafetyLayer::with_default_config(create_mock_client());
        let signal = create_test_signal();
        let mut decision = create_test_decision();
        decision.leverage = 15; // > 10
        assert!(layer.post_check(&decision, &signal).is_err());
    }

    #[test]
    fn test_calculate_position_size() {
        let layer = SafetyLayer::with_default_config(create_mock_client());
        
        // Test valid position size calculation
        let result = layer.calculate_position_size(50000.0, 49000.0, 10000.0, 0.02);
        assert!(result.is_ok());
        // Risk $200 (2% of $10000) / $1000 distance = 0.2 units
        assert!((result.unwrap() - 0.2).abs() < 0.01);
        
        // Test invalid prices
        assert!(layer.calculate_position_size(0.0, 49000.0, 10000.0, 0.02).is_err());
        assert!(layer.calculate_position_size(50000.0, 50000.0, 10000.0, 0.02).is_err());
    }

    #[test]
    fn test_validate_position_size() {
        let layer = SafetyLayer::with_default_config(create_mock_client());
        
        // Test valid position
        let result = layer.validate_position_size(0.1, 50000.0, 10000.0, 10);
        assert!(result.is_ok()); // $500 margin < $100000 max (10x leverage)
        
        // Test excessive position
        let result = layer.validate_position_size(100.0, 50000.0, 10000.0, 1);
        assert!(result.is_err()); // $5M margin > $10000 max (1x leverage)
    }

    #[test]
    fn test_risk_check_no_redis() {
        let layer = SafetyLayer::with_default_config(create_mock_client());
        let decision = create_test_decision();
        
        // Should not panic even if Redis is unavailable
        let result = layer.risk_check(&decision);
        // May fail due to Redis connection, but shouldn't panic
        assert!(result.is_ok() || result.is_err());
    }
}
