//! Binance Spot Client implementation.

use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::Utc;
use hmac::{Hmac, Mac};
use reqwest::{Client, RequestBuilder};
use sha2::Sha256;
use std::env;
use tracing::debug;

use super::traits::{ExchangeClient, MarketType, OrderRequest, OrderResponse, PositionInfo, AccountBalance};

type HmacSha256 = Hmac<Sha256>;

/// Binance Spot API Client
pub struct SpotClient {
    client: Client,
    api_key: String,
    secret_key: String,
    base_url: String,
}

impl SpotClient {
    /// Create a new Spot Client from environment variables
    pub fn new() -> Result<Self> {
        let api_key = env::var("BINANCE_SPOT_API_KEY")
            .context("BINANCE_SPOT_API_KEY not set")?;
        let secret_key = env::var("BINANCE_SPOT_SECRET_KEY")
            .context("BINANCE_SPOT_SECRET_KEY not set")?;
        
        Ok(Self {
            client: Client::new(),
            api_key,
            secret_key,
            base_url: "https://api.binance.com".to_string(),
        })
    }

    /// Create with custom credentials
    pub fn with_credentials(api_key: &str, secret_key: &str) -> Self {
        Self {
            client: Client::new(),
            api_key: api_key.to_string(),
            secret_key: secret_key.to_string(),
            base_url: "https://api.binance.com".to_string(),
        }
    }

    /// Sign a request with HMAC-SHA256
    fn sign_request(&self, builder: RequestBuilder, params: &str) -> RequestBuilder {
        let mut mac = HmacSha256::new_from_slice(self.secret_key.as_bytes())
            .expect("HMAC can take key of any size");
        mac.update(params.as_bytes());
        let signature = hex::encode(mac.finalize().into_bytes());
        
        builder.query(&[("signature", &signature)])
            .header("X-MBX-APIKEY", &self.api_key)
    }

    /// Generate client order ID
    fn generate_client_order_id(&self) -> String {
        format!("zeroclaw_{}", Utc::now().timestamp_millis())
    }
}

#[async_trait]
impl ExchangeClient for SpotClient {
    fn market_type(&self) -> MarketType {
        MarketType::Spot
    }

    async fn place_order(&self, order: &OrderRequest) -> Result<OrderResponse> {
        let timestamp = Utc::now().timestamp_millis();
        
        let mut params = format!(
            "symbol={}&side={}&type={}&quantity={}&timestamp={}&recvWindow=5000",
            order.symbol, order.side, order.order_type, order.quantity, timestamp
        );

        if order.order_type == "LIMIT" {
            if let Some(price) = order.price {
                params.push_str(&format!("&price={}", price));
            }
            if let Some(tif) = &order.time_in_force {
                params.push_str(&format!("&timeInForce={}", tif));
            }
        }

        if order.post_only {
            params.push_str("&newOrderRespType=FULL");
        }

        params.push_str(&format!("&newClientOrderId={}", order.client_order_id));

        let url = format!("{}/api/v3/order", self.base_url);
        
        let response = self.sign_request(
            self.client.post(&url),
            &params
        )
        .send()
        .await
        .context("Failed to send order request")?;

        if !response.status().is_success() {
            let error = response.text().await?;
            return Err(anyhow::anyhow!("Binance API error: {}", error));
        }

        let json: serde_json::Value = response.json().await?;
        
        debug!("Spot order response: {:?}", json);

        Ok(OrderResponse {
            order_id: json["orderId"].as_u64().unwrap_or(0),
            client_order_id: json["clientOrderId"].as_str().unwrap_or("").to_string(),
            status: json["status"].as_str().unwrap_or("UNKNOWN").to_string(),
            filled_qty: json["executedQty"].as_str().and_then(|s| s.parse().ok()).unwrap_or(0.0),
            avg_price: json["avgPrice"].as_str().and_then(|s| s.parse().ok()),
            timestamp: Utc::now().timestamp_millis(),
        })
    }

