//! Polars DataFrame manager for the Muscle signal engine.
//!
//! This module provides a rolling window DataFrame manager that maintains
//! a fixed-size window of candlestick data for technical analysis.
//!
//! ## Features
//!
//! - Fixed-size rolling window (default: 1000 candles)
//! - Warm-up buffer support (default: 100 candles)
//! - Efficient updates with minimal cloning
//! - LazyFrame optimization support
//! - Zero-copy slice extraction for calculations

use polars::prelude::*;
use anyhow::{Context, Result};
use tracing::info;

use crate::data::models::Kline;

/// Default working window size (excluding warm-up)
pub const DEFAULT_WORKING_WINDOW: usize = 1000;

/// Default warm-up buffer size
pub const DEFAULT_WARMUP_SIZE: usize = 100;

/// Total default size (warmup + working)
pub const DEFAULT_TOTAL_SIZE: usize = DEFAULT_WARMUP_SIZE + DEFAULT_WORKING_WINDOW;

/// DataFrame manager with rolling window support
pub struct DataFrameManager {
    /// The main DataFrame containing all candle data
    df: DataFrame,
    /// Maximum number of rows to keep (warmup + working)
    max_rows: usize,
    /// Number of warm-up rows (not used for signal export)
    warmup_rows: usize,
    /// Number of working rows (used for signal export)
    working_rows: usize,
    /// Track if we have enough data for warm-up
    is_warmed_up: bool,
}

impl DataFrameManager {
    /// Create a new DataFrame manager with default settings
    pub fn new() -> Self {
        Self::with_size(DEFAULT_TOTAL_SIZE, DEFAULT_WARMUP_SIZE)
    }

    /// Create a new DataFrame manager with custom sizes
    ///
    /// # Arguments
    /// * `max_rows` - Total maximum rows (warmup + working)
    /// * `warmup_rows` - Number of warm-up rows (excluded from signal export)
    pub fn with_size(max_rows: usize, warmup_rows: usize) -> Self {
        let working_rows = max_rows.saturating_sub(warmup_rows);
        
        Self {
            df: DataFrame::empty(),
            max_rows,
            warmup_rows,
            working_rows,
            is_warmed_up: false,
        }
    }

    /// Initialize the DataFrame with schema
    fn init_schema(&mut self) -> Result<()> {
        if self.df.width() == 0 {
            self.df = df! {
                "timestamp_ms" => Vec::<i64>::new(),
                "open" => Vec::<f64>::new(),
                "high" => Vec::<f64>::new(),
                "low" => Vec::<f64>::new(),
                "close" => Vec::<f64>::new(),
                "volume" => Vec::<f64>::new(),
                "is_closed" => Vec::<bool>::new(),
                "received_at_ms" => Vec::<i64>::new(),
            }.context("Failed to create initial DataFrame")?;
        }
        Ok(())
    }

    /// Initialize DataFrame from a vector of Klines
    ///
    /// # Arguments
    /// * `klines` - Vector of klines to load (typically 1100 for bootstrap)
    ///
    /// # Returns
    /// Number of rows loaded
    pub fn init_from_klines(&mut self, klines: &[Kline]) -> Result<usize> {
        self.init_schema()?;

        let n = klines.len();
        
        // Create series from kline data
        let timestamp_ms: Vec<i64> = klines.iter().map(|k| k.timestamp_ms).collect();
        let open: Vec<f64> = klines.iter().map(|k| k.open).collect();
        let high: Vec<f64> = klines.iter().map(|k| k.high).collect();
        let low: Vec<f64> = klines.iter().map(|k| k.low).collect();
        let close: Vec<f64> = klines.iter().map(|k| k.close).collect();
        let volume: Vec<f64> = klines.iter().map(|k| k.volume).collect();
        let is_closed: Vec<bool> = klines.iter().map(|k| k.is_closed).collect();
        let received_at_ms: Vec<i64> = klines.iter().map(|k| k.received_at_ms).collect();

        self.df = df! {
            "timestamp_ms" => timestamp_ms,
            "open" => open,
            "high" => high,
            "low" => low,
            "close" => close,
            "volume" => volume,
            "is_closed" => is_closed,
            "received_at_ms" => received_at_ms,
        }.context("Failed to create DataFrame from klines")?;

        // Check if we have enough data for warm-up
        self.is_warmed_up = n >= self.warmup_rows;

        info!(
            "Initialized DataFrame with {} rows (warmup: {}, working: {})",
            n, self.warmup_rows, self.working_rows
        );

        Ok(n)
    }

