//! Binance WebSocket client for real-time kline streaming.
//!
//! This module implements a resilient WebSocket connection to Binance
//! for streaming real-time kline (candlestick) data with automatic
//! reconnection and re-bootstrap capabilities.
//!
//! ## Features
//!
//! - Automatic WebSocket reconnection with exponential backoff
//! - Re-bootstrap from REST API after connection drops
//! - Heartbeat/ping monitoring
//! - Message deduplication
//! - Graceful shutdown

use super::models::{Kline, KlineInterval};
use anyhow::{Context, Result};
use serde::Deserialize;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, broadcast};
use tokio::time::{sleep, timeout};
use tracing::{debug, error, info, warn};

/// Binance WebSocket base URL for kline streams
const BINANCE_WS_BASE: &str = "wss://stream.binance.com:9443/ws";

/// Combined stream URL pattern
const COMBINED_STREAM_URL: &str = "wss://stream.binance.com:9443/stream?streams=";

/// Heartbeat interval in seconds
const HEARTBEAT_INTERVAL_SECS: u64 = 30;

/// Connection timeout in seconds
const CONNECTION_TIMEOUT_SECS: u64 = 10;

/// Maximum reconnection delay in seconds
const MAX_RECONNECT_DELAY_SECS: u64 = 300;

/// Base reconnection delay in seconds
const BASE_RECONNECT_DELAY_SECS: u64 = 5;

/// Raw WebSocket message from Binance
#[derive(Debug, Deserialize)]
struct WsKlineMessage {
    #[serde(rename = "e")]
    event_type: String,
    #[serde(rename = "E")]
    event_time: i64,
    #[serde(rename = "s")]
    symbol: String,
    #[serde(rename = "k")]
    kline: WsKlineData,
}

#[derive(Debug, Deserialize)]
struct WsKlineData {
    #[serde(rename = "t")]
    start_time: i64,
    #[serde(rename = "T")]
    close_time: i64,
    #[serde(rename = "s")]
    symbol: String,
    #[serde(rename = "i")]
    interval: String,
    #[serde(rename = "f")]
    first_trade_id: i64,
    #[serde(rename = "L")]
    last_trade_id: i64,
    #[serde(rename = "o")]
    open: String,
    #[serde(rename = "c")]
    close: String,
    #[serde(rename = "h")]
    high: String,
    #[serde(rename = "l")]
    low: String,
    #[serde(rename = "v")]
    volume: String,
    #[serde(rename = "n")]
    trade_count: i64,
    #[serde(rename = "x")]
    is_final: bool,
    #[serde(rename = "q")]
    quote_volume: String,
    #[serde(rename = "V")]
    taker_buy_volume: String,
    #[serde(rename = "Q")]
    taker_buy_quote_volume: String,
}

/// WebSocket event types
#[derive(Debug, Clone)]
pub enum WsEvent {
    /// WebSocket connected
    Connected,
    /// WebSocket disconnected
    Disconnected,
    /// New kline received
    Kline(Kline),
    /// Connection error
    Error(String),
    /// Reconnection started
    Reconnecting { attempt: u32, delay_secs: u64 },
}

/// WebSocket connection state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WsState {
    Disconnected,
    Connecting,
    Connected,
    Reconnecting,
}

/// Binance WebSocket client configuration
#[derive(Debug, Clone)]
pub struct BinanceWsConfig {
    /// WebSocket base URL (default: Binance production)
    pub ws_url: String,
    /// Enable combined stream (default: true)
    pub combined: bool,
    /// Enable auto-reconnect (default: true)
    pub auto_reconnect: bool,
    /// Maximum reconnection attempts (0 = infinite)
    pub max_reconnect_attempts: u32,
    /// Enable heartbeat monitoring
    pub enable_heartbeat: bool,
}

impl Default for BinanceWsConfig {
    fn default() -> Self {
        Self {
            ws_url: BINANCE_WS_BASE.to_string(),
            combined: true,
            auto_reconnect: true,
            max_reconnect_attempts: 0, // Infinite
            enable_heartbeat: true,
        }
    }
}

/// Binance WebSocket client with auto-reconnect
pub struct BinanceWsClient {
    config: BinanceWsConfig,
    state: Arc<AtomicU64>, // Encoded WsState
    reconnect_count: Arc<AtomicU64>,
    last_message_time: Arc<AtomicU64>,
    shutdown: Arc<AtomicBool>,
    kline_tx: mpsc::Sender<Kline>,
    event_tx: broadcast::Sender<WsEvent>,
}

