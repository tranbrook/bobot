//! UltimateChannel - Keltner Channel variant using Ultimate Smoother.
//!
//! This module implements a Keltner-style channel around the Ultimate Smoother center line.
//! Unlike Bollinger Bands which use standard deviation, Keltner Channels use ATR
//! (Average True Range) for the band width.
//!
//! ## Formula
//!
//! - Center Line = Ultimate Smoother(typical_price)
//! - Upper Band = Center + (multiplier * ATR)
//! - Lower Band = Center - (multiplier * ATR)
//!
//! Where ATR is the smoothed Average True Range.

use super::smoother::{compute_ultimate_smoother, DEFAULT_C1, DEFAULT_C2, DEFAULT_C3};

/// Default ATR multiplier for channel bands
pub const DEFAULT_ATR_MULTIPLIER: f64 = 2.0;

/// Default ATR period for smoothing
pub const DEFAULT_ATR_PERIOD: usize = 14;

/// UltimateChannel output structure
#[derive(Debug, Clone)]
pub struct UltimateChannel {
    /// Center line (US-smoothed values)
    pub center: Vec<f64>,
    /// Upper band
    pub upper: Vec<f64>,
    /// Lower band
    pub lower: Vec<f64>,
    /// ATR values
    pub atr: Vec<f64>,
    /// Channel width (normalized: (upper - lower) / center)
    pub width: Vec<f64>,
    /// Channel width percentage
    pub width_pct: Vec<f64>,
}

impl UltimateChannel {
    /// Create new UltimateChannel with empty vectors
    pub fn new() -> Self {
        Self {
            center: Vec::new(),
            upper: Vec::new(),
            lower: Vec::new(),
            atr: Vec::new(),
            width: Vec::new(),
            width_pct: Vec::new(),
        }
    }

    /// Get the number of data points
    pub fn len(&self) -> usize {
        self.center.len()
    }

    /// Check if channel is empty
    pub fn is_empty(&self) -> bool {
        self.center.is_empty()
    }

    /// Get the latest channel values
    pub fn latest(&self) -> Option<(f64, f64, f64, f64)> {
        if self.center.is_empty() {
            None
        } else {
            Some((
                self.center.last().copied()?,
                self.upper.last().copied()?,
                self.lower.last().copied()?,
                self.atr.last().copied()?,
            ))
        }
    }
}

impl Default for UltimateChannel {
    fn default() -> Self {
        Self::new()
    }
}

/// Calculate True Range for each candle.
///
/// True Range = max(High - Low, |High - PrevClose|, |Low - PrevClose|)
///
/// # Arguments
/// * `high` - High prices
/// * `low` - Low prices
/// * `close` - Close prices
///
/// # Returns
/// Vector of True Range values (first value is High - Low since no prev close)
pub fn calculate_true_range(high: &[f64], low: &[f64], close: &[f64]) -> Vec<f64> {
    let n = high.len();
    if n == 0 {
        return Vec::new();
    }

    let mut tr = Vec::with_capacity(n);

    // First TR = High - Low (no previous close)
    tr.push(high[0] - low[0]);

    for i in 1..n {
        let prev_close = close[i - 1];
        let tr1 = high[i] - low[i];
        let tr2 = (high[i] - prev_close).abs();
        let tr3 = (low[i] - prev_close).abs();

        tr.push(tr1.max(tr2).max(tr3));
    }

    tr
}

/// Calculate ATR (Average True Range) using Wilder's smoothing method.
///
/// # Arguments
/// * `true_range` - True Range values
/// * `period` - ATR period (default: 14)
///
/// # Returns
/// Vector of ATR values
pub fn calculate_atr(true_range: &[f64], period: usize) -> Vec<f64> {
    let n = true_range.len();
    if n == 0 {
        return Vec::new();
    }

    let mut atr = Vec::with_capacity(n);

    // First ATR is simple average of first 'period' TR values
    let first_atr = if period <= n {
        true_range[..period].iter().sum::<f64>() / period as f64
    } else {
        true_range.iter().sum::<f64>() / n as f64
    };
    atr.push(first_atr);

    // Wilder's smoothing: ATR[n] = (ATR[n-1] * (period-1) + TR[n]) / period
    for i in 1..n {
        let prev_atr = atr[i - 1];
        let new_atr = (prev_atr * (period - 1) as f64 + true_range[i]) / period as f64;
        atr.push(new_atr);
    }

    atr
}

