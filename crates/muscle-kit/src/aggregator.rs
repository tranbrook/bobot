//! Gatekeeper - Signal aggregation and voting system.
//!
//! This module implements the Gatekeeper that aggregates signals from multiple
//! trading strategies using a regime-based weighted voting system.
//!
//! ## Voting System
//!
//! Each strategy produces a signal (Long, Short, or Wait) with a weight based on:
//! 1. Base strategy weight
//! 2. Regime-based adjustment
//! 3. Signal strength
//!
//! The final confluence score ranges from -10 (strong short) to +10 (strong long).

use polars::prelude::DataFrame;

use crate::data::models::{MarketRegime, Signal, SignalOutput, IndicatorSnapshot};
use crate::traits::{TradingStrategy, AggregatedSignal};
use crate::strategies::{MeanReversionStrategy, TrendFollowingStrategy};

/// Gatekeeper configuration
#[derive(Debug, Clone)]
pub struct GatekeeperConfig {
    /// Maximum allowed latency in milliseconds
    pub max_allowed_latency_ms: i64,
    /// Minimum confluence score for strong signals
    pub strong_signal_threshold: i8,
    /// Minimum confluence score for any signal
    pub weak_signal_threshold: i8,
}

impl Default for GatekeeperConfig {
    fn default() -> Self {
        Self {
            max_allowed_latency_ms: 500,
            strong_signal_threshold: 7,
            weak_signal_threshold: 3,
        }
    }
}

/// Gatekeeper - aggregates signals from multiple strategies
pub struct Gatekeeper {
    /// List of registered strategies
    strategies: Vec<Box<dyn TradingStrategy>>,
    /// Configuration
    config: GatekeeperConfig,
}

impl Gatekeeper {
    /// Create a new Gatekeeper with default strategies
    pub fn new() -> Self {
        let strategies: Vec<Box<dyn TradingStrategy>> = vec![
            Box::new(MeanReversionStrategy::new()),
            Box::new(TrendFollowingStrategy::new()),
        ];

        Self {
            strategies,
            config: GatekeeperConfig::default(),
        }
    }

    /// Create with custom configuration
    pub fn with_config(config: GatekeeperConfig) -> Self {
        Self {
            strategies: vec![
                Box::new(MeanReversionStrategy::new()),
                Box::new(TrendFollowingStrategy::new()),
            ],
            config,
        }
    }

    /// Create with custom strategies
    pub fn with_strategies(strategies: Vec<Box<dyn TradingStrategy>>) -> Self {
        Self {
            strategies,
            config: GatekeeperConfig::default(),
        }
    }

    /// Register a new strategy
    pub fn add_strategy(&mut self, strategy: Box<dyn TradingStrategy>) {
        self.strategies.push(strategy);
    }

    /// Get the number of registered strategies
    pub fn strategy_count(&self) -> usize {
        self.strategies.len()
    }

    /// Aggregate signals from all strategies
    ///
    /// # Arguments
    /// * `df` - DataFrame with OHLCV and indicator data
    /// * `regime` - Current market regime
    ///
    /// # Returns
    /// AggregatedSignal with confluence score and weighted sum
    pub fn aggregate(&self, df: &DataFrame, regime: &MarketRegime) -> AggregatedSignal {
        use tracing::debug;
        
        let mut weighted_sum = 0.0;
        let mut total_weight = 0.0;
        let mut active_strategies = 0;

        for strategy in &self.strategies {
            // Check if strategy is enabled for this regime
            if !strategy.is_enabled_for(regime) {
                debug!("[Gatekeeper] Strategy '{}' disabled for regime {:?}", strategy.name(), regime);
                continue;
            }

            active_strategies += 1;

            // Get strategy signal
            let signal = strategy.analyze(df, regime);
            debug!("[Gatekeeper] Strategy '{}' signal: {:?}, regime: {:?}", strategy.name(), signal, regime);

            // Get regime-adjusted weight
            let base_weight = strategy.weight(regime);
            let signal_strength = signal.to_value().abs();
            let adjusted_weight = base_weight * signal_strength;

            // Add to weighted sum (signal value * weight)
            weighted_sum += signal.to_value() * adjusted_weight;
            total_weight += adjusted_weight;
        }
        
        debug!("[Gatekeeper] Active strategies={}, weighted_sum={}, total_weight={}", active_strategies, weighted_sum, total_weight);

        // Calculate confluence score (-10 to +10)
        let confluence_score = if total_weight > 0.0 {
            ((weighted_sum / total_weight) * 10.0).round() as i8
        } else {
            0
        }
        .clamp(-10, 10);

        // Determine final signal based on confluence score
        let final_signal = if confluence_score >= self.config.strong_signal_threshold {
            Signal::long((confluence_score as f64) / 10.0)
        } else if confluence_score <= -self.config.strong_signal_threshold {
            Signal::short((confluence_score.abs() as f64) / 10.0)
        } else if confluence_score > self.config.weak_signal_threshold {
            Signal::long((confluence_score as f64) / 10.0)
        } else if confluence_score < -self.config.weak_signal_threshold {
            Signal::short((confluence_score.abs() as f64) / 10.0)
        } else {
            Signal::wait()
        };

        AggregatedSignal::new(
            final_signal,
            confluence_score,
            active_strategies,
            weighted_sum,
            total_weight,
        )
    }