    /// Add a new kline to the DataFrame
    ///
    /// If the DataFrame is at max capacity, the oldest row is removed.
    ///
    /// # Arguments
    /// * `kline` - New kline to add
    ///
    /// # Returns
    /// Current row count after addition
    pub fn add_kline(&mut self, kline: &Kline) -> Result<usize> {
        // Check if we need to remove oldest row
        if self.df.height() >= self.max_rows {
            // Remove first row (oldest)
            self.df = self.df.slice(1, self.df.height() - 1);
        }

        // Handle empty DataFrame case - initialize with first row
        if self.df.is_empty() || self.df.width() == 0 {
            self.df = df! {
                "timestamp_ms" => vec![kline.timestamp_ms],
                "open" => vec![kline.open],
                "high" => vec![kline.high],
                "low" => vec![kline.low],
                "close" => vec![kline.close],
                "volume" => vec![kline.volume],
                "is_closed" => vec![kline.is_closed],
                "received_at_ms" => vec![kline.received_at_ms],
            }?;
        } else {
            // Collect existing data
            let mut timestamp_ms: Vec<i64> = self.df.column("timestamp_ms")?.i64()?.into_iter().map(|v| v.unwrap_or(0)).collect();
            let mut open: Vec<f64> = self.df.column("open")?.f64()?.into_iter().map(|v| v.unwrap_or(0.0)).collect();
            let mut high: Vec<f64> = self.df.column("high")?.f64()?.into_iter().map(|v| v.unwrap_or(0.0)).collect();
            let mut low: Vec<f64> = self.df.column("low")?.f64()?.into_iter().map(|v| v.unwrap_or(0.0)).collect();
            let mut close: Vec<f64> = self.df.column("close")?.f64()?.into_iter().map(|v| v.unwrap_or(0.0)).collect();
            let mut volume: Vec<f64> = self.df.column("volume")?.f64()?.into_iter().map(|v| v.unwrap_or(0.0)).collect();
            let mut is_closed: Vec<bool> = self.df.column("is_closed")?.bool()?.into_iter().map(|v| v.unwrap_or(false)).collect();
            let mut received_at_ms: Vec<i64> = self.df.column("received_at_ms")?.i64()?.into_iter().map(|v| v.unwrap_or(0)).collect();

            // Add new row
            timestamp_ms.push(kline.timestamp_ms);
            open.push(kline.open);
            high.push(kline.high);
            low.push(kline.low);
            close.push(kline.close);
            volume.push(kline.volume);
            is_closed.push(kline.is_closed);
            received_at_ms.push(kline.received_at_ms);

            // Rebuild DataFrame
            self.df = df! {
                "timestamp_ms" => timestamp_ms,
                "open" => open,
                "high" => high,
                "low" => low,
                "close" => close,
                "volume" => volume,
                "is_closed" => is_closed,
                "received_at_ms" => received_at_ms,
            }?;
        }

        // Update warm-up status
        if !self.is_warmed_up && self.df.height() >= self.warmup_rows {
            self.is_warmed_up = true;
            info!("DataFrame warm-up complete. Ready for signal export.");
        }

        Ok(self.df.height())
    }

