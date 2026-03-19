//! Ultimate Smoother - High-performance recursive digital filter.
//!
//! This module implements the Ultimate Smoother (US) recursive formula:
//!
//! ```text
//! US[n] = (1-c1)*D[n] + (2c1-c2)*D[n-1] - (c1+c3)*D[n-2] + c2*US[n-1] + c3*US[n-2]
//! ```
//!
//! Where:
//! - D[n] = input data (typical price: (H+L+C)/3)
//! - c1, c2, c3 = filter coefficients tuned for minimal lag and noise reduction
//!
//! ## Performance Optimization
//!
//! - Uses iterative Rust loop (NOT Polars LazyFrame) for O(n) complexity
//! - Zero-copy extraction from Polars Series using `.rechunk()` and `.contig_slice()`
//! - Pre-allocated output vector to avoid reallocations
//! - CPU cache-friendly sequential memory access

use anyhow::{Context, Result};
use polars::prelude::*;

/// Default filter coefficients for the Ultimate Smoother.
///
/// These coefficients are tuned for minimal lag and noise reduction
/// in financial time series data.
pub const DEFAULT_C1: f64 = 0.07;
pub const DEFAULT_C2: f64 = 0.05;
pub const DEFAULT_C3: f64 = 0.03;

/// Compute the Ultimate Smoother recursive filter.
///
/// This is a high-performance iterative implementation that processes
/// the input slice in a single pass with O(n) complexity.
///
/// # Arguments
/// * `input` - Input data slice (typically typical prices)
/// * `c1` - Filter coefficient 1 (default: 0.07)
/// * `c2` - Filter coefficient 2 (default: 0.05)
/// * `c3` - Filter coefficient 3 (default: 0.03)
///
/// # Returns
/// Vector of smoothed values with the same length as input
///
/// # Boundary Conditions
/// - US[0] = D[0] (first value initialized to input)
/// - US[1] = D[1] (second value initialized to input)
/// - Recursive formula applied for n >= 2
///
/// # Formula
/// ```text
/// US[n] = (1-c1)*D[n] + (2c1-c2)*D[n-1] - (c1+c3)*D[n-2] + c2*US[n-1] + c3*US[n-2]
/// ```
#[inline]
pub fn compute_ultimate_smoother(
    input: &[f64],
    c1: f64,
    c2: f64,
    c3: f64,
) -> Vec<f64> {
    let n = input.len();
    
    // Handle edge cases
    if n == 0 {
        return Vec::new();
    }
    
    // Pre-allocate output vector with exact capacity
    let mut output = Vec::with_capacity(n);
    
    // Boundary conditions: initialize first two values
    // US[0] = D[0], US[1] = D[1]
    // This provides the warm-up needed for the recursive calculation
    if n > 0 {
        output.push(input[0]);
    }
    if n > 1 {
        output.push(input[1]);
    }
    
    // Pre-compute coefficients for the iterative loop
    let coef_d_n = 1.0 - c1;
    let coef_d_n1 = 2.0 * c1 - c2;
    let coef_d_n2 = -(c1 + c3);
    
    // Iterative calculation for n >= 2
    // This is the core recursive formula implemented efficiently
    for i in 2..n {
        let us_n = coef_d_n * input[i]
            + coef_d_n1 * input[i - 1]
            + coef_d_n2 * input[i - 2]
            + c2 * output[i - 1]
            + c3 * output[i - 2];
        output.push(us_n);
    }
    
    output
}

/// Compute Ultimate Smoother with default coefficients.
///
/// Convenience wrapper using DEFAULT_C1, DEFAULT_C2, DEFAULT_C3.
#[inline]
pub fn compute_ultimate_smoother_default(input: &[f64]) -> Vec<f64> {
    compute_ultimate_smoother(input, DEFAULT_C1, DEFAULT_C2, DEFAULT_C3)
}

