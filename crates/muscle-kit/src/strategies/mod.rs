//! Trading strategies module.
//!
//! This module contains all trading strategy implementations:
//! - Market Regime Detector
//! - Strategy A: Mean Reversion
//! - Strategy B: Trend Following

pub mod regime;
pub mod mean_reversion;
pub mod trend_following;

pub use regime::{
    calculate_adx,
    calculate_bb_width,
    detect_regime,
    calculate_market_regime,
    get_latest_regime,
    DEFAULT_ADX_PERIOD,
    DEFAULT_BB_PERIOD,
    TRENDING_ADX_THRESHOLD,
    RANGING_ADX_THRESHOLD,
    VOLATILE_BB_WIDTH_THRESHOLD,
    RANGING_BB_WIDTH_THRESHOLD,
};

pub use mean_reversion::MeanReversionStrategy;
pub use trend_following::TrendFollowingStrategy;
