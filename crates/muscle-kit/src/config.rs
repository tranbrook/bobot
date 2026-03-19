//! Muscle module configuration schema.
//!
//! This module defines the configuration structures for the Muscle signal engine,
//! including trading pairs, intervals, strategy weights, and performance settings.

use serde::{Deserialize, Serialize};
use schemars::JsonSchema;

/// Muscle signal engine configuration
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MuscleConfig {
    /// Enable/disable the muscle signal engine
    #[serde(default)]
    pub enabled: bool,

    /// Trading pairs to monitor (e.g., ["BTCUSDT", "ETHUSDT"])
    #[serde(default = "default_pairs")]
    pub pairs: Vec<String>,

    /// Time intervals (e.g., ["1m", "5m", "15m", "1h"])
    #[serde(default = "default_intervals")]
    pub intervals: Vec<String>,

    /// Redis connection URL (default: "redis://localhost:6379")
    #[serde(default = "default_redis_url")]
    pub redis_url: String,

    /// Strategy weights configuration
    #[serde(default)]
    pub strategy_weights: StrategyWeightsConfig,

    /// Ultimate Smoother parameters
    #[serde(default)]
    pub smoother: SmootherConfig,

    /// Performance and safety settings
    #[serde(default)]
    pub performance: MusclePerformanceConfig,
}

impl Default for MuscleConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            pairs: default_pairs(),
            intervals: default_intervals(),
            redis_url: default_redis_url(),
            strategy_weights: StrategyWeightsConfig::default(),
            smoother: SmootherConfig::default(),
            performance: MusclePerformanceConfig::default(),
        }
    }
}

/// Strategy weights configuration
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct StrategyWeightsConfig {
    /// Mean Reversion strategy base weight
    #[serde(default = "default_mean_reversion_weight")]
    pub mean_reversion: f64,

    /// Trend Following strategy base weight
    #[serde(default = "default_trend_following_weight")]
    pub trend_following: f64,
}

impl Default for StrategyWeightsConfig {
    fn default() -> Self {
        Self {
            mean_reversion: default_mean_reversion_weight(),
            trend_following: default_trend_following_weight(),
        }
    }
}

/// Ultimate Smoother configuration
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SmootherConfig {
    /// Filter coefficient c1
    #[serde(default = "default_c1")]
    pub c1: f64,

    /// Filter coefficient c2
    #[serde(default = "default_c2")]
    pub c2: f64,

    /// Filter coefficient c3
    #[serde(default = "default_c3")]
    pub c3: f64,
}

impl Default for SmootherConfig {
    fn default() -> Self {
        Self {
            c1: default_c1(),
            c2: default_c2(),
            c3: default_c3(),
        }
    }
}

/// Performance and safety configuration
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MusclePerformanceConfig {
    /// Maximum allowed ingestion latency in milliseconds (default: 500ms)
    /// If exceeded, trade_advice is forced to WAIT with LATENCY_SPIKE reason
    #[serde(default = "default_max_allowed_latency_ms")]
    pub max_allowed_latency_ms: i64,

    /// Number of warm-up candles for indicator convergence (default: 100)
    #[serde(default = "default_warmup_candles")]
    pub warmup_candles: usize,

    /// Total candles to fetch from REST API (warmup + working window, default: 1100)
    #[serde(default = "default_total_candles")]
    pub total_candles: usize,
}

impl Default for MusclePerformanceConfig {
    fn default() -> Self {
        Self {
            max_allowed_latency_ms: default_max_allowed_latency_ms(),
            warmup_candles: default_warmup_candles(),
            total_candles: default_total_candles(),
        }
    }
}

// Default value functions

fn default_pairs() -> Vec<String> {
    vec!["BTCUSDT".to_string()]
}

fn default_intervals() -> Vec<String> {
    vec!["1m".to_string(), "5m".to_string()]
}

fn default_redis_url() -> String {
    "redis://localhost:6379".to_string()
}

fn default_mean_reversion_weight() -> f64 {
    0.5
}

fn default_trend_following_weight() -> f64 {
    0.5
}

fn default_c1() -> f64 {
    0.07
}

fn default_c2() -> f64 {
    0.05
}

fn default_c3() -> f64 {
    0.03
}

fn default_max_allowed_latency_ms() -> i64 {
    500
}

fn default_warmup_candles() -> usize {
    100
}

fn default_total_candles() -> usize {
    1100
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_muscle_config_default() {
        let config = MuscleConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.pairs, vec!["BTCUSDT"]);
        assert_eq!(config.intervals, vec!["1m", "5m"]);
        assert_eq!(config.redis_url, "redis://localhost:6379");
    }

    #[test]
    fn test_strategy_weights_default() {
        let weights = StrategyWeightsConfig::default();
        assert!((weights.mean_reversion - 0.5).abs() < 0.01);
        assert!((weights.trend_following - 0.5).abs() < 0.01);
    }

    #[test]
    fn test_smoother_config_default() {
        let smoother = SmootherConfig::default();
        assert!((smoother.c1 - 0.07).abs() < 0.01);
        assert!((smoother.c2 - 0.05).abs() < 0.01);
        assert!((smoother.c3 - 0.03).abs() < 0.01);
    }

    #[test]
    fn test_performance_config_default() {
        let perf = MusclePerformanceConfig::default();
        assert_eq!(perf.max_allowed_latency_ms, 500);
        assert_eq!(perf.warmup_candles, 100);
        assert_eq!(perf.total_candles, 1100);
    }

    #[test]
    fn test_muscle_config_serialization() {
        let config = MuscleConfig::default();
        let json = serde_json::to_string(&config).unwrap();

        assert!(json.contains("\"enabled\":false"));
        assert!(json.contains("\"pairs\""));
        assert!(json.contains("\"redis_url\""));
    }

    #[test]
    fn test_muscle_config_deserialization() {
        let json = r#"{
            "enabled": true,
            "pairs": ["BTCUSDT", "ETHUSDT"],
            "intervals": ["5m"],
            "redis_url": "redis://redis:6379"
        }"#;

        let config: MuscleConfig = serde_json::from_str(json).unwrap();
        assert!(config.enabled);
        assert_eq!(config.pairs, vec!["BTCUSDT", "ETHUSDT"]);
        assert_eq!(config.intervals, vec!["5m"]);
        assert_eq!(config.redis_url, "redis://redis:6379");
    }
}