/// Calculate UltimateChannel from OHLC data.
///
/// # Arguments
/// * `high` - High prices
/// * `low` - Low prices
/// * `close` - Close prices
/// * `atr_multiplier` - Multiplier for ATR (default: 2.0)
/// * `atr_period` - ATR calculation period (default: 14)
/// * `c1`, `c2`, `c3` - Ultimate Smoother coefficients
///
/// # Returns
/// UltimateChannel structure with center, upper, lower, and atr values
pub fn calculate_ultimate_channel(
    high: &[f64],
    low: &[f64],
    close: &[f64],
    atr_multiplier: f64,
    atr_period: usize,
    c1: f64,
    c2: f64,
    c3: f64,
) -> UltimateChannel {
    let n = close.len();
    if n == 0 {
        return UltimateChannel::new();
    }

    // Calculate typical prices for US
    let typical_prices: Vec<f64> = close
        .iter()
        .zip(high.iter())
        .zip(low.iter())
        .map(|((&c, &h), &l)| (h + l + c) / 3.0)
        .collect();

    // Calculate center line (US-smoothed typical price)
    let center = compute_ultimate_smoother(&typical_prices, c1, c2, c3);

    // Calculate True Range and ATR
    let tr = calculate_true_range(high, low, close);
    let atr = calculate_atr(&tr, atr_period);

    // Calculate bands
    let mut upper = Vec::with_capacity(n);
    let mut lower = Vec::with_capacity(n);
    let mut width = Vec::with_capacity(n);
    let mut width_pct = Vec::with_capacity(n);

    for i in 0..n {
        let atr_val = atr.get(i).copied().unwrap_or(0.0);
        let center_val = center[i];
        let band_offset = atr_multiplier * atr_val;

        upper.push(center_val + band_offset);
        lower.push(center_val - band_offset);

        let channel_width = upper[i] - lower[i];
        width.push(channel_width);

        let width_p = if center_val.abs() > 1e-10 {
            (channel_width / center_val.abs()) * 100.0
        } else {
            0.0
        };
        width_pct.push(width_p);
    }

    UltimateChannel {
        center,
        upper,
        lower,
        atr,
        width,
        width_pct,
    }
}

/// Calculate UltimateChannel with default parameters.
#[inline]
pub fn calculate_ultimate_channel_default(
    high: &[f64],
    low: &[f64],
    close: &[f64],
) -> UltimateChannel {
    calculate_ultimate_channel(
        high,
        low,
        close,
        DEFAULT_ATR_MULTIPLIER,
        DEFAULT_ATR_PERIOD,
        DEFAULT_C1,
        DEFAULT_C2,
        DEFAULT_C3,
    )
}

/// Check if price is touching or exceeding channel bands.
///
/// # Returns
/// - "upper" if price >= upper band
/// - "lower" if price <= lower band
/// - "middle" if price is between bands
pub fn check_channel_touch(price: f64, upper: f64, lower: f64) -> &'static str {
    if price >= upper {
        "upper"
    } else if price <= lower {
        "lower"
    } else {
        "middle"
    }
}

