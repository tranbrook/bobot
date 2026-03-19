//! Execution Bridge Tool - dispatches trading commands to Redis.
//!
//! This tool allows ZeroClaw to send execution commands to the
//! Execution Engine via Redis queue.

use super::traits::{Tool, ToolResult};
use anyhow::{Context, Result};
use async_trait::async_trait;
use redis::Client;
use serde::{Deserialize, Serialize};
use zeroclaw_common::ExecutionCommand;

/// Execution Bridge Tool for dispatching commands to Redis
pub struct ExecutionBridgeTool {
    redis_url: String,
    key_prefix: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct ExecutionBridgeArgs {
    symbol: String,
    side: String,  // "BUY" or "SELL"
    leverage: u32,
    stop_loss: f64,
    take_profit: f64,
    risk_percent: f64,
    #[serde(default = "default_order_type")]
    order_type: String,
}

fn default_order_type() -> String {
    "MARKET".to_string()
}

impl ExecutionBridgeTool {
    /// Create a new Execution Bridge Tool
    pub fn new(redis_url: &str) -> Self {
        Self {
            redis_url: redis_url.to_string(),
            key_prefix: "execution:queue".to_string(),
        }
    }

    /// Create with custom key prefix
    pub fn with_prefix(redis_url: &str, key_prefix: &str) -> Self {
        Self {
            redis_url: redis_url.to_string(),
            key_prefix: key_prefix.to_string(),
        }
    }

    /// Dispatch execution command to Redis queue
    pub async fn dispatch_command(&self, command: &ExecutionCommand) -> Result<()> {
        let client = Client::open(self.redis_url.as_str())
            .context("Failed to create Redis client")?;

        let mut conn = client
            .get_async_connection()
            .await
            .context("Failed to connect to Redis")?;

        let key = format!("{}:{}", self.key_prefix, command.symbol);
        let json = command.to_json().context("Failed to serialize command")?;

        // LPUSH to queue
        redis::cmd("LPUSH")
            .arg(&key)
            .arg(&json)
            .query_async::<()>(&mut conn)
            .await
            .context("Failed to push command to Redis queue")?;

        Ok(())
    }

    /// Dispatch execution command to Redis (legacy method)
    async fn dispatch(&self, args: &ExecutionBridgeArgs) -> Result<String> {
        let client = Client::open(self.redis_url.as_str())
            .context("Failed to create Redis client")?;

        let mut conn = client
            .get_async_connection()
            .await
            .context("Failed to connect to Redis")?;

        // Create execution command
        let command = ExecutionCommand {
            symbol: args.symbol.to_uppercase(),
            side: args.side.to_uppercase(),
            order_type: args.order_type.clone(),
            quantity: 0.0, // Will be calculated by execution engine based on risk_percent
            stop_loss: Some(args.stop_loss),
            take_profit: Some(args.take_profit),
            timestamp_ms: chrono::Utc::now().timestamp_millis(),
            market_type: "FUTURES".to_string(),
            price: None,
            post_only: false,
        };

        let key = format!("{}:{}", self.key_prefix, command.symbol);
        let json = command.to_json().context("Failed to serialize command")?;

        // LPUSH to queue
        redis::cmd("LPUSH")
            .arg(&key)
            .arg(&json)
            .query_async::<()>(&mut conn)
            .await
            .context("Failed to push command to Redis queue")?;

        let timestamp = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC").to_string();
        
        Ok(format!(
            "✅ Command dispatched to Execution Engine for {} at {}\n\
             Side: {} | Leverage: {}x | SL: ${:.2} | TP: ${:.2} | Risk: {:.2}%",
            command.symbol,
            timestamp,
            command.side,
            args.leverage,
            args.stop_loss,
            args.take_profit,
            args.risk_percent * 100.0
        ))
    }
}

#[async_trait]
impl Tool for ExecutionBridgeTool {
    fn name(&self) -> &str {
        "dispatch_execution"
    }

    fn description(&self) -> &str {
        "Dispatch a trading execution command to the Execution Engine via Redis. Use this after analyzing market signals and deciding to execute a trade. Requires symbol, side (BUY/SELL), leverage, stop_loss, take_profit, and risk_percent."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "symbol": {
                    "type": "string",
                    "description": "Trading pair symbol (e.g., 'BTCUSDT')"
                },
                "side": {
                    "type": "string",
                    "enum": ["BUY", "SELL"],
                    "description": "Trade direction: BUY for Long, SELL for Short"
                },
                "leverage": {
                    "type": "integer",
                    "description": "Leverage multiplier (1-10)",
                    "minimum": 1,
                    "maximum": 10
                },
                "stop_loss": {
                    "type": "number",
                    "description": "Stop loss price level"
                },
                "take_profit": {
                    "type": "number",
                    "description": "Take profit price level"
                },
                "risk_percent": {
                    "type": "number",
                    "description": "Risk percentage of account (0.01 = 1%, max 0.05 = 5%)",
                    "minimum": 0.01,
                    "maximum": 0.05
                },
                "order_type": {
                    "type": "string",
                    "enum": ["MARKET", "LIMIT"],
                    "description": "Order type (default: MARKET)",
                    "default": "MARKET"
                }
            },
            "required": ["symbol", "side", "leverage", "stop_loss", "take_profit", "risk_percent"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> Result<ToolResult> {
        let args: ExecutionBridgeArgs = serde_json::from_value(args)
            .context("Failed to parse arguments")?;

        // Validate inputs
        if args.leverage < 1 || args.leverage > 10 {
            return Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some("Leverage must be between 1 and 10".to_string()),
            });
        }

        if args.risk_percent < 0.01 || args.risk_percent > 0.05 {
            return Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some("Risk percent must be between 0.01 (1%) and 0.05 (5%)".to_string()),
            });
        }

        if args.side.to_uppercase() != "BUY" && args.side.to_uppercase() != "SELL" {
            return Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some("Side must be either 'BUY' or 'SELL'".to_string()),
            });
        }

        match self.dispatch(&args).await {
            Ok(message) => Ok(ToolResult {
                success: true,
                output: message,
                error: None,
            }),
            Err(e) => Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(e.to_string()),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_metadata() {
        let tool = ExecutionBridgeTool::new("redis://localhost:6379");
        
        assert_eq!(tool.name(), "dispatch_execution");
        assert!(tool.description().contains("Execution Engine"));
        assert!(tool.description().contains("Redis"));
        
        let schema = tool.parameters_schema();
        assert!(schema["properties"]["symbol"].is_object());
        assert!(schema["required"].as_array().unwrap().contains(&serde_json::json!("symbol")));
    }

    #[test]
    fn test_parameter_validation() {
        let tool = ExecutionBridgeTool::new("redis://localhost:6379");
        
        // Test leverage validation
        let invalid_leverage = serde_json::json!({
            "symbol": "BTCUSDT",
            "side": "BUY",
            "leverage": 15,
            "stop_loss": 49000.0,
            "take_profit": 52000.0,
            "risk_percent": 0.02
        });
        
        // Schema should reject leverage > 10
        let schema = tool.parameters_schema();
        assert!(schema["properties"]["leverage"]["maximum"] == 10);
    }
}
