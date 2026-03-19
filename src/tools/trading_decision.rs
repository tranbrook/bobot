//! Trading Decision Tool - AI-powered trading analysis.
//!
//! This tool allows the ZeroClaw Agent to analyze market signals
//! and make trading decisions using external LLM providers.

use anyhow::{Context, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;
use tracing::{info, warn};

use crate::tools::{Tool, ToolResult};
use crate::agent::trading::{SignalReader, SafetyLayer, TradingMemory};
use crate::agent::trading::safety_layer::{TradingDecision, TradingSafetyConfig};

/// Trading Decision Tool configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradingDecisionToolConfig {
    /// Redis URL
    pub redis_url: String,
    /// Symbols to monitor
    pub symbols: Vec<String>,
    /// Check interval in milliseconds
    pub check_interval_ms: u64,
    /// Safety configuration
    pub safety: TradingSafetyConfig,
}

impl Default for TradingDecisionToolConfig {
    fn default() -> Self {
        Self {
            redis_url: "redis://localhost:6379".to_string(),
            symbols: vec!["BTCUSDT".to_string()],
            check_interval_ms: 5000,
            safety: TradingSafetyConfig::default(),
        }
    }
}

/// Trading Decision Tool for AI-powered trading analysis
pub struct TradingDecisionTool {
    config: TradingDecisionToolConfig,
    pub signal_reader: SignalReader,
    pub safety_layer: SafetyLayer,
    memory: Arc<TradingMemory>,
}

impl TradingDecisionTool {
    /// Create a new Trading Decision Tool
    pub fn new(config: TradingDecisionToolConfig) -> Result<Self> {
        // Create Redis client
        let redis = redis::Client::open(config.redis_url.as_str())
            .context("Failed to create Redis client")?;
        
        // Create trading memory (in-memory for now, can be configured for persistence)
        let memory = Arc::new(TradingMemory::new_in_memory()?);
        
        Ok(Self {
            signal_reader: SignalReader::new(redis.clone(), Some("market:signal")),
            safety_layer: SafetyLayer::new(redis.clone(), config.safety.clone()),
            memory,
            config,
        })
    }

    /// Create with custom database path
    pub fn with_db_path(config: TradingDecisionToolConfig, db_path: &std::path::Path) -> Result<Self> {
        // Create Redis client
        let redis = redis::Client::open(config.redis_url.as_str())
            .context("Failed to create Redis client")?;
        
        // Create trading memory with persistent storage
        let memory = Arc::new(TradingMemory::new(db_path)?);
        
        Ok(Self {
            signal_reader: SignalReader::new(redis.clone(), Some("market:signal")),
            safety_layer: SafetyLayer::new(redis.clone(), config.safety.clone()),
            memory,
            config,
        })
    }

    /// Analyze a symbol and return trading decision
    pub fn analyze_symbol(&self, symbol: &str) -> Result<Option<TradingDecision>> {
        // Read signal from Redis
        let signal = match self.signal_reader.read_signal(symbol)? {
            Some(signal) => signal,
            None => {
                info!("No signal found for {}", symbol);
                return Ok(None);
            }
        };

        // Pre-check: Validate signal
        if let Err(e) = self.safety_layer.pre_check(&signal) {
            warn!("Signal failed pre-check for {}: {}", symbol, e);
            return Ok(None);
        }

        // Convert signal to prompt format
        let signal_prompt = self.signal_reader.signal_to_prompt(&signal);
        
        // Build the full prompt for LLM
        let _prompt = format!(
            r#"You are a professional cryptocurrency trader with 10+ years of experience.
You specialize in technical analysis and risk management.

## Market Data for {symbol}

{signal_prompt}

## Your Task

Analyze this market data and make a trading decision.

## Rules

1. Never risk more than 2% per trade
2. Always use stop loss
3. Only trade when confluence score >= 7
4. Prefer trending markets (ADX > 25)
5. Avoid trading during high volatility unless regime is clearly trending

## Response Format

Return your decision in JSON format:
{{
  "decision": "EXECUTE" or "WAIT",
  "action": "LONG", "SHORT", or "NONE",
  "leverage": 1-10,
  "stop_loss": price_level,
  "take_profit": price_level,
  "risk_percent": 0.01-0.05,
  "reasoning": "Detailed explanation of your analysis"
}}"#,
            symbol = symbol,
            signal_prompt = signal_prompt,
        );

        info!("Generated trading prompt for {}", symbol);
        
        // Note: In real implementation, the Agent would call the LLM here
        // For now, we return the prompt so the Agent can use it
        // This is a simplified version - the actual decision would come from LLM
        
        Ok(None) // Return None for now - actual decision comes from LLM
    }