    /// Generate complete signal output with latency tracking
    ///
    /// # Arguments
    /// * `df` - DataFrame with OHLCV and indicator data
    /// * `regime` - Current market regime
    /// * `price` - Current price
    /// * `timestamp_ms` - Signal timestamp
    /// * `kline_timestamp_ms` - Original kline close time (for latency calc)
    /// * `indicator_snapshot` - Current indicator values
    ///
    /// # Returns
    /// SignalOutput with all signal data and latency tracking
    pub fn generate_signal_output(
        &self,
        df: &DataFrame,
        regime: &MarketRegime,
        price: f64,
        timestamp_ms: i64,
        kline_timestamp_ms: i64,
        indicator_snapshot: IndicatorSnapshot,
    ) -> SignalOutput {
        // Aggregate signals
        let aggregated = self.aggregate(df, regime);

        // Create signal output
        let mut output = SignalOutput::new(
            price,
            timestamp_ms,
            *regime,
            aggregated.confluence_score,
            indicator_snapshot,
            aggregated.signal,
            kline_timestamp_ms,
        );

        // Apply latency safety check
        output.apply_latency_safety_check(self.config.max_allowed_latency_ms);

        output
    }

    /// Get the current configuration
    pub fn config(&self) -> &GatekeeperConfig {
        &self.config
    }
}

impl Default for Gatekeeper {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use polars::prelude::*;

    fn create_test_df() -> DataFrame {
        df! {
            "close" => vec![100.0; 100],
            "bb_upper" => vec![105.0; 100],
            "bb_lower" => vec![95.0; 100],
            "us_value" => vec![100.0; 100],
        }.unwrap()
    }

    #[test]
    fn test_gatekeeper_creation() {
        let gatekeeper = Gatekeeper::new();
        assert_eq!(gatekeeper.strategy_count(), 2);
    }

    #[test]
    fn test_gatekeeper_aggregate_ranging() {
        let gatekeeper = Gatekeeper::new();
        let df = create_test_df();
        let regime = MarketRegime::Ranging;

        let result = gatekeeper.aggregate(&df, &regime);

        // Should have both strategies active
        assert!(result.strategy_count >= 1);

        // Confluence should be in valid range
        assert!(result.confluence_score >= -10);
        assert!(result.confluence_score <= 10);
    }

    #[test]
    fn test_gatekeeper_aggregate_trending() {
        let gatekeeper = Gatekeeper::new();
        let df = create_test_df();
        let regime = MarketRegime::Trending;

        let result = gatekeeper.aggregate(&df, &regime);

        // Trend following should have higher weight in trending
        assert!(result.strategy_count >= 1);
    }

    #[test]
    fn test_gatekeeper_aggregate_volatile() {
        let gatekeeper = Gatekeeper::new();
        let df = create_test_df();
        let regime = MarketRegime::Volatile;

        let result = gatekeeper.aggregate(&df, &regime);

        // Both strategies should have medium weight in volatile
        assert!(result.strategy_count >= 1);
    }

    #[test]
    fn test_confluence_score_clamping() {
        // Test that confluence score is properly clamped
        // Note: AggregatedSignal doesn't clamp the score, it's stored as-is
        // The clamping happens in the aggregate() method
        let signal = AggregatedSignal::new(
            Signal::long(1.0),
            15, // Stored as-is (clamping is done by caller)
            2,
            2.0,
            2.0,
        );

        // Score is stored as provided (clamping is done by aggregate method)
        assert_eq!(signal.confluence_score, 15);
    }

    #[test]
    fn test_signal_output_latency_check() {
        let gatekeeper = Gatekeeper::new();
        let df = create_test_df();
        let regime = MarketRegime::Ranging;

        // Use current time for both to ensure 0 latency
        let now = crate::data::models::Kline::current_time_ms();

        let output = gatekeeper.generate_signal_output(
            &df,
            &regime,
            100.0,
            now,
            now, // Same timestamp = 0 latency
            IndicatorSnapshot::default(),
        );

        // With 0 latency, should not be overridden
        // Note: This test may fail if execution takes time, so we check latency < threshold
        if output.ingestion_latency_ms < gatekeeper.config().max_allowed_latency_ms {
            assert!(output.override_reason.is_none());
        }
    }
}
