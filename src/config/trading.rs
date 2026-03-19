//! Trading configuration for ZeroClaw Agent.
//!
//! This module provides configuration for the trading integration.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

// Import TradingSafetyConfig from safety_layer
// We re-export it here to avoid circular dependencies
pub use crate::agent::trading::safety_layer::TradingSafetyConfig;

/// Trading provider configuration
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TradingProviderConfig {
    /// Provider name (e.g., "anthropic", "openai", "openrouter", "ollama")
    #[serde(default = "default_provider_name")]
    pub name: String,
    
    /// Model to use for trading decisions
    #[serde(default = "default_model")]
    pub model: String,
    
    /// Temperature for LLM (0.0-2.0)
    #[serde(default = "default_temperature")]
    pub temperature: f64,
    
    /// Max tokens for response
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
    
    /// Timeout in seconds
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u64,
}

fn default_provider_name() -> String { "anthropic".to_string() }
fn default_model() -> String { "claude-sonnet-4-20250514".to_string() }
fn default_temperature() -> f64 { 0.7 }
fn default_max_tokens() -> u32 { 1000 }
fn default_timeout_secs() -> u64 { 30 }

impl Default for TradingProviderConfig {
    fn default() -> Self {
        Self {
            name: default_provider_name(),
            model: default_model(),
            temperature: default_temperature(),
            max_tokens: default_max_tokens(),
            timeout_secs: default_timeout_secs(),
        }
    }
}

/// Trading memory configuration
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TradingMemoryConfig {
    /// Enable trading memory
    #[serde(default)]
    pub enabled: bool,
    
    /// Store decisions
    #[serde(default = "default_true")]
    pub store_decisions: bool,
    
    /// Store signals
    #[serde(default = "default_true")]
    pub store_signals: bool,
    
    /// Recall limit
    #[serde(default = "default_recall_limit")]
    pub recall_limit: usize,
}

fn default_true() -> bool { true }
fn default_recall_limit() -> usize { 10 }

impl Default for TradingMemoryConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            store_decisions: default_true(),
            store_signals: default_true(),
            recall_limit: default_recall_limit(),
        }
    }
}

/// Trading prompt configuration
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TradingPromptConfig {
    /// Custom system prompt
    #[serde(default = "default_system_prompt")]
    pub system_prompt: String,
}

fn default_system_prompt() -> String {
    r#"You are a professional cryptocurrency trader with 10+ years of experience.
You specialize in technical analysis and risk management.

Rules:
1. Never risk more than 2% per trade
2. Always use stop loss
3. Only trade when confluence score >= 7
4. Prefer trending markets (ADX > 25)
5. Avoid trading during high volatility unless regime is clearly trending

Return decisions in JSON format with clear reasoning."#.to_string()
}

impl Default for TradingPromptConfig {
    fn default() -> Self {
        Self {
            system_prompt: default_system_prompt(),
        }
    }
}

/// Top-level trading configuration
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TradingConfig {
    /// Enable trading integration
    #[serde(default)]
    pub enabled: bool,
    
    /// Symbols to monitor
    #[serde(default = "default_symbols")]
    pub symbols: Vec<String>,
    
    /// Check interval in milliseconds
    #[serde(default = "default_check_interval_ms")]
    pub check_interval_ms: u64,
    
    /// Redis URL
    #[serde(default = "default_redis_url")]
    pub redis_url: String,
    
    /// Provider configuration
    #[serde(default)]
    pub provider: TradingProviderConfig,
    
    /// Risk configuration
    #[serde(default)]
    pub risk: TradingSafetyConfig,
    
    /// Safety configuration (alias for risk)
    #[serde(default, alias = "safety")]
    pub safety: TradingSafetyConfig,
    
    /// Memory configuration
    #[serde(default)]
    pub memory: TradingMemoryConfig,
    
    /// Prompt configuration
    #[serde(default)]
    pub prompt: TradingPromptConfig,
}

fn default_symbols() -> Vec<String> {
    vec!["BTCUSDT".to_string(), "ETHUSDT".to_string()]
}

fn default_check_interval_ms() -> u64 { 5000 }

fn default_redis_url() -> String {
    "redis://localhost:6379".to_string()
}

impl Default for TradingConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            symbols: default_symbols(),
            check_interval_ms: default_check_interval_ms(),
            redis_url: default_redis_url(),
            provider: TradingProviderConfig::default(),
            risk: TradingSafetyConfig::default(),
            safety: TradingSafetyConfig::default(),
            memory: TradingMemoryConfig::default(),
            prompt: TradingPromptConfig::default(),
        }
    }
}
