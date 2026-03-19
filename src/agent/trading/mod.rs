//! Trading module for ZeroClaw Agent.
//!
//! This module provides trading decision capabilities using the ZeroClaw Agent
//! with external LLM providers (Claude, GPT-4, Ollama) instead of local brain.
//!
//! ## Architecture
//!
//! ```text
//! Muscle-Kit → Redis (market:signal) → SignalReader → Agent → LLM → TradingDecisionTool → ExecutionBridge
//! ```
//!
//! ## Modules
//!
//! - `signal_reader` - Reads trading signals from Redis
//! - `safety_layer` - Multi-layer safety validation
//! - `memory` - Trading history storage and recall
//! - `monitor` - PnL tracking and alerting
//! - `backtest` - Backtesting engine for strategy validation

pub mod signal_reader;
pub mod safety_layer;
pub mod memory;
pub mod monitor;
pub mod backtest;

pub use signal_reader::SignalReader;
pub use safety_layer::{SafetyLayer, SafetyError};
pub use memory::TradingMemory;
pub use monitor::TradingMonitor;
pub use backtest::{BacktestEngine, BacktestConfig, BacktestResult};

use crate::config::Config;
use crate::tools::execution_bridge::ExecutionBridgeTool;
use crate::tools::trading_decision::TradingDecisionTool;
use anyhow::Result;
use redis::Client;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{error, info, warn};

/// Run the trading loop - checks signals and dispatches commands
pub async fn run_trading_loop(config: Config) -> Result<()> {
    info!(
        "📈 Trading loop started: symbols={:?}, interval={}ms",
        config.trading.symbols, config.trading.check_interval_ms
    );

    let redis_url = config.trading.redis_url.clone();
    let symbols = config.trading.symbols.clone();
    let check_interval = Duration::from_millis(config.trading.check_interval_ms);
    let min_confluence = config.trading.safety.min_confluence_score;

    // Create Redis client
    let redis_client = Client::open(redis_url.as_str())?;

    // Create trading decision tool
    let trading_config = crate::tools::trading_decision::TradingDecisionToolConfig {
        redis_url: redis_url.clone(),
        symbols: symbols.clone(),
        check_interval_ms: config.trading.check_interval_ms,
        safety: config.trading.safety.clone(),
    };

    let trading_tool = match TradingDecisionTool::new(trading_config) {
        Ok(tool) => tool,
        Err(e) => {
            error!("Failed to create TradingDecisionTool: {}", e);
            return Err(e);
        }
    };

    // Create execution bridge tool
    let execution_tool = ExecutionBridgeTool::new(&redis_url);

    info!("✅ Trading tools initialized");

    // Main trading loop
    loop {
        for symbol in &symbols {
            // Read signal from Redis
            match trading_tool.signal_reader.read_signal(symbol) {
                Ok(Some(signal)) => {
                    // Check confluence score
                    if signal.confluence_score.abs() < min_confluence {
                        continue; // Skip low confluence signals
                    }

                    info!(
                        "📊 Signal for {}: confluence={}, advice={:?}",
                        symbol, signal.confluence_score, signal.trade_advice
                    );

                    // Pre-check signal
                    match trading_tool.safety_layer.pre_check(&signal) {
                        Ok(_) => {
                            // Signal passed pre-check
                            // In full implementation, this would call the LLM for decision
                            // For now, we use the muscle-kit advice directly

                            // Convert signal to execution command
                            let (side, quantity) = match signal.trade_advice {
                                zeroclaw_common::Signal::Long { .. } => ("BUY", 0.001), // Example quantity
                                zeroclaw_common::Signal::Short { .. } => ("SELL", 0.001),
                                zeroclaw_common::Signal::Wait => continue,
                            };

                            // Dispatch to execution queue
                            let command = zeroclaw_common::ExecutionCommand {
                                symbol: symbol.clone(),
                                side: side.to_string(),
                                order_type: "MARKET".to_string(),
                                quantity,
                                stop_loss: None,
                                take_profit: None,
                                timestamp_ms: chrono::Utc::now().timestamp_millis(),
                                market_type: "SPOT".to_string(),
                                price: None,
                                post_only: false,
                            };

                            if let Err(e) = execution_tool.dispatch_command(&command).await {
                                error!("Failed to dispatch command for {}: {}", symbol, e);
                            } else {
                                info!(
                                    "✅ Command dispatched for {}: {} {}",
                                    symbol, side, quantity
                                );
                            }
                        }
                        Err(e) => {
                            warn!("Signal failed safety check for {}: {}", symbol, e);
                        }
                    }
                }
                Ok(None) => {
                    // No signal yet
                }
                Err(e) => {
                    error!("Failed to read signal for {}: {}", symbol, e);
                }
            }
        }

        sleep(check_interval).await;
    }
}
