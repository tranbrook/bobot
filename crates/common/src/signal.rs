//! Signal-related data structures.
//!
//! This module defines the core types for trading signals, execution commands,
//! and LLM decisions.

use serde::{Deserialize, Serialize};

/// Market regime classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MarketRegime {
    /// Strong trend market (ADX > 25)
    Trending,
    /// Range-bound market (ADX < 20, low volatility)
    Ranging,
    /// High volatility market
    Volatile,
}

impl std::fmt::Display for MarketRegime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Trending => write!(f, "TRENDING"),
            Self::Ranging => write!(f, "RANGING"),
            Self::Volatile => write!(f, "VOLATILE"),
        }
    }
}

/// Trading signal direction.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "direction", content = "strength")]
pub enum Signal {
    /// Long (buy) signal with strength 0.0-1.0
    Long { strength: f64 },
    /// Short (sell) signal with strength 0.0-1.0
    Short { strength: f64 },
    /// No position / wait
    Wait,
}

impl Signal {
    /// Convert signal to numeric value for aggregation.
    #[inline]
    pub fn to_value(&self) -> f64 {
        match self {
            Self::Long { strength } => *strength,
            Self::Short { strength } => -*strength,
            Self::Wait => 0.0,
        }
    }

    /// Get the action string.
    pub fn action(&self) -> &'static str {
        match self {
            Self::Long { .. } => "LONG",
            Self::Short { .. } => "SHORT",
            Self::Wait => "WAIT",
        }
    }
}

impl Default for Signal {
    fn default() -> Self {
        Self::Wait
    }
}

/// Snapshot of key indicator values at signal time.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct IndicatorSnapshot {
    /// Relative Strength Index (14-period)
    pub rsi: f64,
    /// Upper Bollinger Band value
    pub bb_upper: f64,
    /// Lower Bollinger Band value
    pub bb_lower: f64,
    /// Ultimate Smoother value
    pub us_value: f64,
    /// Average Directional Index
    pub adx: f64,
    /// Bollinger Band Width (normalized)
    pub bb_width: f64,
    /// ATR (Average True Range) value
    pub atr: f64,
    /// MACD line value
    pub macd: f64,
    /// MACD signal line value
    pub macd_signal: f64,
    /// MACD histogram value
    pub macd_histogram: f64,
}

/// Complete signal output from Muscle-Kit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalOutput {
    /// Current price used for signal generation
    pub price: f64,
    /// Timestamp of the signal in milliseconds since epoch
    pub timestamp_ms: i64,
    /// Current market regime
    pub market_regime: MarketRegime,
    /// Aggregated confluence score (-10 to +10)
    pub confluence_score: i8,
    /// Snapshot of key indicator values
    pub indicator_snapshot: IndicatorSnapshot,
    /// Final trade advice after aggregation and safety checks
    pub trade_advice: Signal,
    /// Ingestion latency in milliseconds
    pub ingestion_latency_ms: i64,
    /// Reason if trade_advice was overridden by safety mechanisms
    pub override_reason: Option<String>,
}

impl SignalOutput {
    /// Check if the signal is reliable based on latency and override reason.
    pub fn is_reliable(&self, max_latency_ms: i64) -> bool {
        self.ingestion_latency_ms <= max_latency_ms
            && self.override_reason.as_deref() != Some("LATENCY_SPIKE")
    }

    /// Get the recommended action string.
    pub fn action(&self) -> &'static str {
        self.trade_advice.action()
    }
}

/// LLM Decision structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LLMDecision {
    /// Decision: "EXECUTE" or "WATCH"
    pub decision: String,
    /// Action: "LONG", "SHORT", or "NONE"
    pub action: String,
    /// Decision parameters
    pub params: DecisionParams,
}

impl LLMDecision {
    /// Check if decision is to execute.
    pub fn should_execute(&self) -> bool {
        self.decision == "EXECUTE" && self.action != "NONE"
    }

    /// Get the side for execution.
    pub fn side(&self) -> Option<&str> {
        match self.action.as_str() {
            "LONG" => Some("BUY"),
            "SHORT" => Some("SELL"),
            _ => None,
        }
    }
}

/// Parameters for LLM decision.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionParams {
    /// Leverage (1x, 2x, etc.)
    pub leverage: u32,
    /// Stop loss price
    pub stop_loss: f64,
    /// Take profit price
    pub take_profit: f64,
    /// Risk percentage of account (0.01 = 1%)
    pub risk_percent: f64,
}

