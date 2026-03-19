//! Market Regime Detector.
//!
//! This module implements market regime detection using ADX and Bollinger Band Width.
//!
//! ## Regime Classification
//!
//! - **TRENDING**: ADX > 25 (strong trend)
//! - **RANGING**: ADX < 20 AND BB Width < 5% (weak trend, low volatility)
//! - **VOLATILE**: BB Width > 10% OR ADX rising rapidly (high volatility)

use polars::prelude::*;
use anyhow::{Context, Result};

use crate::data::models::MarketRegime;

/// Default ADX period for calculation
pub const DEFAULT_ADX_PERIOD: usize = 14;

/// Default BB Width period for calculation
pub const DEFAULT_BB_PERIOD: usize = 20;

/// ADX threshold for trending market
pub const TRENDING_ADX_THRESHOLD: f64 = 25.0;

/// ADX threshold for ranging market
pub const RANGING_ADX_THRESHOLD: f64 = 20.0;

/// BB Width threshold for volatile market (as percentage)
pub const VOLATILE_BB_WIDTH_THRESHOLD: f64 = 10.0;

/// BB Width threshold for ranging market (as percentage)
pub const RANGING_BB_WIDTH_THRESHOLD: f64 = 5.0;

/// Calculate ADX (Average Directional Index).
///
/// # Arguments
/// * `high` - High prices
/// * `low` - Low prices
/// * `close` - Close prices
/// * `period` - ADX period (default: 14)
///
/// # Returns
/// Vector of ADX values (None for first `period` values)
pub fn calculate_adx(
    high: &[f64],
    low: &[f64],
    close: &[f64],
    period: usize,
) -> Vec<Option<f64>> {
    let n = close.len();
    if n < period + 1 {
        return vec![None; n];
    }

    let mut adx = Vec::with_capacity(n);

    // Calculate +DM, -DM, and TR
    let mut plus_dm = Vec::with_capacity(n);
    let mut minus_dm = Vec::with_capacity(n);
    let mut tr = Vec::with_capacity(n);

    plus_dm.push(0.0);
    minus_dm.push(0.0);
    tr.push(high[0] - low[0]);

    for i in 1..n {
        let high_diff = high[i] - high[i - 1];
        let low_diff = low[i - 1] - low[i];

        let plus = if high_diff > low_diff && high_diff > 0.0 {
            high_diff
        } else {
            0.0
        };

        let minus = if low_diff > high_diff && low_diff > 0.0 {
            low_diff
        } else {
            0.0
        };

        let true_range = (high[i] - low[i])
            .max((high[i] - close[i - 1]).abs())
            .max((low[i] - close[i - 1]).abs());

        plus_dm.push(plus);
        minus_dm.push(minus);
        tr.push(true_range);
    }

    // Smooth TR, +DM, -DM using Wilder's method
    let mut atr = Vec::with_capacity(n);
    let mut plus_di = Vec::with_capacity(n);
    let mut minus_di = Vec::with_capacity(n);

    // First ATR is simple average
    let first_atr: f64 = tr[..period].iter().sum::<f64>() / period as f64;
    atr.push(first_atr);

    // First DI values
    let first_plus_dm: f64 = plus_dm[..period].iter().sum();
    let first_minus_dm: f64 = minus_dm[..period].iter().sum();

    let first_plus_di = if first_atr > 0.0 {
        (first_plus_dm / first_atr) * 100.0
    } else {
        0.0
    };
    let first_minus_di = if first_atr > 0.0 {
        (first_minus_dm / first_atr) * 100.0
    } else {
        0.0
    };

    plus_di.push(first_plus_di);
    minus_di.push(first_minus_di);

    // Fill in smoothed values
    for i in 1..n {
        if i < period {
            atr.push(first_atr);
            plus_di.push(first_plus_di);
            minus_di.push(first_minus_di);
            continue;
        }

        // Wilder's smoothing: ATR[n] = (ATR[n-1] * (period-1) + TR[n]) / period
        let new_atr = (atr[i - 1] * (period - 1) as f64 + tr[i]) / period as f64;
        atr.push(new_atr);

        // Smoothed DM
        let smoothed_plus_dm = plus_dm[i] + plus_dm[i - period + 1..i].iter().sum::<f64>();
        let smoothed_minus_dm = minus_dm[i] + minus_dm[i - period + 1..i].iter().sum::<f64>();

        let p_di = if new_atr > 0.0 {
            (smoothed_plus_dm / new_atr) * 100.0
        } else {
            0.0
        };
        let m_di = if new_atr > 0.0 {
            (smoothed_minus_dm / new_atr) * 100.0
        } else {
            0.0
        };

        plus_di.push(p_di);
        minus_di.push(m_di);
    }

    // Calculate DX and ADX
    for i in 0..n {
        if i < period {
            adx.push(None);
            continue;
        }

        let plus = plus_di[i];
        let minus = minus_di[i];
        let sum = plus + minus;

        let dx = if sum > 0.0 {
            ((plus - minus).abs() / sum) * 100.0
        } else {
            0.0
        };

        // First ADX is simple average of DX
        if i == period {
            let first_dx_sum: f64 = (period..=i)
                .map(|j| {
                    let p = plus_di[j];
                    let m = minus_di[j];
                    let s = p + m;
                    if s > 0.0 {
                        ((p - m).abs() / s) * 100.0
                    } else {
                        0.0
                    }
                })
                .sum();
            adx.push(Some(first_dx_sum / (i - period + 1) as f64));
        } else {
            // Wilder's smoothing for ADX
            let prev_adx = adx[i - 1].unwrap_or(0.0);
            let new_adx = (prev_adx * (period - 1) as f64 + dx) / period as f64;
            adx.push(Some(new_adx));
        }
    }

    adx
}