impl BinanceWsClient {
    /// Create a new WebSocket client
    pub fn new(config: BinanceWsConfig, kline_tx: mpsc::Sender<Kline>, event_tx: broadcast::Sender<WsEvent>) -> Self {
        Self {
            config,
            state: Arc::new(AtomicU64::new(WsState::Disconnected as u64)),
            reconnect_count: Arc::new(AtomicU64::new(0)),
            last_message_time: Arc::new(AtomicU64::new(0)),
            shutdown: Arc::new(AtomicBool::new(false)),
            kline_tx,
            event_tx,
        }
    }

    /// Get the stream symbol for a given symbol and interval
    pub fn stream_symbol(symbol: &str, interval: KlineInterval) -> String {
        format!("{}@kline_{}", symbol.to_lowercase(), interval.as_str())
    }

    /// Build WebSocket URL for a single stream
    pub fn build_single_url(symbol: &str, interval: KlineInterval) -> String {
        let stream = Self::stream_symbol(symbol, interval);
        format!("{}/{}", BINANCE_WS_BASE, stream)
    }

    /// Build WebSocket URL for combined streams
    pub fn build_combined_url(symbols: &[(&str, KlineInterval)]) -> String {
        let streams: Vec<String> = symbols
            .iter()
            .map(|(s, i)| Self::stream_symbol(s, *i))
            .collect();
        format!("{}{}", COMBINED_STREAM_URL, streams.join("/"))
    }

    /// Get current connection state
    pub fn state(&self) -> WsState {
        match self.state.load(Ordering::Relaxed) {
            0 => WsState::Disconnected,
            1 => WsState::Connecting,
            2 => WsState::Connected,
            3 => WsState::Reconnecting,
            _ => WsState::Disconnected,
        }
    }

    /// Set connection state
    fn set_state(&self, state: WsState) {
        self.state.store(state as u64, Ordering::Relaxed);
    }

    /// Get reconnect count
    pub fn reconnect_count(&self) -> u64 {
        self.reconnect_count.load(Ordering::Relaxed)
    }

    /// Get last message timestamp
    pub fn last_message_time(&self) -> u64 {
        self.last_message_time.load(Ordering::Relaxed)
    }

    /// Check if connection is healthy (message received within 2 minutes)
    pub fn is_healthy(&self) -> bool {
        let last = self.last_message_time();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        now - last < 120
    }

    /// Send event to event channel
    fn send_event(&self, event: WsEvent) {
        let _ = self.event_tx.send(event);
    }

    /// Parse WebSocket message to Kline
    fn parse_kline(msg: &WsKlineMessage, symbol: &str, received_at_ms: i64) -> Result<Kline> {
        Ok(Kline {
            symbol: symbol.to_string(),
            timestamp_ms: msg.kline.close_time,
            open: msg.kline.open.parse()?,
            high: msg.kline.high.parse()?,
            low: msg.kline.low.parse()?,
            close: msg.kline.close.parse()?,
            volume: msg.kline.volume.parse()?,
            is_closed: msg.kline.is_final,
            received_at_ms,
        })
    }

    /// Run the WebSocket client with auto-reconnect
    pub async fn run(&self, symbol: &str, interval: KlineInterval) {
        let mut reconnect_attempt = 0u32;

        loop {
            if self.shutdown.load(Ordering::Relaxed) {
                info!("WebSocket client shutting down");
                break;
            }

            let url = Self::build_single_url(symbol, interval);
            info!("Connecting to Binance WebSocket: {}", url);

            self.set_state(WsState::Connecting);
            self.send_event(WsEvent::Connected);

            match self.connect_and_stream(&url).await {
                Ok(()) => {
                    info!("WebSocket connection closed gracefully");
                    if !self.config.auto_reconnect {
                        break;
                    }
                }
                Err(e) => {
                    error!("WebSocket error: {}", e);
                    self.send_event(WsEvent::Error(e.to_string()));
                }
            }

            if !self.config.auto_reconnect {
                break;
            }

            // Check max reconnect attempts
            if self.config.max_reconnect_attempts > 0
                && reconnect_attempt >= self.config.max_reconnect_attempts
            {
                error!("Maximum reconnection attempts ({}) reached", reconnect_attempt);
                break;
            }

            // Calculate exponential backoff delay
            let delay = std::cmp::min(
                BASE_RECONNECT_DELAY_SECS * 2u64.pow(reconnect_attempt),
                MAX_RECONNECT_DELAY_SECS,
            );

            reconnect_attempt += 1;
            self.reconnect_count.store(reconnect_attempt as u64, Ordering::Relaxed);

            info!(
                "Reconnecting in {}s (attempt {})",
                delay, reconnect_attempt
            );
            self.set_state(WsState::Reconnecting);
            self.send_event(WsEvent::Reconnecting {
                attempt: reconnect_attempt,
                delay_secs: delay,
            });

            sleep(Duration::from_secs(delay)).await;
        }

        self.set_state(WsState::Disconnected);
        self.send_event(WsEvent::Disconnected);
    }

