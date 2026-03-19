//! Muscle Kit - High-Frequency Trading Signal Engine.
//!
//! "The Muscle" is a high-frequency trading signal engine that:
//! - Fetches historical market data from Binance (REST API)
//! - Streams real-time klines via WebSocket
//! - Processes data using Polars.rs DataFrames
//! - Implements the Ultimate Smoother recursive digital filter
//! - Generates trading signals using multiple strategies
//! - Exports signals to Redis for downstream consumption
//!
//! ## Architecture
//!
//! ```text
//! Data Ingestion → Polars DataFrame → Ultimate Smoother → Strategies → Gatekeeper → Redis
//! ```
//!
//! ## Module Structure
//!
//! - `data/` - Data models and ingestion (REST, WebSocket)
//! - `engine/` - Core processing (DataFrame, Smoother, Bands, Channel)
//! - `strategies/` - Trading strategies and market regime detection
//! - `aggregator.rs` - Signal aggregation and voting
//! - `redis_store.rs` - Redis export client
//! - `traits.rs` - Core traits (TradingStrategy)
//! - `config.rs` - Configuration schema
//!
//! ## Example Usage
//!
//! ```rust,no_run
//! use zeroclaw_muscle_kit::{MuscleConfig, Gatekeeper, RedisSignalStore};
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let config = MuscleConfig::default();
//!     
//!     // Create Gatekeeper for signal aggregation
//!     let gatekeeper = Gatekeeper::new();
//!     
//!     // Create Redis store for signal export
//!     let store = RedisSignalStore::new(&config.redis_url, Some("market:signal"))?;
//!     
//!     // ... use the engine
//!     
//!     Ok(())
//! }
//! ```

pub mod config;
pub mod data;
pub mod engine;
pub mod strategies;
pub mod traits;
pub mod aggregator;
pub mod redis_store;
pub mod campaign;

pub use config::MuscleConfig;
pub use campaign::{CampaignConfig, CampaignManager, Campaign};
pub use data::{
    Kline, KlineInterval, MarketRegime, Signal, SignalOutput, IndicatorSnapshot,
    BinanceRestClient, BootstrapDataLoader, fetch_bootstrap_klines,
};
pub use engine::{
    compute_ultimate_smoother,
    compute_ultimate_smoother_default,
    extract_f64_from_series,
    DEFAULT_C1,
    DEFAULT_C2,
    DEFAULT_C3,
};
pub use aggregator::Gatekeeper;
pub use redis_store::RedisSignalStore;

/// Muscle Kit version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Default warm-up candles for indicator convergence
pub const DEFAULT_WARMUP_CANDLES: usize = 100;

/// Default working window size
pub const DEFAULT_WORKING_CANDLES: usize = 1000;

/// Default total candles to fetch (warmup + working)
pub const DEFAULT_TOTAL_CANDLES: usize = DEFAULT_WARMUP_CANDLES + DEFAULT_WORKING_CANDLES;

/// Default maximum allowed ingestion latency in milliseconds
pub const DEFAULT_MAX_LATENCY_MS: i64 = 500;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_constants() {
        assert_eq!(DEFAULT_WARMUP_CANDLES, 100);
        assert_eq!(DEFAULT_WORKING_CANDLES, 1000);
        assert_eq!(DEFAULT_TOTAL_CANDLES, 1100);
        assert_eq!(DEFAULT_MAX_LATENCY_MS, 500);
    }

    #[test]
    fn test_version() {
        assert!(!VERSION.is_empty());
    }
}
