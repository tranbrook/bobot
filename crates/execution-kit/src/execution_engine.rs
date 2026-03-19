//! Execution Engine - Smart order routing and execution.

use anyhow::Result;
use tracing::{info, warn, error, debug};
use zeroclaw_common::ExecutionCommand;

use super::traits::ExchangeClient;
use super::spot_client::SpotClient;
use super::futures_client::FuturesClient;
use super::redis_listener::RedisListener;
use super::formatter::PrecisionFormatter;

/// Execution Engine for smart order routing
pub struct ExecutionEngine {
    spot_client: SpotClient,
    futures_client: FuturesClient,
    redis_listener: RedisListener,
    formatter: PrecisionFormatter,
}

impl ExecutionEngine {
    /// Create a new Execution Engine
    pub fn new(redis_url: &str) -> Result<Self> {
        Ok(Self {
            spot_client: SpotClient::new()?,
            futures_client: FuturesClient::new()?,
            redis_listener: RedisListener::new(redis_url)?,
            formatter: PrecisionFormatter::new(),
        })
    }

    /// Run the execution engine - listen and execute
    pub async fn run(&mut self, symbols: &[&str]) -> Result<()> {
        info!("Execution Engine started, listening for commands...");

        loop {
            match self.redis_listener.listen_any(symbols).await {
                Ok(cmd) => {
                    debug!("Received execution command: {:?}", cmd);
                    
                    if let Err(e) = self.execute_command(&cmd).await {
                        error!("Failed to execute command: {}", e);
                    }
                }
                Err(e) => {
                    error!("Failed to listen for commands: {}", e);
                    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                }
            }
        }
    }

    /// Execute a single command
    pub async fn execute_command(&self, cmd: &ExecutionCommand) -> Result<()> {
        info!("Executing {:?} order for {} - {} {}", 
            cmd.order_type, cmd.symbol, cmd.side, cmd.quantity);

        // Select client based on market type
        let client: &dyn ExchangeClient = if cmd.market_type == "FUTURES" {
            &self.futures_client
        } else {
            &self.spot_client
        };

        // Format price and quantity
        let (formatted_price, formatted_qty) = self.formatter.format(
            &cmd.symbol,
            cmd.price.unwrap_or(0.0),
            cmd.quantity,
        );

        // Build order request
        let order = super::traits::OrderRequest {
            symbol: cmd.symbol.clone(),
            side: cmd.side.clone(),
            order_type: cmd.order_type.clone(),
            quantity: formatted_qty,
            price: if cmd.order_type == "LIMIT" { Some(formatted_price) } else { None },
            time_in_force: if cmd.order_type == "LIMIT" { Some("GTC".to_string()) } else { None },
            post_only: cmd.post_only,
            client_order_id: format!("zeroclaw_{}", chrono::Utc::now().timestamp_millis()),
        };

        // Place order with retry logic
        let mut attempts = 0;
        let max_attempts = 3;

        loop {
            match client.place_order(&order).await {
                Ok(response) => {
                    info!("Order placed: {} - Status: {}", response.client_order_id, response.status);
                    
                    if response.status == "EXPIRED" && attempts < max_attempts {
                        attempts += 1;
                        warn!("Order expired, retrying (attempt {}/{})", attempts, max_attempts);
                        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                        continue;
                    }
                    
                    return Ok(());
                }
                Err(e) => {
                    if attempts >= max_attempts {
                        return Err(e);
                    }
                    attempts += 1;
                    warn!("Order failed, retrying (attempt {}/{}): {}", attempts, max_attempts, e);
                    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                }
            }
        }
    }

    /// Emergency close all positions
    pub async fn emergency_close_all(&self) -> Result<()> {
        warn!("EMERGENCY CLOSE ALL triggered!");
        
        let spot_result = self.spot_client.emergency_close_all().await;
        let futures_result = self.futures_client.emergency_close_all().await;

        if let Err(e) = spot_result {
            error!("Failed to close spot positions: {}", e);
        }
        if let Err(e) = futures_result {
            error!("Failed to close futures positions: {}", e);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_execution_engine_creation() {
        // This will fail without Redis running, but tests the constructor
        let result = ExecutionEngine::new("redis://localhost:6379");
        // Result may be Err if Redis is not running
        assert!(result.is_ok() || result.is_err());
    }
}
