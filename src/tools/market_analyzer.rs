//! Market Analyzer Tool - reads trading signals from Redis.
//!
//! This tool allows ZeroClaw to fetch market signals from Muscle-Kit
//! via Redis and present them in a human-readable format.

use super::traits::{Tool, ToolResult};
use anyhow::{Context, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use zeroclaw_common::SignalOutput;

/// Market Analyzer Tool for fetching signals from Redis
pub struct MarketAnalyzerTool {
    redis_url: String,
    max_latency_ms: i64,
}

#[derive(Debug, Serialize, Deserialize)]
struct MarketAnalyzerArgs {
    symbol: String,
}

impl MarketAnalyzerTool {
    /// Create a new Market Analyzer Tool
    pub fn new(redis_url: &str) -> Self {
        Self {
            redis_url: redis_url.to_string(),
            max_latency_ms: 500,
        }
    }

    /// Create with custom latency threshold
    pub fn with_latency(redis_url: &str, max_latency_ms: i64) -> Self {
        Self {
            redis_url: redis_url.to_string(),
            max_latency_ms,
        }
    }

    /// Fetch signal from Redis and format for LLM
    async fn fetch_signal(&self, symbol: &str) -> Result<String> {
        let client = redis::Client::open(self.redis_url.as_str())
            .context("Failed to create Redis client")?;

        let mut conn = client
            .get_async_connection()
            .await
            .context("Failed to connect to Redis")?;

        let key = format!("market:signal:{}", symbol.to_uppercase());
        
        let json: Option<String> = redis::cmd("GET")
            .arg(&key)
            .query_async(&mut conn)
            .await
            .context("Failed to read signal from Redis")?;

        match json {
            Some(json_str) => {
                let signal: SignalOutput = serde_json::from_str(&json_str)
                    .context("Failed to deserialize SignalOutput")?;

                // Format human-readable summary
                let mut summary = String::new();
                summary.push_str(&format!("📊 Market Signal for {}\n\n", symbol.to_uppercase()));
                summary.push_str(&format!("Price: ${:.2}\n", signal.price));
                summary.push_str(&format!("Market Regime: {}\n", signal.market_regime));
                summary.push_str(&format!("Confluence Score: {}/10\n", signal.confluence_score));
                summary.push_str(&format!("Muscle Advice: {}\n", signal.action()));
                summary.push_str(&format!("\nIndicators:\n"));
                summary.push_str(&format!("  RSI: {:.1}\n", signal.indicator_snapshot.rsi));
                summary.push_str(&format!("  BB Width: {:.2}%\n", signal.indicator_snapshot.bb_width * 100.0));
                summary.push_str(&format!("  US Value: {:.2}\n", signal.indicator_snapshot.us_value));
                summary.push_str(&format!("  MACD: {:.2}\n", signal.indicator_snapshot.macd));
                summary.push_str(&format!("  ADX: {:.1}\n", signal.indicator_snapshot.adx));

                // Safety check
                if signal.ingestion_latency_ms > self.max_latency_ms {
                    summary.push_str(&format!(
                        "\n⚠️ DATA STALE: HIGH LATENCY DETECTED ({}ms > {}ms)",
                        signal.ingestion_latency_ms, self.max_latency_ms
                    ));
                } else {
                    summary.push_str(&format!(
                        "\nLatency: {}ms (OK)",
                        signal.ingestion_latency_ms
                    ));
                }

                if let Some(reason) = &signal.override_reason {
                    summary.push_str(&format!("\nOverride: {}", reason));
                }

                Ok(summary)
            }
            None => Ok(format!("No signal found for {}", symbol.to_uppercase())),
        }
    }
}

#[async_trait]
impl Tool for MarketAnalyzerTool {
    fn name(&self) -> &str {
        "get_market_signal"
    }

    fn description(&self) -> &str {
        "Fetch current market signal from Muscle-Kit via Redis. Returns price, market regime, confluence score, and Muscle's trading advice. Use this to analyze market conditions before making trading decisions."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "symbol": {
                    "type": "string",
                    "description": "Trading pair symbol (e.g., 'BTCUSDT', 'ETHUSDT')"
                }
            },
            "required": ["symbol"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> Result<ToolResult> {
        let args: MarketAnalyzerArgs = serde_json::from_value(args)
            .context("Failed to parse arguments")?;

        match self.fetch_signal(&args.symbol).await {
            Ok(summary) => Ok(ToolResult {
                success: true,
                output: summary,
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
        let tool = MarketAnalyzerTool::new("redis://localhost:6379");
        
        assert_eq!(tool.name(), "get_market_signal");
        assert!(tool.description().contains("Muscle-Kit"));
        assert!(tool.description().contains("Redis"));
        
        let schema = tool.parameters_schema();
        assert!(schema["properties"]["symbol"].is_object());
        assert!(schema["required"].as_array().unwrap().contains(&serde_json::json!("symbol")));
    }
}
