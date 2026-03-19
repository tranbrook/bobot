//! Core processing engine for the Muscle signal engine.
//!
//! This module contains the core signal processing components:
//! - Ultimate Smoother recursive digital filter
//! - UltimateBands (Bollinger Bands variant)
//! - UltimateChannel (Keltner Channel variant)
//! - DataFrame manager for rolling window operations

pub mod smoother;
pub mod bands;
pub mod channel;
pub mod dataframe;

pub use smoother::{
    compute_ultimate_smoother,
    compute_ultimate_smoother_default,
    compute_ultimate_smoother_from_series,
    compute_ultimate_smoother_from_series_default,
    extract_f64_from_series,
    add_ultimate_smoother_column,
    add_ultimate_smoother_column_default,
    DEFAULT_C1,
    DEFAULT_C2,
    DEFAULT_C3,
};

pub use bands::{
    UltimateBands,
    calculate_ultimate_bands,
    calculate_ultimate_bands_default,
    add_ultimate_bands_columns,
    check_band_touch,
    calculate_band_exhaustion,
    DEFAULT_STDDEV_MULTIPLIER,
    DEFAULT_WINDOW_SIZE,
};

pub use channel::{
    UltimateChannel,
    calculate_ultimate_channel,
    calculate_ultimate_channel_default,
    calculate_true_range,
    calculate_atr,
    check_channel_touch,
    calculate_channel_signal,
    DEFAULT_ATR_MULTIPLIER,
    DEFAULT_ATR_PERIOD,
};

pub use dataframe::{
    DataFrameManager,
    add_typical_price_column,
    DEFAULT_WORKING_WINDOW,
    DEFAULT_WARMUP_SIZE,
    DEFAULT_TOTAL_SIZE,
};
