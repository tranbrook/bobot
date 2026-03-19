//! ZeroClaw Execution Kit - Binance Spot & Futures Execution Engine.
//!
//! This crate provides a robust, high-performance execution engine that:
//! - Listens to Redis commands (execution:queue:{symbol})
//! - Executes trades on Binance (Spot & Futures)
//! - Maintains real-time position feedback via WebSocket
//!
//! ## Architecture
//!
//! ```text
//! Redis (execution:queue) → Execution Engine → Binance API
//!                              ↓
//!                       WebSocket (User Data Stream)
//!                              ↓
//!                       Redis (market:position)
//! ```

pub mod traits;
pub mod spot_client;
pub mod futures_client;
pub mod redis_listener;
pub mod execution_engine;
pub mod formatter;

pub use traits::{ExchangeClient, MarketType};
pub use spot_client::SpotClient;
pub use futures_client::FuturesClient;
pub use redis_listener::RedisListener;
pub use execution_engine::ExecutionEngine;
pub use formatter::PrecisionFormatter;

/// Execution Kit version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