    /// Update the current (last) kline in place
    ///
    /// Used for updating the active candle before it closes.
    ///
    /// # Arguments
    /// * `kline` - Updated kline data
    ///
    /// # Returns
    /// true if updated, false if no kline to update
    pub fn update_current_kline(&mut self, kline: &Kline) -> Result<bool> {
        if self.df.height() == 0 {
            return Ok(false);
        }

        let last_idx = self.df.height() - 1;
        
        // Check if timestamp matches
        let ts_col = self.df.column("timestamp_ms")?;
        let current_ts = ts_col.get(last_idx)?;
        if current_ts.extract::<i64>() != Some(kline.timestamp_ms) {
            // Different candle, add as new
            self.add_kline(kline)?;
            return Ok(true);
        }

        // For in-place update, we need to use with_column to replace columns
        // This is less efficient but works with Polars 0.46 API
        let mut open_data: Vec<f64> = self.df.column("open")?.f64()?.into_iter().map(|v| v.unwrap_or(0.0)).collect();
        let mut high_data: Vec<f64> = self.df.column("high")?.f64()?.into_iter().map(|v| v.unwrap_or(0.0)).collect();
        let mut low_data: Vec<f64> = self.df.column("low")?.f64()?.into_iter().map(|v| v.unwrap_or(0.0)).collect();
        let mut close_data: Vec<f64> = self.df.column("close")?.f64()?.into_iter().map(|v| v.unwrap_or(0.0)).collect();
        let mut volume_data: Vec<f64> = self.df.column("volume")?.f64()?.into_iter().map(|v| v.unwrap_or(0.0)).collect();
        let mut is_closed_data: Vec<bool> = self.df.column("is_closed")?.bool()?.into_iter().map(|v| v.unwrap_or(false)).collect();

        open_data[last_idx] = kline.open;
        high_data[last_idx] = kline.high;
        low_data[last_idx] = kline.low;
        close_data[last_idx] = kline.close;
        volume_data[last_idx] = kline.volume;
        is_closed_data[last_idx] = kline.is_closed;

        // Replace columns
        self.df = self.df.drop("open")?.hstack(&[Series::new("open".into(), open_data).into()])?;
        self.df = self.df.drop("high")?.hstack(&[Series::new("high".into(), high_data).into()])?;
        self.df = self.df.drop("low")?.hstack(&[Series::new("low".into(), low_data).into()])?;
        self.df = self.df.drop("close")?.hstack(&[Series::new("close".into(), close_data).into()])?;
        self.df = self.df.drop("volume")?.hstack(&[Series::new("volume".into(), volume_data).into()])?;
        self.df = self.df.drop("is_closed")?.hstack(&[Series::new("is_closed".into(), is_closed_data).into()])?;

        Ok(true)
    }

    /// Get the current DataFrame reference
    pub fn df(&self) -> &DataFrame {
        &self.df
    }

    /// Get a mutable reference to the DataFrame
    pub fn df_mut(&mut self) -> &mut DataFrame {
        &mut self.df
    }

    /// Get the number of rows
    pub fn len(&self) -> usize {
        self.df.height()
    }

    /// Check if DataFrame is empty
    pub fn is_empty(&self) -> bool {
        self.df.is_empty()
    }

    /// Check if warm-up is complete
    pub fn is_warmed_up(&self) -> bool {
        self.is_warmed_up
    }

    /// Get the number of warm-up rows
    pub fn warmup_rows(&self) -> usize {
        self.warmup_rows
    }

    /// Get the number of working rows
    pub fn working_rows(&self) -> usize {
        self.working_rows
    }

    /// Get the working DataFrame (excluding warm-up rows)
    ///
    /// Returns a slice of the DataFrame starting from the warm-up boundary.
    pub fn working_df(&self) -> DataFrame {
        if !self.is_warmed_up || self.df.height() <= self.warmup_rows {
            return self.df.clone();
        }
        self.df.slice(self.warmup_rows as i64, self.working_rows)
    }

    /// Get the last N rows as a new DataFrame
    pub fn last_n(&self, n: usize) -> DataFrame {
        let actual_n = n.min(self.df.height());
        if actual_n == 0 {
            return DataFrame::empty();
        }
        let start = (self.df.height() - actual_n) as i64;
        self.df.slice(start, actual_n)
    }

