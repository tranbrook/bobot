//! Binance Futures Client implementation.

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

/// Binance Futures API Client
pub struct FuturesClient {
    client: Client,
    api_key: String,
    secret_key: String,
    base_url: String,
}

impl FuturesClient {
    /// Create a new Futures Client from environment variables
    pub fn new() -> Result<Self> {
        let api_key = env::var("BINANCE_FUTURES_API_KEY")
            .context("BINANCE_FUTURES_API_KEY not set")?;
        let secret_key = env::var("BINANCE_FUTURES_SECRET_KEY")
            .context("BINANCE_FUTURES_SECRET_KEY not set")?;
        
        Ok(Self {
            client: Client::new(),
            api_key,
            secret_key,
            base_url: "https://fapi.binance.com".to_string(),
        })
    }

    /// Create with custom credentials
    pub fn with_credentials(api_key: &str, secret_key: &str) -> Self {
        Self {
            client: Client::new(),
            api_key: api_key.to_string(),
            secret_key: secret_key.to_string(),
            base_url: "https://fapi.binance.com".to_string(),
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
impl ExchangeClient for FuturesClient {
    fn market_type(&self) -> MarketType {
        MarketType::Futures
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

        // Post-Only (GTX) for Futures
        if order.post_only {
            params.push_str("&timeInForce=GTX");
            params.push_str("&newOrderRespType=RESULT");
        }

        params.push_str(&format!("&newClientOrderId={}", order.client_order_id));

        let url = format!("{}/fapi/v1/order", self.base_url);
        
        let response = self.sign_request(
            self.client.post(&url),
            &params
        )
        .send()
        .await
        .context("Failed to send order request")?;

        if !response.status().is_success() {
            let error = response.text().await?;
            return Err(anyhow::anyhow!("Binance Futures API error: {}", error));
        }

        let json: serde_json::Value = response.json().await?;
        
        debug!("Futures order response: {:?}", json);

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

        let url = format!("{}/fapi/v1/order", self.base_url);
        
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
        let timestamp = Utc::now().timestamp_millis();
        let params = format!("timestamp={}&recvWindow=5000", timestamp);

        let url = format!("{}/fapi/v2/positionRisk", self.base_url);
        
        let response = self.sign_request(
            self.client.get(&url),
            &params
        )
        .send()
        .await
        .context("Failed to get position risk")?;

        let json: serde_json::Value = response.json().await?;
        
        // Find the position for the symbol
        if let Some(positions) = json.as_array() {
            for pos in positions {
                if pos["symbol"].as_str() == Some(symbol) {
                    let position_amt: f64 = pos["positionAmt"].as_str().and_then(|s| s.parse().ok()).unwrap_or(0.0);
                    let entry_price = pos["entryPrice"].as_str().and_then(|s| s.parse().ok()).unwrap_or(0.0);
                    let mark_price = pos["markPrice"].as_str().and_then(|s| s.parse().ok()).unwrap_or(0.0);
                    let leverage = pos["leverage"].as_u64().unwrap_or(1) as u32;
                    
                    let unrealized_pnl = pos["unRealizedProfit"].as_str().and_then(|s| s.parse().ok()).unwrap_or(0.0);
                    
                    let side = if position_amt > 0.0 {
                        "LONG"
                    } else if position_amt < 0.0 {
                        "SHORT"
                    } else {
                        "NONE"
                    };

                    return Ok(PositionInfo {
                        symbol: symbol.to_string(),
                        side: side.to_string(),
                        quantity: position_amt.abs(),
                        entry_price,
                        mark_price,
                        unrealized_pnl,
                        unrealized_pnl_percent: if entry_price > 0.0 {
                            (mark_price - entry_price) / entry_price * 100.0 * if side == "LONG" { 1.0 } else { -1.0 }
                        } else {
                            0.0
                        },
                        leverage,
                        margin_type: pos["marginType"].as_str().unwrap_or("ISOLATED").to_string(),
                    });
                }
            }
        }

        Ok(PositionInfo::default())
    }

    async fn get_account_balance(&self) -> Result<AccountBalance> {
        let timestamp = Utc::now().timestamp_millis();
        let params = format!("timestamp={}&recvWindow=5000", timestamp);

        let url = format!("{}/fapi/v2/balance", self.base_url);
        
        let response = self.sign_request(
            self.client.get(&url),
            &params
        )
        .send()
        .await
        .context("Failed to get futures balance")?;

        let json: serde_json::Value = response.json().await?;
        
        // Find USDT balance
        if let Some(balances) = json.as_array() {
            for balance in balances {
                if balance["asset"].as_str() == Some("USDT") {
                    let total = balance["balance"].as_str().and_then(|s| s.parse().ok()).unwrap_or(0.0);
                    let available = balance["availableBalance"].as_str().and_then(|s| s.parse().ok()).unwrap_or(0.0);
                    
                    return Ok(AccountBalance {
                        total_balance: total,
                        available_balance: available,
                        total_unrealized_pnl: 0.0,
                        timestamp: Utc::now().timestamp_millis(),
                    });
                }
            }
        }

        Ok(AccountBalance::default())
    }

    async fn emergency_close_all(&self) -> Result<()> {
        debug!("Emergency close all for Futures - closing all positions");
        
        // Get all positions
        let timestamp = Utc::now().timestamp_millis();
        let params = format!("timestamp={}&recvWindow=5000", timestamp);
        let url = format!("{}/fapi/v2/positionRisk", self.base_url);
        
        let response = self.sign_request(
            self.client.get(&url),
            &params
        )
        .send()
        .await?;

        let json: serde_json::Value = response.json().await?;
        
        if let Some(positions) = json.as_array() {
            for pos in positions {
                let position_amt: f64 = pos["positionAmt"].as_str().and_then(|s| s.parse().ok()).unwrap_or(0.0);
                let symbol = pos["symbol"].as_str().unwrap_or("");
                
                if position_amt != 0.0 {
                    // Close position with MARKET order
                    let side = if position_amt > 0.0 { "SELL" } else { "BUY" };
                    let quantity = position_amt.abs();
                    
                    let order = OrderRequest {
                        symbol: symbol.to_string(),
                        side: side.to_string(),
                        order_type: "MARKET".to_string(),
                        quantity,
                        price: None,
                        time_in_force: None,
                        post_only: false,
                        client_order_id: self.generate_client_order_id(),
                    };
                    
                    let _ = self.place_order(&order).await;
                    debug!("Emergency closed position for {}", symbol);
                }
            }
        }
        
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
    fn test_futures_client_creation() {
        let client = FuturesClient::with_credentials("test_key", "test_secret");
        assert_eq!(client.market_type(), MarketType::Futures);
        assert_eq!(client.base_url(), "https://fapi.binance.com");
    }
}