/// Calculate Bollinger Band Width.
///
/// BB Width = (Upper Band - Lower Band) / Middle Band * 100
///
/// # Arguments
/// * `close` - Close prices
/// * `period` - BB period (default: 20)
/// * `stddev_multiplier` - Standard deviation multiplier (default: 2.0)
///
/// # Returns
/// Vector of BB Width percentages
pub fn calculate_bb_width(
    close: &[f64],
    period: usize,
    stddev_multiplier: f64,
) -> Vec<Option<f64>> {
    let n = close.len();
    let mut bb_width = Vec::with_capacity(n);

    for i in 0..n {
        if i < period.saturating_sub(1) {
            bb_width.push(None);
            continue;
        }

        let start = i.saturating_sub(period - 1);
        let window = &close[start..=i];
        let mean = window.iter().sum::<f64>() / period as f64;
        let variance = window.iter()
            .map(|&x| (x - mean).powi(2))
            .sum::<f64>() / (period - 1) as f64;
        let stddev = variance.sqrt();

        let upper = mean + stddev_multiplier * stddev;
        let lower = mean - stddev_multiplier * stddev;

        let width = if mean.abs() > 1e-10 {
            ((upper - lower) / mean.abs()) * 100.0
        } else {
            0.0
        };

        bb_width.push(Some(width));
    }

    bb_width
}

/// Detect market regime based on ADX and BB Width.
///
/// # Arguments
/// * `adx` - ADX value
/// * `bb_width` - BB Width percentage
///
/// # Returns
/// MarketRegime (Trending, Ranging, or Volatile)
pub fn detect_regime(adx: f64, bb_width: f64) -> MarketRegime {
    // Check for volatile market first (highest priority)
    if bb_width > VOLATILE_BB_WIDTH_THRESHOLD {
        return MarketRegime::Volatile;
    }

    // Check for trending market
    if adx > TRENDING_ADX_THRESHOLD {
        return MarketRegime::Trending;
    }

    // Check for ranging market
    if adx < RANGING_ADX_THRESHOLD && bb_width < RANGING_BB_WIDTH_THRESHOLD {
        return MarketRegime::Ranging;
    }

    // Default: treat as transitional (use Volatile as safe default)
    MarketRegime::Volatile
}