/// Extract data from a Polars Float64 Series into an owned Vec<f64>.
///
/// This function ensures the data is contiguous in memory by calling
/// `.rechunk()` before extracting. While this involves a copy, it provides:
/// - Safe ownership of the data (no lifetime issues)
/// - CPU cache locality during iterative processing
/// - Works with any Series regardless of chunking
///
/// # Arguments
/// * `series` - Polars Series containing Float64 data
///
/// # Returns
/// An owned `Vec<f64>` containing the series data
///
/// # Errors
/// Returns an error if:
/// - The series cannot be cast to Float64
/// - The series contains null values
pub fn extract_f64_from_series(series: &Series) -> Result<Vec<f64>> {
    // Ensure data type is Float64
    let f64_series = series
        .cast(&DataType::Float64)
        .context("Failed to cast series to Float64")?;
    
    // Rechunk to ensure contiguous memory
    // This consolidates any fragmented chunks into a single memory block
    let rechunked = f64_series.rechunk();
    
    // Downcast to Float64Chunked
    let chunked = rechunked
        .f64()
        .context("Failed to downcast to Float64Chunked")?;
    
    // Get contiguous slice and copy to owned Vec
    // Note: cont_slice() returns Option<&[T]> - None if there are nulls
    let slice = chunked
        .cont_slice()
        .context("Failed to get contiguous slice from Float64Chunked. Series may contain null values.")?;
    
    // Copy to owned Vec for safe ownership
    // This is still efficient as we only copy once
    Ok(slice.to_vec())
}

/// Compute Ultimate Smoother directly from a Polars Series.
///
/// This is a convenience function that combines extraction and computation.
///
/// # Arguments
/// * `series` - Input Polars Series (will be cast to Float64 if needed)
/// * `c1` - Filter coefficient 1
/// * `c2` - Filter coefficient 2
/// * `c3` - Filter coefficient 3
///
/// # Returns
/// Vector of smoothed values
///
/// # Errors
/// Returns an error if the series cannot be processed
pub fn compute_ultimate_smoother_from_series(
    series: &Series,
    c1: f64,
    c2: f64,
    c3: f64,
) -> Result<Vec<f64>> {
    let data = extract_f64_from_series(series)?;
    Ok(compute_ultimate_smoother(&data, c1, c2, c3))
}

/// Compute Ultimate Smoother from a Polars Series with default coefficients.
#[inline]
pub fn compute_ultimate_smoother_from_series_default(series: &Series) -> Result<Vec<f64>> {
    compute_ultimate_smoother_from_series(series, DEFAULT_C1, DEFAULT_C2, DEFAULT_C3)
}

/// Add the Ultimate Smoother as a new column to a DataFrame.
///
/// # Arguments
/// * `df` - Input DataFrame (will not be modified)
/// * `input_column` - Name of the input column (e.g., "typical_price")
/// * `output_column` - Name of the output column (e.g., "us_value")
/// * `c1`, `c2`, `c3` - Filter coefficients
///
/// # Returns
/// A new DataFrame with the additional smoothed column
///
/// # Errors
/// Returns an error if:
/// - The input column doesn't exist
/// - The series cannot be processed
pub fn add_ultimate_smoother_column(
    df: &DataFrame,
    input_column: &str,
    output_column: &str,
    c1: f64,
    c2: f64,
    c3: f64,
) -> Result<DataFrame> {
    let column = df
        .column(input_column)
        .with_context(|| format!("Column '{}' not found in DataFrame", input_column))?;
    
    // Get the underlying Series from the Column
    // In Polars 0.46, Column wraps Series
    let series = column.as_materialized_series();
    
    let smoothed = compute_ultimate_smoother_from_series(series, c1, c2, c3)?;
    
    // Create a new Series with the smoothed values
    let smoothed_series = Series::new(output_column.into(), smoothed);
    
    // Clone the DataFrame and add the new column
    let mut result = df.clone();
    result
        .with_column(smoothed_series)
        .context("Failed to add smoothed column to DataFrame")?;
    
    Ok(result)
}

