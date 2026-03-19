//! Strategy A: Mean Reversion using UltimateBands exhaustion.
//!
//! This strategy identifies overextended price movements and trades
//! the reversion to the mean (Ultimate Smoother center line).
//!
//! ## Entry Conditions (Long)
//! - Price touches or breaks below UltimateBands Lower
//! - RSI < 30 (oversold)
//! - Market Regime: RANGING (preferred) or VOLATILE
//!
//! ## Entry Conditions (Short)
//! - Price touches or breaks above UltimateBands Upper
//! - RSI > 70 (overbought)
//! - Market Regime: RANGING (preferred) or VOLATILE
//!
//! ## Exit Conditions
//! - Price crosses back above/below Ultimate Smoother center line
//! - Stop loss: 2x ATR from entry

use polars::prelude::DataFrame;

use crate::data::models::{MarketRegime, Signal};
use crate::traits::TradingStrategy;
use crate::engine::bands::calculate_band_exhaustion;

/// RSI period for overbought/oversold detection
pub const RSI_PERIOD: usize = 14;

/// RSI oversold threshold (reduced from 30.0 for more signals)
pub const RSI_OVERSOLD: f64 = 35.0;

/// RSI overbought threshold (reduced from 70.0 for more signals)
pub const RSI_OVERBOUGHT: f64 = 65.0;

/// Minimum exhaustion level for entry (0.0 to 1.0)
pub const MIN_EXHAUSTION_LEVEL: f64 = 0.2;

/// Mean Reversion Strategy
pub struct MeanReversionStrategy {
    /// Strategy name
    name: String,
    /// Enable for ranging markets
    enable_ranging: bool,
    /// Enable for volatile markets
    enable_volatile: bool,
    /// Enable for trending markets (not recommended)
    enable_trending: bool,
}

impl MeanReversionStrategy {
    /// Create a new Mean Reversion strategy with default settings
    pub fn new() -> Self {
        Self {
            name: "Mean Reversion".to_string(),
            enable_ranging: true,
            enable_volatile: true,
            enable_trending: false, // Mean reversion performs poorly in trends
        }
    }

    /// Create with custom settings
    pub fn with_config(
        enable_ranging: bool,
        enable_volatile: bool,
        enable_trending: bool,
    ) -> Self {
        Self {
            name: "Mean Reversion".to_string(),
            enable_ranging,
            enable_volatile,
            enable_trending,
        }
    }

    /// Calculate RSI from close prices
    fn calculate_rsi(close: &[f64], period: usize) -> Vec<Option<f64>> {
        let n = close.len();
        let mut rsi = Vec::with_capacity(n);

        if n < period + 1 {
            return vec![None; n];
        }

        // Calculate price changes
        let mut gains = Vec::with_capacity(n);
        let mut losses = Vec::with_capacity(n);

        gains.push(0.0);
        losses.push(0.0);

        for i in 1..n {
            let change = close[i] - close[i - 1];
            if change > 0.0 {
                gains.push(change);
                losses.push(0.0);
            } else {
                gains.push(0.0);
                losses.push(-change);
            }
        }

        // First average gain/loss (simple average)
        let mut avg_gain: f64 = gains[..=period].iter().sum::<f64>() / period as f64;
        let mut avg_loss: f64 = losses[..=period].iter().sum::<f64>() / period as f64;

        // First RSI
        for i in 0..=period {
            rsi.push(None);
        }

        // Calculate RSI using Wilder's smoothing
        for i in (period + 1)..n {
            avg_gain = (avg_gain * (period - 1) as f64 + gains[i]) / period as f64;
            avg_loss = (avg_loss * (period - 1) as f64 + losses[i]) / period as f64;

            let rs = if avg_loss.abs() > 1e-10 {
                avg_gain / avg_loss
            } else {
                100.0
            };

            let rsi_val = 100.0 - (100.0 / (1.0 + rs));
            rsi.push(Some(rsi_val));
        }

        rsi
    }