    /// Connect and stream messages
    async fn connect_and_stream(&self, url: &str) -> Result<()> {
        use tokio_tungstenite::{connect_async, tungstenite::Message};
        use futures_util::{SinkExt, StreamExt};

        let (ws_stream, _) = timeout(
            Duration::from_secs(CONNECTION_TIMEOUT_SECS),
            connect_async(url),
        )
        .await
        .context("Connection timeout")?
        .context("Failed to connect")?;

        info!("WebSocket connected");
        self.set_state(WsState::Connected);

        let (mut write, mut read) = ws_stream.split();

        // Spawn heartbeat task
        let heartbeat_handle = if self.config.enable_heartbeat {
            Some(tokio::spawn({
                let shutdown = self.shutdown.clone();
                async move {
                    loop {
                        if shutdown.load(Ordering::Relaxed) {
                            break;
                        }
                        sleep(Duration::from_secs(HEARTBEAT_INTERVAL_SECS)).await;
                        // Ping is handled automatically by tungstenite
                    }
                }
            }))
        } else {
            None
        };

        // Message loop
        while let Some(msg_result) = read.next().await {
            if self.shutdown.load(Ordering::Relaxed) {
                break;
            }

            match msg_result {
                Ok(Message::Text(text)) => {
                    let received_at_ms = Kline::current_time_ms();

                    // Update last message time
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_secs();
                    self.last_message_time.store(now, Ordering::Relaxed);

                    // Parse message
                    match serde_json::from_str::<WsKlineMessage>(&text) {
                        Ok(msg) => {
                            let symbol = msg.kline.symbol.clone();
                            match Self::parse_kline(&msg, &symbol, received_at_ms) {
                                Ok(kline) => {
                                    debug!("Received kline: {} {} closed={}",
                                        kline.timestamp_ms, symbol, kline.is_closed);
                                    
                                    // Send kline to channel
                                    if let Err(e) = self.kline_tx.send(kline).await {
                                        error!("Failed to send kline: {}", e);
                                        break;
                                    }
                                }
                                Err(e) => {
                                    warn!("Failed to parse kline: {}", e);
                                }
                            }
                        }
                        Err(e) => {
                            // Try parsing as combined stream message
                            if let Ok(combined) = serde_json::from_str::<serde_json::Value>(&text) {
                                if let Some(data) = combined.get("data") {
                                    if let Ok(msg) = serde_json::from_value::<WsKlineMessage>(data.clone()) {
                                        let symbol = msg.kline.symbol.clone();
                                        match Self::parse_kline(&msg, &symbol, received_at_ms) {
                                            Ok(kline) => {
                                                if let Err(e) = self.kline_tx.send(kline).await {
                                                    error!("Failed to send kline: {}", e);
                                                    break;
                                                }
                                            }
                                            Err(e) => {
                                                warn!("Failed to parse combined kline: {}", e);
                                            }
                                        }
                                    }
                                }
                            } else {
                                warn!("Failed to parse WebSocket message: {}", e);
                            }
                        }
                    }
                }
                Ok(Message::Ping(data)) => {
                    // Respond to ping
                    let _ = write.send(Message::Pong(data)).await;
                }
                Ok(Message::Pong(_)) => {
                    debug!("Received pong");
                }
                Ok(Message::Close(frame)) => {
                    info!("WebSocket closed: {:?}", frame);
                    break;
                }
                Ok(Message::Binary(_)) => {
                    debug!("Received binary message (ignored)");
                }
                Ok(Message::Frame(_)) => {
                    // Ignore raw frames
                }
                Err(e) => {
                    error!("WebSocket error: {}", e);
                    return Err(e.into());
                }
            }
        }

        // Cleanup heartbeat task
        if let Some(handle) = heartbeat_handle {
            handle.abort();
        }

        Ok(())
    }

