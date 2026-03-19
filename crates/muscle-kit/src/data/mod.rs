//! Data ingestion module for the Muscle signal engine.
//!
//! This module handles all data-related operations including:
//! - Historical klines fetch from Binance REST API
//! - Real-time WebSocket stream subscription
//! - Data models and serialization

pub mod models;
pub mod binance_rest;
pub mod binance_ws;

pub use models::{Kline, KlineInterval, MarketRegime, Signal, SignalOutput, IndicatorSnapshot};
pub use binance_rest::{BinanceRestClient, BootstrapDataLoader, fetch_bootstrap_klines};
pub use binance_ws::{
    BinanceWsClient, BinanceWsConfig, WsEvent, WsState,
    spawn_ws_client,
};