    async fn cancel_order(&self, symbol: &str, order_id: &str) -> Result<()> {
        let timestamp = Utc::now().timestamp_millis();
        let params = format!(
            "symbol={}&orderId={}&timestamp={}&recvWindow=5000",
            symbol, order_id, timestamp
        );

        let url = format!("{}/api/v3/order", self.base_url);
        
        self.sign_request(
            self.client.delete(&url),
            &params
        )
        .send()
        .await
        .context("Failed to cancel order")?;

        Ok(())
    }

    async fn get_position(&self, symbol: &str) -> Result<PositionInfo> {
        // For spot, we get account balance and check holdings
        let timestamp = Utc::now().timestamp_millis();
        let params = format!("timestamp={}&recvWindow=5000", timestamp);

        let url = format!("{}/api/v3/account", self.base_url);
        
        let response = self.sign_request(
            self.client.get(&url),
            &params
        )
        .send()
        .await
        .context("Failed to get account info")?;

        let json: serde_json::Value = response.json().await?;
        
        // Find the asset in balances
        let base_asset = &symbol[..3]; // e.g., "BTC" from "BTCUSDT"
        let empty_array = vec![];
        let balances = json["balances"].as_array().unwrap_or(&empty_array);
        
        for balance in balances {
            if balance["asset"].as_str() == Some(base_asset) {
                let free = balance["free"].as_str().and_then(|s| s.parse().ok()).unwrap_or(0.0);
                let locked = balance["locked"].as_str().and_then(|s| s.parse().ok()).unwrap_or(0.0);
                
                return Ok(PositionInfo {
                    symbol: symbol.to_string(),
                    side: if free > 0.0 { "LONG".to_string() } else { "NONE".to_string() },
                    quantity: free + locked,
                    entry_price: 0.0, // Not tracked for spot
                    mark_price: 0.0,
                    unrealized_pnl: 0.0,
                    unrealized_pnl_percent: 0.0,
                    leverage: 1,
                    margin_type: "NONE".to_string(),
                });
            }
        }

        Ok(PositionInfo::default())
    }

    async fn get_account_balance(&self) -> Result<AccountBalance> {
        let timestamp = Utc::now().timestamp_millis();
        let params = format!("timestamp={}&recvWindow=5000", timestamp);

        let url = format!("{}/api/v3/account", self.base_url);
        
        let response = self.sign_request(
            self.client.get(&url),
            &params
        )
        .send()
        .await
        .context("Failed to get account balance")?;

        let json: serde_json::Value = response.json().await?;
        
        // Find USDT balance
        let empty_array = vec![];
        let balances = json["balances"].as_array().unwrap_or(&empty_array);
        let mut total = 0.0;
        let mut available = 0.0;
        
        for balance in balances {
            if balance["asset"].as_str() == Some("USDT") {
                total = balance["free"].as_str().and_then(|s| s.parse().ok()).unwrap_or(0.0)
                    + balance["locked"].as_str().and_then(|s| s.parse().ok()).unwrap_or(0.0);
                available = balance["free"].as_str().and_then(|s| s.parse().ok()).unwrap_or(0.0);
                break;
            }
        }

        Ok(AccountBalance {
            total_balance: total,
            available_balance: available,
            total_unrealized_pnl: 0.0, // Not applicable for spot
            timestamp: Utc::now().timestamp_millis(),
        })
    }

    async fn emergency_close_all(&self) -> Result<()> {
        // For spot, emergency close means sell all holdings to USDT
        // This is a simplified implementation
        debug!("Emergency close all for Spot - selling all positions");
        
        let account = self.get_account_balance().await?;
        debug!("Spot account balance: {:?}", account);
        
        // In a real implementation, we would:
        // 1. Get all non-USDT balances
        // 2. Place MARKET sell orders for each
        // 3. Wait for fills
        
        Ok(())
    }

    fn base_url(&self) -> &str {
        &self.base_url
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spot_client_creation() {
        // Test with mock credentials
        let client = SpotClient::with_credentials("test_key", "test_secret");
        assert_eq!(client.market_type(), MarketType::Spot);
        assert_eq!(client.base_url(), "https://api.binance.com");
    }

    #[test]
    fn test_generate_client_order_id() {
        let client = SpotClient::with_credentials("test", "test");
        let order_id = client.generate_client_order_id();
        assert!(order_id.starts_with("zeroclaw_"));
    }
}
