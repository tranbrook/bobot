//! UltimateBands - Bollinger Bands variant based on Ultimate Smoother.
//!
//! This module implements volatility bands around the Ultimate Smoother center line.
//! The bands are calculated using standard deviation of the US-smoothed series.
//!
//! ## Formula
//!
//! - Center Line = Ultimate Smoother(typical_price)
//! - Upper Band = Center + (multiplier * std_dev)
//! - Lower Band = Center - (multiplier * std_dev)
//!
//! Where std_dev is calculated over a rolling window of the US-smoothed values.

use polars::prelude::*;
use anyhow::{Context, Result};

use super::smoother::{compute_ultimate_smoother, DEFAULT_C1, DEFAULT_C2, DEFAULT_C3};

/// Default standard deviation multiplier for bands
pub const DEFAULT_STDDEV_MULTIPLIER: f64 = 2.0;

/// Default rolling window for standard deviation calculation
pub const DEFAULT_WINDOW_SIZE: usize = 20;

/// UltimateBands output structure
#[derive(Debug, Clone)]
pub struct UltimateBands {
    /// Center line (US-smoothed values)
    pub center: Vec<f64>,
    /// Upper band
    pub upper: Vec<f64>,
    /// Lower band
    pub lower: Vec<f64>,
    /// Band width (normalized: (upper - lower) / center)
    pub width: Vec<f64>,
    /// Band width percentage (width * 100)
    pub width_pct: Vec<f64>,
}

impl UltimateBands {
    /// Create new UltimateBands with empty vectors
    pub fn new() -> Self {
        Self {
            center: Vec::new(),
            upper: Vec::new(),
            lower: Vec::new(),
            width: Vec::new(),
            width_pct: Vec::new(),
        }
    }

    /// Get the number of data points
    pub fn len(&self) -> usize {
        self.center.len()
    }

    /// Check if bands are empty
    pub fn is_empty(&self) -> bool {
        self.center.is_empty()
    }

    /// Get the latest band values
    pub fn latest(&self) -> Option<(f64, f64, f64)> {
        if self.center.is_empty() {
            None
        } else {
            Some((self.center.last().copied()?, self.upper.last().copied()?, self.lower.last().copied()?))
        }
    }

    /// Get the latest band width percentage
    pub fn latest_width_pct(&self) -> Option<f64> {
        self.width_pct.last().copied()
    }
}

impl Default for UltimateBands {
    fn default() -> Self {
        Self::new()
    }
}

/// Calculate UltimateBands from input price data.
///
/// # Arguments
/// * `input` - Input price slice (typically typical prices)
/// * `window_size` - Rolling window for standard deviation calculation
/// * `stddev_multiplier` - Multiplier for standard deviation (default: 2.0)
/// * `c1`, `c2`, `c3` - Ultimate Smoother coefficients
///
/// # Returns
/// UltimateBands structure with center, upper, lower, and width values
pub fn calculate_ultimate_bands(
    input: &[f64],
    window_size: usize,
    stddev_multiplier: f64,
    c1: f64,
    c2: f64,
    c3: f64,
) -> UltimateBands {
    let n = input.len();
    if n == 0 {
        return UltimateBands::new();
    }

    // First, calculate the Ultimate Smoother (center line)
    let center = compute_ultimate_smoother(input, c1, c2, c3);

    // Calculate rolling standard deviation and bands
    let mut upper = Vec::with_capacity(n);
    let mut lower = Vec::with_capacity(n);
    let mut width = Vec::with_capacity(n);
    let mut width_pct = Vec::with_capacity(n);

    for i in 0..n {
        // Calculate rolling std dev up to current index
        let start_idx = i.saturating_sub(window_size - 1);
        let window = &center[start_idx..=i];
        
        let std_dev = if window.len() > 1 {
            let mean = window.iter().sum::<f64>() / window.len() as f64;
            let variance = window.iter()
                .map(|&x| (x - mean).powi(2))
                .sum::<f64>() / (window.len() - 1) as f64;
            variance.sqrt()
        } else {
            0.0
        };

        let center_val = center[i];
        let band_offset = stddev_multiplier * std_dev;

        upper.push(center_val + band_offset);
        lower.push(center_val - band_offset);
        
        let band_width = upper[i] - lower[i];
        width.push(band_width);
        
        // Normalized width percentage
        let width_p = if center_val.abs() > 1e-10 {
            (band_width / center_val.abs()) * 100.0
        } else {
            0.0
        };
        width_pct.push(width_p);
    }

    UltimateBands {
        center,
        upper,
        lower,
        width,
        width_pct,
    }
}