    /// Get the latest RSI value
    fn get_latest_rsi(close: &[f64]) -> Option<f64> {
        let rsi = Self::calculate_rsi(close, RSI_PERIOD);
        rsi.last().copied().flatten()
    }

    /// Analyze band exhaustion for signal generation
    fn analyze_exhaustion(
        price: f64,
        bb_upper: f64,
        bb_lower: f64,
        rsi: Option<f64>,
    ) -> Signal {
        // Calculate exhaustion level
        let exhaustion = calculate_band_exhaustion(price, bb_upper, bb_lower, (bb_upper + bb_lower) / 2.0);

        // Check for long signal (price below lower band, oversold)
        if exhaustion < -MIN_EXHAUSTION_LEVEL {
            if let Some(rsi_val) = rsi {
                if rsi_val < RSI_OVERSOLD {
                    // Strength based on how oversold
                    let strength = ((RSI_OVERSOLD - rsi_val) / RSI_OVERSOLD).min(1.0);
                    return Signal::long(strength);
                }
            }
        }

        // Check for short signal (price above upper band, overbought)
        if exhaustion > MIN_EXHAUSTION_LEVEL {
            if let Some(rsi_val) = rsi {
                if rsi_val > RSI_OVERBOUGHT {
                    // Strength based on how overbought
                    let strength = ((rsi_val - RSI_OVERBOUGHT) / (100.0 - RSI_OVERBOUGHT)).min(1.0);
                    return Signal::short(strength);
                }
            }
        }

        Signal::wait()
    }
}

impl Default for MeanReversionStrategy {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl TradingStrategy for MeanReversionStrategy {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        "Mean reversion strategy using UltimateBands exhaustion and RSI for overbought/oversold detection"
    }

    fn analyze(&self, df: &DataFrame, regime: &MarketRegime) -> Signal {
        use tracing::debug;
        
        // Check if strategy is enabled for this regime
        if !self.is_enabled_for(regime) {
            debug!("[MeanReversion] Disabled for regime: {:?}", regime);
            return Signal::wait();
        }

        // Get latest values from DataFrame
        let close_col = match df.column("close") {
            Ok(col) => col,
            Err(e) => {
                debug!("[MeanReversion] Missing 'close' column: {:?}", e);
                return Signal::wait();
            }
        };

        let bb_upper_col = match df.column("bb_upper") {
            Ok(col) => col,
            Err(e) => {
                debug!("[MeanReversion] Missing 'bb_upper' column: {:?}", e);
                return Signal::wait();
            }
        };

        let bb_lower_col = match df.column("bb_lower") {
            Ok(col) => col,
            Err(e) => {
                debug!("[MeanReversion] Missing 'bb_lower' column: {:?}", e);
                return Signal::wait();
            }
        };

        // Get latest values
        let last_idx = df.height().saturating_sub(1);
        
        let price = close_col.get(last_idx).ok()
            .and_then(|v| v.extract::<f64>())
            .unwrap_or(0.0);
        let bb_upper = bb_upper_col.get(last_idx).ok()
            .and_then(|v| v.extract::<f64>())
            .unwrap_or(0.0);
        let bb_lower = bb_lower_col.get(last_idx).ok()
            .and_then(|v| v.extract::<f64>())
            .unwrap_or(0.0);

        // Get close prices as slice for RSI calculation
        let close: Vec<f64> = close_col.f64()
            .ok()
            .map(|ca| ca.into_iter().filter_map(|v| v).collect())
            .unwrap_or_default();

        let rsi = Self::get_latest_rsi(&close);

        // Analyze exhaustion
        let result = Self::analyze_exhaustion(price, bb_upper, bb_lower, rsi);
        tracing::info!("[MeanReversion] price={:.4}, bb_upper={:.4}, bb_lower={:.4}, rsi={:?}, exhaustion result={:?}", price, bb_upper, bb_lower, rsi, result);
        result
    }

    fn weight(&self, regime: &MarketRegime) -> f64 {
        // Mean reversion performs best in ranging markets
        match regime {
            MarketRegime::Ranging => 0.7,    // High weight in ranging
            MarketRegime::Volatile => 0.5,   // Medium weight in volatile
            MarketRegime::Trending => 0.2,   // Low weight in trending
        }
    }