    /// Get the latest kline
    pub fn latest_kline(&self) -> Option<Kline> {
        if self.df.height() == 0 {
            return None;
        }

        let last_idx = self.df.height() - 1;
        
        let timestamp_ms = self.df.column("timestamp_ms").ok()?.get(last_idx).ok()?.extract::<i64>()?;
        let open = self.df.column("open").ok()?.get(last_idx).ok()?.extract::<f64>()?;
        let high = self.df.column("high").ok()?.get(last_idx).ok()?.extract::<f64>()?;
        let low = self.df.column("low").ok()?.get(last_idx).ok()?.extract::<f64>()?;
        let close = self.df.column("close").ok()?.get(last_idx).ok()?.extract::<f64>()?;
        let volume = self.df.column("volume").ok()?.get(last_idx).ok()?.extract::<f64>()?;
        let is_closed = self.df.column("is_closed").ok()?.get(last_idx).ok()?.extract::<i64>()? != 0;
        let received_at_ms = self.df.column("received_at_ms").ok()?.get(last_idx).ok()?.extract::<i64>()?;

        Some(Kline {
            symbol: "UNKNOWN".to_string(), // DataFrame doesn't track symbol
            timestamp_ms,
            open,
            high,
            low,
            close,
            volume,
            is_closed,
            received_at_ms,
        })
    }

    /// Get a column as a contiguous Vec<f64>
    ///
    /// This is useful for passing to calculation functions that need slices.
    pub fn get_column_slice(&self, column_name: &str) -> Result<Vec<f64>> {
        let series = self.df.column(column_name)
            .with_context(|| format!("Column '{}' not found", column_name))?;
        
        let f64_series = series.cast(&DataType::Float64)
            .context("Failed to cast column to Float64")?;
        
        let chunked = f64_series.f64()
            .context("Failed to downcast to Float64Chunked")?;
        
        // Use cont_slice for zero-copy if possible, otherwise copy
        match chunked.cont_slice() {
            Ok(slice) => Ok(slice.to_vec()),
            Err(_) => {
                // Fallback: collect from iterator
                Ok(chunked.into_iter().filter_map(|v| v).collect())
            }
        }
    }

    /// Clear all data from the DataFrame
    pub fn clear(&mut self) {
        self.df = DataFrame::empty();
        self.is_warmed_up = false;
        self.init_schema().ok();
    }

    /// Get a LazyFrame for optimized queries
    pub fn lazy(&self) -> LazyFrame {
        self.df.clone().lazy()
    }

    /// Get a LazyFrame for the working data only
    pub fn working_lazy(&self) -> LazyFrame {
        self.working_df().lazy()
    }
}