    /// Get the prompt for a symbol (for Agent to use with LLM)
    pub fn get_trading_prompt(&self, symbol: &str) -> Result<Option<String>> {
        let signal = match self.signal_reader.read_signal(symbol)? {
            Some(signal) => signal,
            None => return Ok(None),
        };

        // Pre-check
        if let Err(e) = self.safety_layer.pre_check(&signal) {
            return Ok(Some(format!(
                "Signal failed pre-check: {}. Recommendation: WAIT",
                e
            )));
        }

        let signal_prompt = self.signal_reader.signal_to_prompt(&signal);
        
        let prompt = format!(
            r#"You are a professional cryptocurrency trader.

## Market Data for {symbol}

{signal_prompt}

## Decision Required

Return JSON:
{{
  "decision": "EXECUTE" or "WAIT",
  "action": "LONG", "SHORT", or "NONE",
  "leverage": 1-10,
  "stop_loss": price,
  "take_profit": price,
  "risk_percent": 0.01-0.05,
  "reasoning": "explanation"
}}"#,
            symbol = symbol,
            signal_prompt = signal_prompt,
        );

        Ok(Some(prompt))
    }

    /// Validate and process LLM decision
    pub fn process_llm_decision(
        &self,
        symbol: &str,
        decision: &TradingDecision,
    ) -> Result<bool> {
        let signal = match self.signal_reader.read_signal(symbol)? {
            Some(signal) => signal,
            None => {
                warn!("No signal found for {} when processing decision", symbol);
                return Ok(false);
            }
        };

        // Post-check: Validate decision
        if let Err(e) = self.safety_layer.post_check(decision, &signal) {
            warn!("Decision failed post-check: {}", e);
            return Ok(false);
        }

        // Risk-check
        if let Err(e) = self.safety_layer.risk_check(decision) {
            warn!("Decision failed risk-check: {}", e);
            return Ok(false);
        }

        info!("Decision validated for {}: {:?}", symbol, decision);
        Ok(true)
    }
}

#[async_trait]
impl Tool for TradingDecisionTool {
    fn name(&self) -> &str {
        "trading_decision"
    }

    fn description(&self) -> &str {
        "Analyze market signals and make trading decisions using AI. \
         Use this tool to get trading recommendations based on technical analysis. \
         Input: {\"symbol\": \"BTCUSDT\", \"action\": \"analyze\"} \
         Output: Trading decision with entry, stop loss, and take profit levels."
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "symbol": {
                    "type": "string",
                    "description": "Trading pair symbol (e.g., BTCUSDT, ETHUSDT)"
                },
                "action": {
                    "type": "string",
                    "enum": ["analyze", "get_prompt", "process_decision"],
                    "description": "Type of trading operation"
                },
                "decision": {
                    "type": "object",
                    "description": "LLM decision object (required for process_decision action)",
                    "properties": {
                        "decision": { "type": "string" },
                        "action": { "type": "string" },
                        "leverage": { "type": "integer" },
                        "stop_loss": { "type": "number" },
                        "take_profit": { "type": "number" },
                        "risk_percent": { "type": "number" },
                        "reasoning": { "type": "string" }
                    }
                }
            },
            "required": ["symbol", "action"]
        })
    }

    async fn execute(&self, args: Value) -> Result<ToolResult> {
        let symbol = args["symbol"]
            .as_str()
            .context("Missing or invalid 'symbol' argument")?;
        
        let action = args["action"]
            .as_str()
            .context("Missing or invalid 'action' argument")?;

        match action {
            "analyze" => {
                // Note: This is a simplified version
                // In real implementation, we'd need mutable access to self
                Ok(ToolResult {
                    success: true,
                    output: format!(
                        "Analysis requested for {}. Use get_prompt to retrieve the actual prompt for LLM.",
                        symbol
                    ),
                    error: None,
                })
            }
            "get_prompt" => {
                Ok(ToolResult {
                    success: true,
                    output: format!(
                        "Prompt generation for {} - requires Agent LLM integration",
                        symbol
                    ),
                    error: None,
                })
            }
            "process_decision" => {
                Ok(ToolResult {
                    success: true,
                    output: format!(
                        "Decision processing for {} - requires Agent LLM integration",
                        symbol
                    ),
                    error: None,
                })
            }
            _ => Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(format!("Unknown action: {}", action)),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_name() {
        let config = TradingDecisionToolConfig::default();
        let tool = TradingDecisionTool::new(config).unwrap();
        assert_eq!(tool.name(), "trading_decision");
    }

    #[test]
    fn test_tool_description() {
        let config = TradingDecisionToolConfig::default();
        let tool = TradingDecisionTool::new(config).unwrap();
        assert!(!tool.description().is_empty());
        assert!(tool.description().contains("trading"));
    }

    #[test]
    fn test_parameters_schema() {
        let config = TradingDecisionToolConfig::default();
        let tool = TradingDecisionTool::new(config).unwrap();
        let schema = tool.parameters_schema();
        
        assert!(schema.is_object());
        assert!(schema["properties"]["symbol"].is_object());
        assert!(schema["properties"]["action"].is_object());
    }
}