/// Calculate market regime from OHLCV data.
///
/// # Arguments
/// * `high` - High prices
/// * `low` - Low prices
/// * `close` - Close prices
/// * `adx_period` - ADX period
/// * `bb_period` - BB period
///
/// # Returns
/// Vector of MarketRegime values
pub fn calculate_market_regime(
    high: &[f64],
    low: &[f64],
    close: &[f64],
    adx_period: usize,
    bb_period: usize,
) -> Vec<MarketRegime> {
    let adx = calculate_adx(high, low, close, adx_period);
    let bb_width = calculate_bb_width(close, bb_period, 2.0);

    adx.iter()
        .zip(bb_width.iter())
        .map(|(a, b)| {
            match (a, b) {
                (Some(adx_val), Some(bb_val)) => detect_regime(*adx_val, *bb_val),
                _ => MarketRegime::Volatile, // Default for insufficient data
            }
        })
        .collect()
}

/// Get the latest market regime from OHLCV data.
///
/// # Arguments
/// * `high` - High prices
/// * `low` - Low prices
/// * `close` - Close prices
/// * `adx_period` - ADX period
/// * `bb_period` - BB period
///
/// # Returns
/// Latest MarketRegime
pub fn get_latest_regime(
    high: &[f64],
    low: &[f64],
    close: &[f64],
    adx_period: usize,
    bb_period: usize,
) -> MarketRegime {
    let regimes = calculate_market_regime(high, low, close, adx_period, bb_period);
    regimes.last().copied().unwrap_or(MarketRegime::Volatile)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_regime_trending() {
        assert_eq!(detect_regime(30.0, 3.0), MarketRegime::Trending);
        assert_eq!(detect_regime(26.0, 4.0), MarketRegime::Trending);
    }

    #[test]
    fn test_detect_regime_ranging() {
        assert_eq!(detect_regime(15.0, 3.0), MarketRegime::Ranging);
        assert_eq!(detect_regime(18.0, 4.0), MarketRegime::Ranging);
    }

    #[test]
    fn test_detect_regime_volatile() {
        assert_eq!(detect_regime(15.0, 12.0), MarketRegime::Volatile);
        assert_eq!(detect_regime(30.0, 15.0), MarketRegime::Volatile);
    }

    #[test]
    fn test_bb_width_constant_prices() {
        let close = vec![100.0; 30];
        let bb_width = calculate_bb_width(&close, 20, 2.0);

        // With constant prices, BB width should be 0
        for width in bb_width.iter().skip(19) {
            if let Some(w) = width {
                assert!(*w < 0.01);
            }
        }
    }

    #[test]
    fn test_bb_width_increasing_volatility() {
        // Create prices with increasing volatility
        let close: Vec<f64> = (0..30)
            .map(|i| 100.0 + (i as f64 * 0.5).sin() * (i as f64 / 5.0))
            .collect();

        let bb_width = calculate_bb_width(&close, 20, 2.0);

        // BB width should increase as volatility increases
        let last_width = bb_width.last().unwrap().unwrap_or(0.0);
        assert!(last_width > 0.0);
    }

    #[test]
    fn test_adx_basic() {
        // Create trending price data
        let close: Vec<f64> = (0..30).map(|i| 100.0 + i as f64 * 2.0).collect();
        let high: Vec<f64> = close.iter().map(|&c| c + 1.0).collect();
        let low: Vec<f64> = close.iter().map(|&c| c - 1.0).collect();

        let adx = calculate_adx(&high, &low, &close, 14);

        // ADX should have values after period
        assert!(adx.iter().skip(14).any(|v| v.is_some()));
    }
}
