//! Backtesting module for ZeroClaw Trading.
//!
//! This module provides backtesting capabilities for trading strategies.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;
use crate::agent::trading::memory::{TradingMemory, TradingStats};
use crate::agent::trading::safety_layer::TradingDecision;
use zeroclaw_common::{SignalOutput, MarketRegime, Signal, IndicatorSnapshot};

/// Backtest configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BacktestConfig {
    /// Start timestamp (ms)
    pub start_timestamp_ms: i64,
    
    /// End timestamp (ms)
    pub end_timestamp_ms: i64,
    
    /// Symbols to backtest
    pub symbols: Vec<String>,
    
    /// Initial capital
    #[serde(default = "default_initial_capital")]
    pub initial_capital: f64,
    
    /// Risk per trade
    #[serde(default = "default_risk_per_trade")]
    pub risk_per_trade: f64,
    
    /// Maximum leverage
    #[serde(default = "default_max_leverage")]
    pub max_leverage: u32,
    
    /// Include trading fees
    #[serde(default = "default_true")]
    pub include_fees: bool,
    
    /// Fee rate (0.1% = 0.001)
    #[serde(default = "default_fee_rate")]
    pub fee_rate: f64,
}

fn default_initial_capital() -> f64 { 10000.0 }
fn default_risk_per_trade() -> f64 { 0.02 }
fn default_max_leverage() -> u32 { 10 }
fn default_true() -> bool { true }
fn default_fee_rate() -> f64 { 0.001 }

impl Default for BacktestConfig {
    fn default() -> Self {
        Self {
            start_timestamp_ms: 0,
            end_timestamp_ms: 0,
            symbols: vec!["BTCUSDT".to_string()],
            initial_capital: default_initial_capital(),
            risk_per_trade: default_risk_per_trade(),
            max_leverage: default_max_leverage(),
            include_fees: default_true(),
            fee_rate: default_fee_rate(),
        }
    }
}

/// Backtest result summary
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BacktestResult {
    /// Total trades executed
    pub total_trades: usize,
    
    /// Winning trades
    pub winning_trades: usize,
    
    /// Losing trades
    pub losing_trades: usize,
    
    /// Win rate
    pub win_rate: f64,
    
    /// Total PnL
    pub total_pnl: f64,
    
    /// Total PnL percentage
    pub total_pnl_percent: f64,
    
    /// Maximum drawdown
    pub max_drawdown: f64,
    
    /// Maximum drawdown percentage
    pub max_drawdown_percent: f64,
    
    /// Sharpe ratio
    pub sharpe_ratio: Option<f64>,
    
    /// Profit factor
    pub profit_factor: f64,
    
    /// Average win
    pub avg_win: f64,
    
    /// Average loss
    pub avg_loss: f64,
    
    /// Total fees paid
    pub total_fees: f64,
    
    /// Final capital
    pub final_capital: f64,
    
    /// Equity curve (snapshot at each trade)
    pub equity_curve: Vec<f64>,
}

/// Backtest engine
pub struct BacktestEngine {
    config: BacktestConfig,
    memory: TradingMemory,
}

impl BacktestEngine {
    /// Create a new backtest engine
    pub fn new(config: BacktestConfig) -> Result<Self> {
        let memory = TradingMemory::new_in_memory()
            .context("Failed to create in-memory database")?;
        
        Ok(Self { config, memory })
    }

    /// Run backtest on historical signals
    pub fn run(&self, signals: &[SignalOutput], decisions: &[TradingDecision]) -> Result<BacktestResult> {
        let mut result = BacktestResult::default();
        let mut capital = self.config.initial_capital;
        let mut peak_capital = capital;
        let mut equity_curve = Vec::new();
        
        for (signal, decision) in signals.iter().zip(decisions.iter()) {
            if !decision.should_execute() {
                equity_curve.push(capital);
                continue;
            }
            
            // Calculate position size
            let risk_amount = capital * self.config.risk_per_trade;
            let stop_loss_distance = (signal.price - decision.stop_loss).abs();
            
            if stop_loss_distance <= 0.0 {
                equity_curve.push(capital);
                continue;
            }
            
            let position_size = risk_amount / stop_loss_distance;
            let position_value = position_size * signal.price;
            
            // Check leverage
            let required_margin = position_value / decision.leverage as f64;
            if required_margin > capital {
                equity_curve.push(capital);
                continue;
            }
            
            // Simulate trade outcome (simplified - in reality would need exit data)
            let exit_price = if decision.action == "LONG" {
                signal.price + (decision.take_profit - signal.price) * 0.5 // Simplified
            } else {
                signal.price - (signal.price - decision.take_profit) * 0.5
            };
            
            // Calculate PnL
            let pnl = if decision.action == "LONG" {
                (exit_price - signal.price) * position_size
            } else {
                (signal.price - exit_price) * position_size
            };
            
            // Calculate fees
            let fees = if self.config.include_fees {
                position_value * self.config.fee_rate * 2.0 // Entry + exit
            } else {
                0.0
            };
            
            let net_pnl = pnl - fees;
            capital += net_pnl;
            result.total_fees += fees;
            
            // Update metrics
            result.total_trades += 1;
            if net_pnl > 0.0 {
                result.winning_trades += 1;
            } else {
                result.losing_trades += 1;
            }
            
            result.total_pnl += net_pnl;
            
            // Update peak and drawdown
            if capital > peak_capital {
                peak_capital = capital;
            }
            
            let drawdown = peak_capital - capital;
            let drawdown_percent = (drawdown / peak_capital) * 100.0;
            
            if drawdown > result.max_drawdown {
                result.max_drawdown = drawdown;
            }
            
            if drawdown_percent > result.max_drawdown_percent {
                result.max_drawdown_percent = drawdown_percent;
            }
            
            equity_curve.push(capital);
        }
        
        // Calculate final metrics
        result.final_capital = capital;
        result.total_pnl_percent = ((capital - self.config.initial_capital) / self.config.initial_capital) * 100.0;
        result.win_rate = if result.total_trades > 0 {
            result.winning_trades as f64 / result.total_trades as f64
        } else {
            0.0
        };
        
        result.equity_curve = equity_curve;
        
        Ok(result)
    }

