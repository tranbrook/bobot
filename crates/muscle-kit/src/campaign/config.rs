//! Campaign configuration for multi-symbol trading campaigns

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Campaign configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CampaignConfig {
    /// Campaign name
    pub name: String,
    
    /// Campaign description
    #[serde(default)]
    pub description: Option<String>,
    
    /// Enable/disable campaign
    #[serde(default = "default_true")]
    pub enabled: bool,
    
    /// List of symbols to trade
    pub symbols: Vec<String>,
    
    /// Symbol-specific intervals (optional, defaults to default_interval)
    #[serde(default)]
    pub symbol_intervals: HashMap<String, String>,
    
    /// Default interval for symbols not in symbol_intervals
    #[serde(default = "default_interval")]
    pub default_interval: String,
    
    /// Strategy configuration
    #[serde(default)]
    pub strategies: StrategyConfig,
    
    /// Risk management configuration
    #[serde(default)]
    pub risk: RiskConfig,
    
    /// Correlation checking configuration
    #[serde(default)]
    pub correlation: CorrelationConfig,
}

/// Strategy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyConfig {
    /// Enable trend following strategy
    #[serde(default = "default_true")]
    pub trend_following: bool,
    
    /// Trend following weight
    #[serde(default = "default_weight")]
    pub trend_following_weight: f64,
    
    /// Enable mean reversion strategy
    #[serde(default)]
    pub mean_reversion: bool,
    
    /// Mean reversion weight
    #[serde(default)]
    pub mean_reversion_weight: f64,
}

/// Risk management configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskConfig {
    /// Maximum total position size in USD for the entire campaign
    #[serde(default = "default_max_total_position")]
    pub max_total_position_usd: f64,
    
    /// Maximum position size per symbol in USD
    #[serde(default = "default_max_per_symbol")]
    pub max_per_symbol_usd: f64,
    
    /// Maximum daily trades for the entire campaign
    #[serde(default = "default_max_daily_trades")]
    pub max_daily_trades: usize,
    
    /// Maximum daily trades per symbol
    #[serde(default = "default_max_daily_trades_per_symbol")]
    pub max_daily_trades_per_symbol: usize,
    
    /// Emergency PnL threshold (negative percentage)
    #[serde(default = "default_emergency_pnl")]
    pub emergency_pnl_threshold: f64,
    
    /// Maximum single symbol ratio of total risk
    #[serde(default = "default_max_single_symbol_ratio")]
    pub max_single_symbol_ratio: f64,
}

/// Correlation checking configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorrelationConfig {
    /// Enable correlation checking
    #[serde(default)]
    pub enabled: bool,
    
    /// Maximum correlation threshold (0.0-1.0)
    #[serde(default = "default_max_correlation")]
    pub max_correlation_threshold: f64,
    
    /// Maximum number of correlated symbols to trade simultaneously
    #[serde(default = "default_max_correlated_symbols")]
    pub max_correlated_symbols: usize,
}

fn default_true() -> bool { true }
fn default_interval() -> String { "1m".to_string() }
fn default_weight() -> f64 { 0.5 }
fn default_max_total_position() -> f64 { 10000.0 }
fn default_max_per_symbol() -> f64 { 2500.0 }
fn default_max_daily_trades() -> usize { 20 }
fn default_max_daily_trades_per_symbol() -> usize { 5 }
fn default_emergency_pnl() -> f64 { -5.0 }
fn default_max_single_symbol_ratio() -> f64 { 0.5 }
fn default_max_correlation() -> f64 { 0.8 }
fn default_max_correlated_symbols() -> usize { 2 }

impl Default for CampaignConfig {
    fn default() -> Self {
        Self {
            name: "default".to_string(),
            description: None,
            enabled: true,
            symbols: vec!["BTCUSDT".to_string()],
            symbol_intervals: HashMap::new(),
            default_interval: default_interval(),
            strategies: StrategyConfig::default(),
            risk: RiskConfig::default(),
            correlation: CorrelationConfig::default(),
        }
    }
}

impl Default for StrategyConfig {
    fn default() -> Self {
        Self {
            trend_following: true,
            trend_following_weight: default_weight(),
            mean_reversion: false,
            mean_reversion_weight: 0.0,
        }
    }
}

impl Default for RiskConfig {
    fn default() -> Self {
        Self {
            max_total_position_usd: default_max_total_position(),
            max_per_symbol_usd: default_max_per_symbol(),
            max_daily_trades: default_max_daily_trades(),
            max_daily_trades_per_symbol: default_max_daily_trades_per_symbol(),
            emergency_pnl_threshold: default_emergency_pnl(),
            max_single_symbol_ratio: default_max_single_symbol_ratio(),
        }
    }
}

impl Default for CorrelationConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            max_correlation_threshold: default_max_correlation(),
            max_correlated_symbols: default_max_correlated_symbols(),
        }
    }
}
