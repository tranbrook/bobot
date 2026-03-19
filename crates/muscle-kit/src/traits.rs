//! Core traits for the Muscle signal engine.
//!
//! This module defines the core trait abstractions used throughout
//! the muscle module for strategy implementation and signal generation.

use polars::prelude::DataFrame;
use async_trait::async_trait;

use crate::data::models::{MarketRegime, Signal};

/// Trading strategy trait.
///
/// Implement this trait to create custom trading strategies.
/// Each strategy analyzes the DataFrame and produces a trading signal
/// based on the current market regime.
#[async_trait]
pub trait TradingStrategy: Send + Sync {
    /// Strategy name (used for logging and identification)
    fn name(&self) -> &str;

    /// Human-readable description of the strategy
    fn description(&self) -> &str;

    /// Analyze the DataFrame and return a trading signal.
    ///
    /// # Arguments
    /// * `df` - DataFrame containing OHLCV data and calculated indicators
    /// * `regime` - Current market regime (Trending, Ranging, Volatile)
    ///
    /// # Returns
    /// A Signal (Long, Short, or Wait) with optional strength value
    fn analyze(&self, df: &DataFrame, regime: &MarketRegime) -> Signal;

    /// Get the base weight for this strategy.
    ///
    /// The weight determines how much influence this strategy has
    /// on the final aggregated signal. Weights are adjusted dynamically
    /// based on the market regime.
    ///
    /// # Arguments
    /// * `regime` - Current market regime
    ///
    /// # Returns
    /// Weight value (typically 0.0 to 1.0)
    fn weight(&self, regime: &MarketRegime) -> f64 {
        // Default: equal weight for all regimes
        match regime {
            MarketRegime::Trending => 0.5,
            MarketRegime::Ranging => 0.5,
            MarketRegime::Volatile => 0.5,
        }
    }

    /// Check if this strategy is enabled for the given regime.
    ///
    /// Some strategies may only be appropriate for certain market conditions.
    ///
    /// # Arguments
    /// * `regime` - Current market regime
    ///
    /// # Returns
    /// true if the strategy should be active for this regime
    fn is_enabled_for(&self, _regime: &MarketRegime) -> bool {
        true // Enabled for all regimes by default
    }
}

/// Signal aggregation result.
///
/// Contains the aggregated signal from multiple strategies
/// along with metadata about the aggregation process.
#[derive(Debug, Clone)]
pub struct AggregatedSignal {
    /// Final aggregated signal
    pub signal: Signal,
    /// Confluence score (-10 to +10)
    pub confluence_score: i8,
    /// Number of strategies contributing to the signal
    pub strategy_count: usize,
    /// Weighted sum of all strategy signals
    pub weighted_sum: f64,
    /// Total weight applied
    pub total_weight: f64,
}

impl AggregatedSignal {
    /// Create a new aggregated signal
    pub fn new(
        signal: Signal,
        confluence_score: i8,
        strategy_count: usize,
        weighted_sum: f64,
        total_weight: f64,
    ) -> Self {
        Self {
            signal,
            confluence_score,
            strategy_count,
            weighted_sum,
            total_weight,
        }
    }

    /// Get the average signal strength
    pub fn average_strength(&self) -> f64 {
        if self.total_weight > 0.0 {
            self.weighted_sum / self.total_weight
        } else {
            0.0
        }
    }

    /// Check if the signal is strong (absolute confluence >= 7)
    pub fn is_strong(&self) -> bool {
        self.confluence_score.abs() >= 7
    }

    /// Check if the signal is weak (absolute confluence <= 3)
    pub fn is_weak(&self) -> bool {
        self.confluence_score.abs() <= 3
    }
}

impl Default for AggregatedSignal {
    fn default() -> Self {
        Self {
            signal: Signal::Wait,
            confluence_score: 0,
            strategy_count: 0,
            weighted_sum: 0.0,
            total_weight: 0.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_aggregated_signal_default() {
        let signal = AggregatedSignal::default();
        assert!(matches!(signal.signal, Signal::Wait));
        assert_eq!(signal.confluence_score, 0);
        assert_eq!(signal.strategy_count, 0);
        assert_eq!(signal.weighted_sum, 0.0);
        assert_eq!(signal.total_weight, 0.0);
    }

    #[test]
    fn test_aggregated_signal_strength() {
        let signal = AggregatedSignal::new(
            Signal::long(0.8),
            7,
            2,
            1.5,
            2.0,
        );

        assert!((signal.average_strength() - 0.75).abs() < 0.01);
        assert!(signal.is_strong());
        assert!(!signal.is_weak());
    }

    #[test]
    fn test_aggregated_signal_weak() {
        let signal = AggregatedSignal::new(
            Signal::long(0.3),
            2,
            1,
            0.3,
            1.0,
        );

        assert!(!signal.is_strong());
        assert!(signal.is_weak());
    }
}