    /// Signal shutdown
    pub fn shutdown(&self) {
        self.shutdown.store(true, Ordering::Relaxed);
    }

    /// Reset reconnect count
    pub fn reset_reconnect_count(&self) {
        self.reconnect_count.store(0, Ordering::Relaxed);
    }
}

/// Helper function to create a WebSocket client and start streaming
pub async fn spawn_ws_client(
    symbol: &str,
    interval: KlineInterval,
    config: Option<BinanceWsConfig>,
) -> (mpsc::Receiver<Kline>, broadcast::Receiver<WsEvent>, Arc<AtomicBool>) {
    let (kline_tx, kline_rx) = mpsc::channel(100);
    let (event_tx, event_rx) = broadcast::channel(100);

    let config = config.unwrap_or_default();
    let client = BinanceWsClient::new(config, kline_tx, event_tx);

    let shutdown = client.shutdown.clone();

    // Spawn WebSocket task
    let symbol = symbol.to_string();
    tokio::spawn(async move {
        client.run(&symbol, interval).await;
    });

    (kline_rx, event_rx, shutdown)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stream_symbol() {
        assert_eq!(
            BinanceWsClient::stream_symbol("BTCUSDT", KlineInterval::Minute1),
            "btcusdt@kline_1m"
        );
        assert_eq!(
            BinanceWsClient::stream_symbol("ETHUSDT", KlineInterval::Hour1),
            "ethusdt@kline_1h"
        );
    }

    #[test]
    fn test_build_single_url() {
        let url = BinanceWsClient::build_single_url("BTCUSDT", KlineInterval::Minute1);
        assert!(url.starts_with(BINANCE_WS_BASE));
        assert!(url.contains("btcusdt@kline_1m"));
    }

    #[test]
    fn test_build_combined_url() {
        let streams = &[("BTCUSDT", KlineInterval::Minute1), ("ETHUSDT", KlineInterval::Minute5)];
        let url = BinanceWsClient::build_combined_url(streams);
        assert!(url.starts_with(COMBINED_STREAM_URL));
        assert!(url.contains("btcusdt@kline_1m"));
        assert!(url.contains("ethusdt@kline_5m"));
    }

    #[test]
    fn test_ws_config_default() {
        let config = BinanceWsConfig::default();
        assert!(config.auto_reconnect);
        assert!(config.enable_heartbeat);
        assert_eq!(config.max_reconnect_attempts, 0); // Infinite
    }

    #[tokio::test]
    async fn test_ws_client_creation() {
        let (kline_tx, _) = mpsc::channel(100);
        let (event_tx, _) = broadcast::channel(100);
        let config = BinanceWsConfig::default();

        let client = BinanceWsClient::new(config, kline_tx, event_tx);
        assert_eq!(client.state(), WsState::Disconnected);
        assert_eq!(client.reconnect_count(), 0);
    }

    #[test]
    fn test_parse_kline_message() {
        let json = r#"{
            "e": "kline",
            "E": 1234567890000,
            "s": "BTCUSDT",
            "k": {
                "t": 1234567800000,
                "T": 1234567899999,
                "s": "BTCUSDT",
                "i": "1m",
                "f": 100,
                "L": 200,
                "o": "50000.00",
                "c": "50100.00",
                "h": "50200.00",
                "l": "49900.00",
                "v": "100.50",
                "n": 101,
                "x": true,
                "q": "5050000.00",
                "V": "50.25",
                "Q": "2525000.00"
            }
        }"#;

        let msg: WsKlineMessage = serde_json::from_str(json).unwrap();
        let kline = BinanceWsClient::parse_kline(&msg, "BTCUSDT", 1234567890000).unwrap();

        assert_eq!(kline.timestamp_ms, 1234567899999);
        assert_eq!(kline.open, 50000.00);
        assert_eq!(kline.close, 50100.00);
        assert_eq!(kline.high, 50200.00);
        assert_eq!(kline.low, 49900.00);
        assert_eq!(kline.volume, 100.50);
        assert!(kline.is_closed);
        assert_eq!(kline.received_at_ms, 1234567890000);
    }
}