    /// Load historical signals from file
    pub fn load_signals_from_file(path: &Path) -> Result<Vec<SignalOutput>> {
        let content = std::fs::read_to_string(path)
            .context("Failed to read signals file")?;
        
        let signals: Vec<SignalOutput> = serde_json::from_str(&content)
            .context("Failed to parse signals JSON")?;
        
        Ok(signals)
    }

    /// Export backtest results to JSON
    pub fn export_results(result: &BacktestResult, path: &Path) -> Result<()> {
        let json = serde_json::to_string_pretty(result)
            .context("Failed to serialize results")?;
        
        std::fs::write(path, json)
            .context("Failed to write results file")?;
        
        Ok(())
    }

    /// Generate backtest report
    pub fn generate_report(result: &BacktestResult) -> String {
        format!(
            r#"
╔══════════════════════════════════════════════════════════╗
║              ZeroClaw Backtest Report                    ║
╠══════════════════════════════════════════════════════════╣
║  Performance Summary                                     ║
╠══════════════════════════════════════════════════════════╣
║  Total Trades:      {:>10}                               ║
║  Winning Trades:    {:>10}                               ║
║  Losing Trades:     {:>10}                               ║
║  Win Rate:          {:>10.2}%                            ║
╠══════════════════════════════════════════════════════════╣
║  Financial Summary                                       ║
╠══════════════════════════════════════════════════════════╣
║  Initial Capital:   ${:>12.2}                            ║
║  Final Capital:     ${:>12.2}                            ║
║  Total PnL:         ${:>12.2}                            ║
║  Total PnL %:       {:>12.2}%                            ║
║  Total Fees:        ${:>12.2}                            ║
╠══════════════════════════════════════════════════════════╣
║  Risk Metrics                                            ║
╠══════════════════════════════════════════════════════════╣
║  Max Drawdown:      ${:>12.2}                            ║
║  Max Drawdown %:    {:>12.2}%                            ║
║  Profit Factor:     {:>12.2}                            ║
║  Average Win:       ${:>12.2}                            ║
║  Average Loss:      ${:>12.2}                            ║
╚══════════════════════════════════════════════════════════╝
"#,
            result.total_trades,
            result.winning_trades,
            result.losing_trades,
            result.win_rate * 100.0,
            result.final_capital - result.total_pnl,
            result.final_capital,
            result.total_pnl,
            result.total_pnl_percent,
            result.total_fees,
            result.max_drawdown,
            result.max_drawdown_percent,
            result.profit_factor,
            result.avg_win,
            result.avg_loss,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_signal() -> SignalOutput {
        SignalOutput {
            price: 50000.0,
            timestamp_ms: 1000,
            market_regime: MarketRegime::Trending,
            confluence_score: 7,
            indicator_snapshot: IndicatorSnapshot::default(),
            trade_advice: Signal::long(0.8),
            ingestion_latency_ms: 50,
            override_reason: None,
        }
    }

    fn create_test_decision() -> TradingDecision {
        TradingDecision {
            decision: "EXECUTE".to_string(),
            action: "LONG".to_string(),
            leverage: 5,
            stop_loss: 49000.0,
            take_profit: 52000.0,
            risk_percent: 0.02,
            reasoning: "Test".to_string(),
        }
    }

    #[test]
    fn test_backtest_engine_creation() {
        let config = BacktestConfig::default();
        let engine = BacktestEngine::new(config);
        assert!(engine.is_ok());
    }

    #[test]
    fn test_backtest_run() {
        let config = BacktestConfig::default();
        let engine = BacktestEngine::new(config).unwrap();
        
        let signals = vec![create_test_signal(); 10];
        let decisions = vec![create_test_decision(); 10];
        
        let result = engine.run(&signals, &decisions).unwrap();
        
        assert!(result.total_trades > 0);
        assert!(result.final_capital > 0.0);
    }

    #[test]
    fn test_backtest_report_generation() {
        let result = BacktestResult {
            total_trades: 100,
            winning_trades: 65,
            losing_trades: 35,
            win_rate: 0.65,
            total_pnl: 5000.0,
            total_pnl_percent: 50.0,
            max_drawdown: 500.0,
            max_drawdown_percent: 5.0,
            sharpe_ratio: Some(1.5),
            profit_factor: 2.0,
            avg_win: 100.0,
            avg_loss: 50.0,
            total_fees: 100.0,
            final_capital: 15000.0,
            equity_curve: vec![],
        };
        
        let report = BacktestEngine::generate_report(&result);
        assert!(report.contains("Backtest Report"));
        assert!(report.contains("100")); // Total trades
        assert!(report.contains("65.00%")); // Win rate
    }
}