/// Calculate UltimateBands with default parameters.
#[inline]
pub fn calculate_ultimate_bands_default(input: &[f64]) -> UltimateBands {
    calculate_ultimate_bands(
        input,
        DEFAULT_WINDOW_SIZE,
        DEFAULT_STDDEV_MULTIPLIER,
        DEFAULT_C1,
        DEFAULT_C2,
        DEFAULT_C3,
    )
}

/// Add UltimateBands columns to a DataFrame.
///
/// # Arguments
/// * `df` - Input DataFrame
/// * `input_column` - Name of input price column
/// * `prefix` - Prefix for output column names (e.g., "ub" for "ub_center", "ub_upper", etc.)
/// * `_window_size` - Rolling window for std dev (currently using default)
/// * `_stddev_multiplier` - Std dev multiplier (currently using default)
///
/// # Returns
/// New DataFrame with added columns: {prefix}_center, {prefix}_upper, {prefix}_lower, {prefix}_width, {prefix}_width_pct
pub fn add_ultimate_bands_columns(
    df: &DataFrame,
    input_column: &str,
    prefix: &str,
    _window_size: usize,
    _stddev_multiplier: f64,
) -> Result<DataFrame> {
    let series = df
        .column(input_column)
        .with_context(|| format!("Column '{}' not found in DataFrame", input_column))?;

    let input_data = series
        .f64()
        .context("Failed to cast column to Float64")?;

    let input_slice = input_data
        .cont_slice()
        .context("Failed to get contiguous slice from input column")?;

    let bands = calculate_ultimate_bands_default(input_slice);

    let mut result = df.clone();

    // Add center column
    result
        .with_column(Series::new(format!("{}_center", prefix).into(), bands.center.clone()))
        .context("Failed to add bands center column")?;

    // Add upper column
    result
        .with_column(Series::new(format!("{}_upper", prefix).into(), bands.upper))
        .context("Failed to add bands upper column")?;

    // Add lower column
    result
        .with_column(Series::new(format!("{}_lower", prefix).into(), bands.lower))
        .context("Failed to add bands lower column")?;

    // Add width column
    result
        .with_column(Series::new(format!("{}_width", prefix).into(), bands.width))
        .context("Failed to add bands width column")?;

    // Add width_pct column
    result
        .with_column(Series::new(format!("{}_width_pct", prefix).into(), bands.width_pct))
        .context("Failed to add bands width_pct column")?;

    Ok(result)
}

/// Check if price is touching or exceeding bands.
///
/// # Returns
/// - "upper" if price >= upper band
/// - "lower" if price <= lower band
/// - "middle" if price is between bands
/// - "none" if no bands data available
pub fn check_band_touch(price: f64, upper: f64, lower: f64) -> &'static str {
    if price >= upper {
        "upper"
    } else if price <= lower {
        "lower"
    } else {
        "middle"
    }
}

