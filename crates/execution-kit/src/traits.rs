//! ExchangeClient trait for unified Spot & Futures API.

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Market type enum
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MarketType {
    Spot,
    Futures,
}

/// Order request structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderRequest {
    pub symbol: String,
    pub side: String,       // BUY | SELL
    pub order_type: String, // MARKET | LIMIT
    pub quantity: f64,
    pub price: Option<f64>,
    pub time_in_force: Option<String>,
    pub post_only: bool,
    pub client_order_id: String,
}

/// Order response structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderResponse {
    pub order_id: u64,
    pub client_order_id: String,
    pub status: String,      // NEW, FILLED, PARTIALLY_FILLED, CANCELED, EXPIRED
    pub filled_qty: f64,
    pub avg_price: Option<f64>,
    pub timestamp: i64,
}

/// Position information
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PositionInfo {
    pub symbol: String,
    pub side: String,
    pub quantity: f64,
    pub entry_price: f64,
    pub mark_price: f64,
    pub unrealized_pnl: f64,
    pub unrealized_pnl_percent: f64,
    pub leverage: u32,
    pub margin_type: String, // ISOLATED | CROSSED
}

/// Account balance
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AccountBalance {
    pub total_balance: f64,
    pub available_balance: f64,
    pub total_unrealized_pnl: f64,
    pub timestamp: i64,
}

/// ExchangeClient trait for Spot & Futures
#[async_trait]
pub trait ExchangeClient: Send + Sync {
    /// Get market type
    fn market_type(&self) -> MarketType;

    /// Place an order
    async fn place_order(&self, order: &OrderRequest) -> Result<OrderResponse>;

    /// Cancel an order
    async fn cancel_order(&self, symbol: &str, order_id: &str) -> Result<()>;

    /// Get position info
    async fn get_position(&self, symbol: &str) -> Result<PositionInfo>;

    /// Get account balance
    async fn get_account_balance(&self) -> Result<AccountBalance>;

    /// Emergency close all positions
    async fn emergency_close_all(&self) -> Result<()>;

    /// Get API endpoint base URL
    fn base_url(&self) -> &str;
}