/// Calculate channel breakout signal.
///
/// Returns a signal strength (-1.0 to 1.0) based on price position
/// relative to the channel.
///
/// - Positive values indicate upper channel breakout (bullish)
/// - Negative values indicate lower channel breakout (bearish)
pub fn calculate_channel_signal(price: f64, upper: f64, lower: f64, _center: f64) -> f64 {
    let channel_range = upper - lower;
    if channel_range.abs() < 1e-10 {
        return 0.0;
    }

    // Normalize price position: 0 = lower, 0.5 = center, 1.0 = upper
    let position = (price - lower) / channel_range;

    // Convert to signal: -1 to +1
    (position - 0.5) * 2.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_true_range_basic() {
        let high = vec![110.0, 115.0, 120.0];
        let low = vec![100.0, 105.0, 110.0];
        let close = vec![105.0, 110.0, 115.0];

        let tr = calculate_true_range(&high, &low, &close);

        assert_eq!(tr.len(), 3);
        // First TR = 110 - 100 = 10
        assert_eq!(tr[0], 10.0);
        // Second TR = max(10, |115-105|, |105-105|) = max(10, 10, 0) = 10
        assert_eq!(tr[1], 10.0);
    }

    #[test]
    fn test_atr_basic() {
        let tr = vec![10.0, 10.0, 10.0, 10.0, 10.0];
        let atr = calculate_atr(&tr, 3);

        assert_eq!(atr.len(), 5);
        // First ATR = average of first 3 = 10
        assert_eq!(atr[0], 10.0);
        // Subsequent should stay at 10 with constant TR
        for &val in &atr {
            assert!((val - 10.0).abs() < 0.01);
        }
    }

    #[test]
    fn test_atr_smoothing() {
        // TR increases: ATR should increase but lag
        let tr = vec![10.0, 20.0, 30.0, 40.0, 50.0];
        let atr = calculate_atr(&tr, 3);

        assert_eq!(atr.len(), 5);
        // ATR should increase monotonically
        for i in 1..atr.len() {
            assert!(atr[i] >= atr[i - 1]);
        }
        // But ATR should be less than current TR (smoothing effect)
        for i in 1..atr.len() {
            assert!(atr[i] <= tr[i]);
        }
    }

    #[test]
    fn test_ultimate_channel_basic() {
        let high: Vec<f64> = (100..120).map(|x| x as f64).collect();
        let low: Vec<f64> = (90..110).map(|x| x as f64).collect();
        let close: Vec<f64> = (95..115).map(|x| x as f64).collect();

        let channel = calculate_ultimate_channel_default(&high, &low, &close);

        assert_eq!(channel.len(), 20);
        assert_eq!(channel.center.len(), 20);
        assert_eq!(channel.upper.len(), 20);
        assert_eq!(channel.lower.len(), 20);
        assert_eq!(channel.atr.len(), 20);

        // Upper should always be >= center >= lower
        for i in 0..channel.len() {
            assert!(channel.upper[i] >= channel.center[i]);
            assert!(channel.center[i] >= channel.lower[i]);
        }
    }

    #[test]
    fn test_channel_touch() {
        assert_eq!(check_channel_touch(110.0, 100.0, 90.0), "upper");
        assert_eq!(check_channel_touch(90.0, 100.0, 90.0), "lower");
        assert_eq!(check_channel_touch(95.0, 100.0, 90.0), "middle");
    }

    #[test]
    fn test_channel_signal() {
        // Price at center = 0 signal
        let signal = calculate_channel_signal(95.0, 100.0, 90.0, 95.0);
        assert!((signal - 0.0).abs() < 0.01);

        // Price at upper = +1 signal
        let signal = calculate_channel_signal(100.0, 100.0, 90.0, 95.0);
        assert!((signal - 1.0).abs() < 0.01);

        // Price at lower = -1 signal
        let signal = calculate_channel_signal(90.0, 100.0, 90.0, 95.0);
        assert!((signal - (-1.0)).abs() < 0.01);
    }

    #[test]
    fn test_ultimate_channel_empty() {
        let high: Vec<f64> = vec![];
        let low: Vec<f64> = vec![];
        let close: Vec<f64> = vec![];

        let channel = calculate_ultimate_channel_default(&high, &low, &close);
        assert!(channel.is_empty());
    }

    #[test]
    fn test_ultimate_channel_latest() {
        let high: Vec<f64> = (100..120).map(|x| x as f64).collect();
        let low: Vec<f64> = (90..110).map(|x| x as f64).collect();
        let close: Vec<f64> = (95..115).map(|x| x as f64).collect();

        let channel = calculate_ultimate_channel_default(&high, &low, &close);

        let (center, upper, lower, atr) = channel.latest().unwrap();
        assert_eq!(center, channel.center.last().copied().unwrap());
        assert_eq!(upper, channel.upper.last().copied().unwrap());
        assert_eq!(lower, channel.lower.last().copied().unwrap());
        assert_eq!(atr, channel.atr.last().copied().unwrap());
    }
}