/// Execution command to be pushed to Redis queue.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionCommand {
    /// Trading pair symbol
    pub symbol: String,
    /// Order side: "BUY" or "SELL"
    pub side: String,
    /// Order type: "MARKET" or "LIMIT"
    pub order_type: String,
    /// Order quantity
    pub quantity: f64,
    /// Stop loss price (optional)
    pub stop_loss: Option<f64>,
    /// Take profit price (optional)
    pub take_profit: Option<f64>,
    /// Timestamp in milliseconds
    pub timestamp_ms: i64,
    /// Market type: "SPOT" or "FUTURES"
    #[serde(default = "default_market_type")]
    pub market_type: String,
    /// Order price (for LIMIT orders)
    pub price: Option<f64>,
    /// Post-only for maker fee optimization
    #[serde(default)]
    pub post_only: bool,
}

fn default_market_type() -> String {
    "SPOT".to_string()
}

impl ExecutionCommand {
    /// Create a new execution command from LLM decision.
    pub fn from_decision(
        symbol: &str,
        decision: &LLMDecision,
        quantity: f64,
        _current_price: f64,
    ) -> Self {
        let side = decision.side().unwrap_or("BUY");
        
        // Calculate SL and TP based on side
        let (sl, tp) = if side == "BUY" {
            (decision.params.stop_loss, decision.params.take_profit)
        } else {
            // For short, SL should be above entry, TP below
            (decision.params.stop_loss, decision.params.take_profit)
        };

        Self {
            symbol: symbol.to_string(),
            side: side.to_string(),
            order_type: "MARKET".to_string(),
            quantity,
            stop_loss: Some(sl),
            take_profit: Some(tp),
            timestamp_ms: chrono::Utc::now().timestamp_millis(),
            market_type: default_market_type(),
            price: None,
            post_only: false,
        }
    }

    /// Serialize to JSON string for Redis.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Deserialize from JSON string.
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_signal_to_value() {
        assert_eq!(Signal::Long { strength: 0.8 }.to_value(), 0.8);
        assert_eq!(Signal::Short { strength: 0.5 }.to_value(), -0.5);
        assert_eq!(Signal::Wait.to_value(), 0.0);
    }

    #[test]
    fn test_signal_action() {
        assert_eq!(Signal::Long { strength: 0.8 }.action(), "LONG");
        assert_eq!(Signal::Short { strength: 0.5 }.action(), "SHORT");
        assert_eq!(Signal::Wait.action(), "WAIT");
    }

    #[test]
    fn test_market_regime_display() {
        assert_eq!(MarketRegime::Trending.to_string(), "TRENDING");
        assert_eq!(MarketRegime::Ranging.to_string(), "RANGING");
        assert_eq!(MarketRegime::Volatile.to_string(), "VOLATILE");
    }

    #[test]
    fn test_signal_output_is_reliable() {
        let signal = SignalOutput {
            price: 50000.0,
            timestamp_ms: 1000,
            market_regime: MarketRegime::Trending,
            confluence_score: 5,
            indicator_snapshot: IndicatorSnapshot::default(),
            trade_advice: Signal::long(0.8),
            ingestion_latency_ms: 100,
            override_reason: None,
        };

        assert!(signal.is_reliable(500));
        assert!(!signal.is_reliable(50)); // Latency too high
    }

    #[test]
    fn test_llm_decision_should_execute() {
        let decision = LLMDecision {
            decision: "EXECUTE".to_string(),
            action: "LONG".to_string(),
            params: DecisionParams {
                leverage: 1,
                stop_loss: 49000.0,
                take_profit: 52000.0,
                risk_percent: 0.02,
            },
        };

        assert!(decision.should_execute());

        let watch = LLMDecision {
            decision: "WATCH".to_string(),
            action: "NONE".to_string(),
            params: DecisionParams {
                leverage: 1,
                stop_loss: 0.0,
                take_profit: 0.0,
                risk_percent: 0.0,
            },
        };

        assert!(!watch.should_execute());
    }

    #[test]
    fn test_execution_command_serialization() {
        let cmd = ExecutionCommand {
            symbol: "BTCUSDT".to_string(),
            side: "BUY".to_string(),
            order_type: "MARKET".to_string(),
            quantity: 0.01,
            stop_loss: Some(49000.0),
            take_profit: Some(52000.0),
            timestamp_ms: 1000,
        };

        let json = cmd.to_json().unwrap();
        let parsed = ExecutionCommand::from_json(&json).unwrap();

        assert_eq!(parsed.symbol, "BTCUSDT");
        assert_eq!(parsed.side, "BUY");
        assert_eq!(parsed.quantity, 0.01);
    }
}