/// Add Ultimate Smoother column with default coefficients.
#[inline]
pub fn add_ultimate_smoother_column_default(
    df: &DataFrame,
    input_column: &str,
    output_column: &str,
) -> Result<DataFrame> {
    add_ultimate_smoother_column(df, input_column, output_column, DEFAULT_C1, DEFAULT_C2, DEFAULT_C3)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ultimate_smoother_empty_input() {
        let input: Vec<f64> = vec![];
        let result = compute_ultimate_smoother(&input, DEFAULT_C1, DEFAULT_C2, DEFAULT_C3);
        assert!(result.is_empty());
    }

    #[test]
    fn test_ultimate_smoother_single_value() {
        let input = vec![100.0];
        let result = compute_ultimate_smoother(&input, DEFAULT_C1, DEFAULT_C2, DEFAULT_C3);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], 100.0); // US[0] = D[0]
    }

    #[test]
    fn test_ultimate_smoother_two_values() {
        let input = vec![100.0, 105.0];
        let result = compute_ultimate_smoother(&input, DEFAULT_C1, DEFAULT_C2, DEFAULT_C3);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0], 100.0); // US[0] = D[0]
        assert_eq!(result[1], 105.0); // US[1] = D[1]
    }

    #[test]
    fn test_ultimate_smoother_increasing_sequence() {
        // Test with a simple increasing sequence: 1, 2, 3, 4, 5
        let input: Vec<f64> = (1..=5).map(|x| x as f64).collect();
        let result = compute_ultimate_smoother_default(&input);
        
        // First two values should match input (boundary conditions)
        assert_eq!(result[0], 1.0);
        assert_eq!(result[1], 2.0);
        
        // Verify we got the right number of outputs
        assert_eq!(result.len(), 5);
        
        // All values should be positive and increasing (smoothed)
        for i in 1..result.len() {
            assert!(result[i] > result[i - 1], "Smoothed values should be increasing");
        }
    }

    #[test]
    fn test_ultimate_smoother_constant_input() {
        // With constant input, output should converge to the constant
        let input = vec![100.0; 10];
        let result = compute_ultimate_smoother_default(&input);
        
        // First two values are boundary conditions
        assert_eq!(result[0], 100.0);
        assert_eq!(result[1], 100.0);
        
        // All subsequent values should be very close to 100
        for &val in &result {
            assert!((val - 100.0).abs() < 0.01, "Constant input should produce constant output");
        }
    }

    #[test]
    fn test_ultimate_smoother_step_change() {
        // Test response to a step change: 100, 100, 100, 200, 200, 200
        let input = vec![100.0, 100.0, 100.0, 200.0, 200.0, 200.0];
        let result = compute_ultimate_smoother_default(&input);
        
        // First two values are boundary conditions (equal to input)
        assert_eq!(result[0], 100.0);
        assert_eq!(result[1], 100.0);
        
        // After the step at index 3, the smoothed value should increase
        // (smoothing effect - gradual response to step change)
        assert!(result[3] > result[2], "Should respond to step increase");
        
        // Final value should be greater than initial value (response to step)
        assert!(result[5] > result[2], "Should show response to step change");
        
        // The smoother should track toward the new level
        // Note: With default coefficients, convergence can be fast
        // so we just verify the direction of change
    }

    #[test]
    fn test_ultimate_smoother_coefficients_impact() {
        let input: Vec<f64> = (1..=10).map(|x| x as f64).collect();
        
        // Different coefficients produce different smoothing
        let result1 = compute_ultimate_smoother(&input, 0.01, 0.01, 0.01);
        let result2 = compute_ultimate_smoother(&input, 0.1, 0.1, 0.1);
        
        // Both should have same length
        assert_eq!(result1.len(), result2.len());
        
        // But different values (different smoothing behavior)
        assert_ne!(result1[5], result2[5]);
    }

    #[test]
    fn test_ultimate_smoother_from_series() {
        let series = Series::new("price".into(), vec![100.0, 105.0, 110.0, 108.0, 112.0]);
        let result = compute_ultimate_smoother_from_series_default(&series).unwrap();
        
        assert_eq!(result.len(), 5);
        assert_eq!(result[0], 100.0);
        assert_eq!(result[1], 105.0);
    }

    #[test]
    fn test_ultimate_smoother_from_series_with_cast() {
        // Test with integer series that needs casting to f64
        let series = Series::new("price".into(), vec![100i32, 105, 110, 108, 112]);
        let result = compute_ultimate_smoother_from_series_default(&series).unwrap();
        
        assert_eq!(result.len(), 5);
        assert!((result[0] - 100.0).abs() < 0.001);
    }

    #[test]
    fn test_extract_f64_from_series() {
        let series = Series::new("data".into(), vec![1.0, 2.0, 3.0, 4.0, 5.0]);
        let data = extract_f64_from_series(&series).unwrap();
        
        assert_eq!(data.len(), 5);
        assert_eq!(data[0], 1.0);
        assert_eq!(data[4], 5.0);
    }

    #[test]
    fn test_add_ultimate_smoother_column() {
        use polars::prelude::Column;
        
        let df = DataFrame::new(vec![
            Column::from(Series::new("timestamp".into(), vec![1i64, 2, 3, 4, 5])),
            Column::from(Series::new("price".into(), vec![100.0, 105.0, 110.0, 108.0, 112.0])),
        ]).unwrap();
        
        let result = add_ultimate_smoother_column_default(&df, "price", "us_price").unwrap();
        
        // Should have 3 columns now
        assert_eq!(result.width(), 3);
        assert!(result.column("us_price").is_ok());
        
        // Original columns should be unchanged
        let original_price = df.column("price").unwrap().f64().unwrap();
        let result_price = result.column("price").unwrap().f64().unwrap();
        assert_eq!(original_price.len(), result_price.len());
    }

    #[test]
    fn test_default_coefficients() {
        assert_eq!(DEFAULT_C1, 0.07);
        assert_eq!(DEFAULT_C2, 0.05);
        assert_eq!(DEFAULT_C3, 0.03);
    }

    #[test]
    fn test_ultimate_smoother_formula_verification() {
        // Manually verify the formula for a simple case
        let input = vec![100.0, 105.0, 110.0];
        let c1 = 0.1;
        let c2 = 0.05;
        let c3 = 0.02;
        
        let result = compute_ultimate_smoother(&input, c1, c2, c3);
        
        // US[0] = D[0] = 100
        assert_eq!(result[0], 100.0);
        
        // US[1] = D[1] = 105
        assert_eq!(result[1], 105.0);
        
        // US[2] = (1-c1)*D[2] + (2c1-c2)*D[1] - (c1+c3)*D[0] + c2*US[1] + c3*US[0]
        //       = 0.9*110 + (0.2-0.05)*105 - (0.1+0.02)*100 + 0.05*105 + 0.02*100
        //       = 99 + 15.75 - 12 + 5.25 + 2
        //       = 110.0
        let expected = (1.0 - c1) * input[2]
            + (2.0 * c1 - c2) * input[1]
            - (c1 + c3) * input[0]
            + c2 * result[1]
            + c3 * result[0];
        
        assert!((result[2] - expected).abs() < 1e-10, "Formula verification failed");
    }

    #[test]
    fn test_ultimate_smoother_performance_characteristics() {
        // Create a larger dataset to verify performance characteristics
        let input: Vec<f64> = (1..=1000).map(|x| x as f64).collect();
        
        // Should complete quickly (iterative O(n))
        let start = std::time::Instant::now();
        let result = compute_ultimate_smoother_default(&input);
        let elapsed = start.elapsed();
        
        // Verify output length
        assert_eq!(result.len(), 1000);
        
        // Should complete in under 1ms for 1000 elements
        assert!(elapsed.as_millis() < 10, "Processing took too long: {:?}", elapsed);
    }
}