    fn is_enabled_for(&self, regime: &MarketRegime) -> bool {
        match regime {
            MarketRegime::Ranging => self.enable_ranging,
            MarketRegime::Volatile => self.enable_volatile,
            MarketRegime::Trending => self.enable_trending,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use polars::prelude::*;

    fn create_test_df() -> DataFrame {
        // Create test data with ranging market characteristics
        let close: Vec<f64> = vec![
            100.0, 101.0, 99.0, 100.0, 102.0, 98.0, 100.0, 101.0, 99.0, 100.0,
            95.0, 94.0, 93.0, 92.0, 91.0, // Dip for oversold condition
        ];

        let bb_upper: Vec<f64> = vec![
            105.0; 16
        ];
        let bb_lower: Vec<f64> = vec![
            95.0; 16
        ];

        df! {
            "close" => close,
            "bb_upper" => bb_upper,
            "bb_lower" => bb_lower,
        }.unwrap()
    }

    #[test]
    fn test_mean_reversion_strategy_creation() {
        let strategy = MeanReversionStrategy::new();
        assert_eq!(strategy.name(), "Mean Reversion");
        assert!(strategy.is_enabled_for(&MarketRegime::Ranging));
        assert!(strategy.is_enabled_for(&MarketRegime::Volatile));
        assert!(!strategy.is_enabled_for(&MarketRegime::Trending));
    }

    #[test]
    fn test_rsi_calculation() {
        // Create steadily increasing prices (should give high RSI)
        let close: Vec<f64> = (0..20).map(|i| 100.0 + i as f64).collect();
        let rsi = MeanReversionStrategy::calculate_rsi(&close, 14);

        // RSI should have values after period
        assert!(rsi.iter().skip(15).any(|v| v.is_some()));

        // RSI should be high for rising prices
        if let Some(Some(last_rsi)) = rsi.last() {
            assert!(*last_rsi > 50.0);
        }
    }

    #[test]
    fn test_rsi_oversold() {
        // Create steadily decreasing prices (should give low RSI)
        let close: Vec<f64> = (0..20).rev().map(|i| 100.0 + i as f64).collect();
        let rsi = MeanReversionStrategy::calculate_rsi(&close, 14);

        // RSI should be low for falling prices
        if let Some(Some(last_rsi)) = rsi.last() {
            assert!(*last_rsi < 50.0);
        }
    }

    #[test]
    fn test_analyze_exhaustion_long() {
        // Price below lower band, oversold RSI
        let signal = MeanReversionStrategy::analyze_exhaustion(
            90.0,   // price
            100.0,  // bb_upper
            95.0,   // bb_lower
            Some(25.0), // RSI (oversold)
        );

        assert!(matches!(signal, Signal::Long { .. }));
    }

    #[test]
    fn test_analyze_exhaustion_short() {
        // Price above upper band, overbought RSI
        let signal = MeanReversionStrategy::analyze_exhaustion(
            110.0,  // price
            105.0,  // bb_upper
            95.0,   // bb_lower
            Some(75.0), // RSI (overbought)
        );

        assert!(matches!(signal, Signal::Short { .. }));
    }

    #[test]
    fn test_analyze_exhaustion_wait() {
        // Price in middle, neutral RSI
        let signal = MeanReversionStrategy::analyze_exhaustion(
            100.0,  // price
            105.0,  // bb_upper
            95.0,   // bb_lower
            Some(50.0), // RSI (neutral)
        );

        assert!(matches!(signal, Signal::Wait));
    }

    #[test]
    fn test_weight_by_regime() {
        let strategy = MeanReversionStrategy::new();

        assert!((strategy.weight(&MarketRegime::Ranging) - 0.7).abs() < 0.01);
        assert!((strategy.weight(&MarketRegime::Volatile) - 0.5).abs() < 0.01);
        assert!((strategy.weight(&MarketRegime::Trending) - 0.2).abs() < 0.01);
    }
}