impl Default for DataFrameManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Calculate typical price (HLC3) column
pub fn add_typical_price_column(df: &mut DataFrame) -> Result<()> {
    let high = df.column("high")?.f64()?;
    let low = df.column("low")?.f64()?;
    let close = df.column("close")?.f64()?;

    let typical_price: Vec<Option<f64>> = high
        .into_iter()
        .zip(low.into_iter())
        .zip(close.into_iter())
        .map(|((h, l), c)| {
            match (h, l, c) {
                (Some(h), Some(l), Some(c)) => Some((h + l + c) / 3.0),
                _ => None,
            }
        })
        .collect();

    df.with_column(Series::new("typical_price".into(), typical_price))
        .context("Failed to add typical_price column")?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_kline(ts: i64, close: f64) -> Kline {
        Kline {
            timestamp_ms: ts,
            open: close - 1.0,
            high: close + 2.0,
            low: close - 2.0,
            close,
            volume: 100.0,
            is_closed: true,
            received_at_ms: ts,
        }
    }

    #[test]
    fn test_dataframe_manager_creation() {
        let manager = DataFrameManager::new();
        assert!(manager.is_empty());
        assert!(!manager.is_warmed_up());
        assert_eq!(manager.warmup_rows(), DEFAULT_WARMUP_SIZE);
        assert_eq!(manager.working_rows(), DEFAULT_WORKING_WINDOW);
    }

    #[test]
    fn test_dataframe_manager_custom_size() {
        let manager = DataFrameManager::with_size(200, 50);
        assert_eq!(manager.warmup_rows(), 50);
        assert_eq!(manager.working_rows(), 150);
    }

    #[test]
    fn test_init_from_klines() {
        let mut manager = DataFrameManager::with_size(1100, 100);
        let klines: Vec<Kline> = (0..1100)
            .map(|i| create_test_kline(i as i64 * 60000, 50000.0 + i as f64))
            .collect();

        let n = manager.init_from_klines(&klines).unwrap();
        assert_eq!(n, 1100);
        assert_eq!(manager.len(), 1100);
        assert!(manager.is_warmed_up());
    }

    #[test]
    fn test_add_kline() {
        let mut manager = DataFrameManager::with_size(10, 3);
        
        // Add klines
        for i in 0..15 {
            let kline = create_test_kline(i as i64 * 60000, 50000.0);
            manager.add_kline(&kline).unwrap();
        }

        // Should be capped at max_rows
        assert_eq!(manager.len(), 10);
        // Warmup is 3, so after 3 adds we should be warmed up
        assert!(manager.is_warmed_up());
    }

    #[test]
    fn test_warmup_detection() {
        let mut manager = DataFrameManager::with_size(10, 5);
        
        // Initially not warmed up (no data)
        assert!(!manager.is_warmed_up());
        
        // Add klines up to warmup threshold
        for i in 0..5 {
            let kline = create_test_kline(i as i64 * 60000, 50000.0);
            manager.add_kline(&kline).unwrap();
        }

        // After adding 5 rows (warmup threshold), should be warmed up
        assert!(manager.is_warmed_up());
    }

    #[test]
    fn test_working_df() {
        let mut manager = DataFrameManager::with_size(10, 3);
        
        let klines: Vec<Kline> = (0..10)
            .map(|i| create_test_kline(i as i64, 50000.0 + i as f64))
            .collect();
        manager.init_from_klines(&klines).unwrap();

        let working = manager.working_df();
        assert_eq!(working.height(), 7); // 10 - 3 warmup
    }

    #[test]
    fn test_last_n() {
        let mut manager = DataFrameManager::new();
        
        let klines: Vec<Kline> = (0..100)
            .map(|i| create_test_kline(i as i64, 50000.0))
            .collect();
        manager.init_from_klines(&klines).unwrap();

        let last_10 = manager.last_n(10);
        assert_eq!(last_10.height(), 10);
        
        let last_200 = manager.last_n(200);
        assert_eq!(last_200.height(), 100); // Capped at actual size
    }

    #[test]
    fn test_latest_kline() {
        let mut manager = DataFrameManager::new();
        
        // Empty DataFrame should return None
        assert!(manager.latest_kline().is_none());
        
        let kline = create_test_kline(12345, 50100.0);
        manager.add_kline(&kline).unwrap();
        
        let latest = manager.latest_kline().unwrap();
        assert_eq!(latest.timestamp_ms, 12345);
        assert_eq!(latest.close, 50100.0);
    }

    #[test]
    fn test_get_column_slice() {
        let mut manager = DataFrameManager::new();
        
        let klines: Vec<Kline> = (0..10)
            .map(|i| create_test_kline(i as i64, 50000.0 + i as f64))
            .collect();
        manager.init_from_klines(&klines).unwrap();

        let close_prices = manager.get_column_slice("close").unwrap();
        assert_eq!(close_prices.len(), 10);
        assert!((close_prices[0] - 50000.0).abs() < 0.01);
        assert!((close_prices[9] - 50009.0).abs() < 0.01);
    }

    #[test]
    fn test_clear() {
        let mut manager = DataFrameManager::new();
        
        let klines: Vec<Kline> = (0..10)
            .map(|i| create_test_kline(i as i64, 50000.0))
            .collect();
        manager.init_from_klines(&klines).unwrap();
        
        assert_eq!(manager.len(), 10);
        // 10 rows < 100 warmup default, so not warmed up
        assert!(!manager.is_warmed_up());
        
        manager.clear();
        
        assert!(manager.is_empty());
        assert!(!manager.is_warmed_up());
    }
}