/// Calculate band exhaustion signal.
///
/// Returns a signal strength (-1.0 to 1.0) based on how far price
/// has extended beyond the bands.
///
/// - Positive values indicate upper band exhaustion (potential short)
/// - Negative values indicate lower band exhaustion (potential long)
pub fn calculate_band_exhaustion(price: f64, upper: f64, lower: f64, _center: f64) -> f64 {
    let band_range = upper - lower;
    if band_range.abs() < 1e-10 {
        return 0.0;
    }

    // Normalize price position relative to bands
    // 0 = at lower band, 0.5 = at center, 1.0 = at upper band
    let position = (price - lower) / band_range;

    // Convert to exhaustion signal
    // > 1.0 means above upper band (exhaustion, potential reversal)
    // < 0.0 means below lower band (exhaustion, potential reversal)
    (position - 0.5) * 2.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ultimate_bands_empty_input() {
        let input: Vec<f64> = vec![];
        let bands = calculate_ultimate_bands_default(&input);
        assert!(bands.is_empty());
        assert_eq!(bands.len(), 0);
    }

    #[test]
    fn test_ultimate_bands_basic() {
        let input: Vec<f64> = (1..=20).map(|x| x as f64).collect();
        let bands = calculate_ultimate_bands_default(&input);

        assert_eq!(bands.len(), 20);
        assert_eq!(bands.center.len(), 20);
        assert_eq!(bands.upper.len(), 20);
        assert_eq!(bands.lower.len(), 20);

        // Upper should always be >= center >= lower
        for i in 0..bands.len() {
            assert!(bands.upper[i] >= bands.center[i]);
            assert!(bands.center[i] >= bands.lower[i]);
        }
    }

    #[test]
    fn test_ultimate_bands_constant_input() {
        let input = vec![100.0; 30];
        let bands = calculate_ultimate_bands_default(&input);

        // With constant input, bands should converge
        // Center should be close to 100
        for &val in &bands.center {
            assert!((val - 100.0).abs() < 1.0);
        }

        // Width should be very small (low volatility)
        for &w in &bands.width_pct {
            assert!(w < 5.0); // Less than 5% width
        }
    }

    #[test]
    fn test_band_touch() {
        // Price above upper = "upper"
        assert_eq!(check_band_touch(110.0, 100.0, 90.0), "upper");
        // Price at upper = "upper" (touching)
        assert_eq!(check_band_touch(100.0, 100.0, 90.0), "upper");
        // Price at lower = "lower" (touching)
        assert_eq!(check_band_touch(90.0, 100.0, 90.0), "lower");
        // Price below lower = "lower"
        assert_eq!(check_band_touch(85.0, 100.0, 90.0), "lower");
        // Price in middle
        assert_eq!(check_band_touch(95.0, 100.0, 90.0), "middle");
    }

    #[test]
    fn test_band_exhaustion() {
        // Price at center = 0 signal
        let signal = calculate_band_exhaustion(95.0, 100.0, 90.0, 95.0);
        assert!((signal - 0.0).abs() < 0.01);

        // Price at upper band = positive signal
        let signal = calculate_band_exhaustion(100.0, 100.0, 90.0, 95.0);
        assert!(signal > 0.0);

        // Price at lower band = negative signal
        let signal = calculate_band_exhaustion(90.0, 100.0, 90.0, 95.0);
        assert!(signal < 0.0);

        // Price above upper band = strong positive (exhaustion)
        let signal = calculate_band_exhaustion(105.0, 100.0, 90.0, 95.0);
        assert!(signal > 0.5);

        // Price below lower band = strong negative (exhaustion)
        let signal = calculate_band_exhaustion(85.0, 100.0, 90.0, 95.0);
        assert!(signal < -0.5);
    }

    #[test]
    fn test_ultimate_bands_width() {
        // Create input with increasing volatility
        let mut input = Vec::new();
        for i in 0..50 {
            let base = 100.0;
            let volatility = (i / 10) as f64 * 5.0; // Increasing volatility
            input.push(base + (i as f64 * 0.1).sin() * volatility);
        }

        let bands = calculate_ultimate_bands_default(&input);

        // Width should generally increase as volatility increases
        assert!(bands.width_pct.last().unwrap() > bands.width_pct.first().unwrap());
    }

    #[test]
    fn test_ultimate_bands_latest() {
        let input: Vec<f64> = (1..=20).map(|x| x as f64).collect();
        let bands = calculate_ultimate_bands_default(&input);

        let (center, upper, lower) = bands.latest().unwrap();
        assert_eq!(center, bands.center.last().copied().unwrap());
        assert_eq!(upper, bands.upper.last().copied().unwrap());
        assert_eq!(lower, bands.lower.last().copied().unwrap());
    }
}
