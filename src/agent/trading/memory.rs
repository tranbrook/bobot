//! Trading Memory - Storage and recall for trading history.
//!
//! This module stores trading decisions and results for context and learning.
//! Uses SQLite backend for persistent storage with async-safe access via tokio::Mutex.

use anyhow::{Context, Result};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;
use zeroclaw_common::SignalOutput;
use crate::agent::trading::safety_layer::TradingDecision;

/// Trading context for LLM recall
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradingContext {
    pub id: i64,
    pub symbol: String,
    pub timestamp_ms: i64,
    pub signal_price: f64,
    pub market_regime: String,
    pub confluence_score: i8,
    pub decision: String,
    pub action: String,
    pub leverage: u32,
    pub stop_loss: f64,
    pub take_profit: f64,
    pub risk_percent: f64,
    pub reasoning: String,
    pub result: Option<TradeResult>,
}

/// Trade result information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeResult {
    pub id: i64,
    pub trade_id: i64,
    pub pnl: f64,
    pub pnl_percent: f64,
    pub exit_price: f64,
    pub exit_timestamp_ms: i64,
    pub duration_ms: i64,
}

/// Trading statistics
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TradingStats {
    pub total_trades: usize,
    pub winning_trades: usize,
    pub losing_trades: usize,
    pub total_pnl: f64,
    pub win_rate: f64,
    pub avg_pnl: f64,
    pub avg_pnl_percent: f64,
    pub avg_win: f64,
    pub avg_loss: f64,
    pub profit_factor: f64,
    pub max_drawdown: f64,
    pub avg_trade_duration_ms: i64,
}

/// Trading Memory for storing and recalling trading history
pub struct TradingMemory {
    conn: Arc<Mutex<Connection>>,
}

impl TradingMemory {
    /// Create a new Trading Memory with SQLite backend
    pub fn new(db_path: &Path) -> Result<Self> {
        let conn = Connection::open(db_path)
            .context("Failed to open SQLite database")?;
        
        let memory = Self { 
            conn: Arc::new(Mutex::new(conn)),
        };
        memory.create_tables()?;
        
        Ok(memory)
    }

    /// Create a new in-memory Trading Memory (for testing)
    pub fn new_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()
            .context("Failed to create in-memory database")?;
        
        let memory = Self { 
            conn: Arc::new(Mutex::new(conn)),
        };
        memory.create_tables()?;
        
        Ok(memory)
    }

    /// Create database tables (synchronous, called during initialization)
    fn create_tables(&self) -> Result<()> {
        let conn = self.conn.try_lock().context("Failed to acquire lock during init")?;
        
        conn.execute(
            "CREATE TABLE IF NOT EXISTS trades (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                symbol TEXT NOT NULL,
                timestamp_ms INTEGER NOT NULL,
                signal_price REAL NOT NULL,
                market_regime TEXT NOT NULL,
                confluence_score INTEGER NOT NULL,
                decision TEXT NOT NULL,
                action TEXT NOT NULL,
                leverage INTEGER NOT NULL,
                stop_loss REAL NOT NULL,
                take_profit REAL NOT NULL,
                risk_percent REAL NOT NULL,
                reasoning TEXT NOT NULL,
                created_at TEXT DEFAULT CURRENT_TIMESTAMP
            )",
            [],
        )?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS trade_results (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                trade_id INTEGER NOT NULL,
                pnl REAL NOT NULL,
                pnl_percent REAL NOT NULL,
                exit_price REAL NOT NULL,
                exit_timestamp_ms INTEGER NOT NULL,
                duration_ms INTEGER NOT NULL,
                created_at TEXT DEFAULT CURRENT_TIMESTAMP,
                FOREIGN KEY (trade_id) REFERENCES trades(id)
            )",
            [],
        )?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_trades_symbol ON trades(symbol)",
            [],
        )?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_trades_timestamp ON trades(timestamp_ms)",
            [],
        )?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_trade_results_trade_id ON trade_results(trade_id)",
            [],
        )?;

        Ok(())
    }

    /// Store a trading decision with context (async)
    pub async fn store_decision_async(
        &self,
        symbol: &str,
        signal: &SignalOutput,
        decision: &TradingDecision,
    ) -> Result<i64> {
        let conn = self.conn.lock().await;
        
        let mut stmt = conn.prepare(
            "INSERT INTO trades (
                symbol, timestamp_ms, signal_price, market_regime, confluence_score,
                decision, action, leverage, stop_loss, take_profit, risk_percent, reasoning
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        )?;

        let id = stmt.insert(params![
            symbol,
            signal.timestamp_ms,
            signal.price,
            signal.market_regime.to_string(),
            signal.confluence_score,
            decision.decision,
            decision.action,
            decision.leverage,
            decision.stop_loss,
            decision.take_profit,
            decision.risk_percent,
            decision.reasoning,
        ])?;

        Ok(id)
    }

    /// Store a trading decision (sync wrapper for non-async contexts)
    pub fn store_decision(
        &self,
        symbol: &str,
        signal: &SignalOutput,
        decision: &TradingDecision,
    ) -> Result<i64> {
        let conn = self.conn.try_lock().context("Failed to acquire lock")?;
        
        let mut stmt = conn.prepare(
            "INSERT INTO trades (
                symbol, timestamp_ms, signal_price, market_regime, confluence_score,
                decision, action, leverage, stop_loss, take_profit, risk_percent, reasoning
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        )?;

        let id = stmt.insert(params![
            symbol,
            signal.timestamp_ms,
            signal.price,
            signal.market_regime.to_string(),
            signal.confluence_score,
            decision.decision,
            decision.action,
            decision.leverage,
            decision.stop_loss,
            decision.take_profit,
            decision.risk_percent,
            decision.reasoning,
        ])?;

        Ok(id)
    }

    /// Record a trade result (async)
    pub async fn record_trade_result_async(
        &self,
        trade_id: i64,
        pnl: f64,
        pnl_percent: f64,
        exit_price: f64,
        exit_timestamp_ms: i64,
    ) -> Result<i64> {
        let conn = self.conn.lock().await;
        
        // Get entry timestamp
        let entry_timestamp_ms: i64 = conn.query_row(
            "SELECT timestamp_ms FROM trades WHERE id = ?1",
            [trade_id],
            |row| row.get(0),
        )?;

        let duration_ms = exit_timestamp_ms - entry_timestamp_ms;

        let mut stmt = conn.prepare(
            "INSERT INTO trade_results (
                trade_id, pnl, pnl_percent, exit_price, exit_timestamp_ms, duration_ms
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )?;

        let id = stmt.insert(params![
            trade_id,
            pnl,
            pnl_percent,
            exit_price,
            exit_timestamp_ms,
            duration_ms,
        ])?;

        Ok(id)
    }

    /// Record a trade result (sync wrapper)
    pub fn record_trade_result(
        &self,
        trade_id: i64,
        pnl: f64,
        pnl_percent: f64,
        exit_price: f64,
        exit_timestamp_ms: i64,
    ) -> Result<i64> {
        let conn = self.conn.try_lock().context("Failed to acquire lock")?;
        
        let entry_timestamp_ms: i64 = conn.query_row(
            "SELECT timestamp_ms FROM trades WHERE id = ?1",
            [trade_id],
            |row| row.get(0),
        )?;

        let duration_ms = exit_timestamp_ms - entry_timestamp_ms;

        let mut stmt = conn.prepare(
            "INSERT INTO trade_results (
                trade_id, pnl, pnl_percent, exit_price, exit_timestamp_ms, duration_ms
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )?;

        let id = stmt.insert(params![
            trade_id,
            pnl,
            pnl_percent,
            exit_price,
            exit_timestamp_ms,
            duration_ms,
        ])?;

        Ok(id)
    }

    /// Recall recent trading history for a symbol (async)
    pub async fn recall_history_async(&self, symbol: &str, limit: usize) -> Result<Vec<TradingContext>> {
        let conn = self.conn.lock().await;
        
        let mut stmt = conn.prepare(
            "SELECT 
                t.id, t.symbol, t.timestamp_ms, t.signal_price, t.market_regime,
                t.confluence_score, t.decision, t.action, t.leverage,
                t.stop_loss, t.take_profit, t.risk_percent, t.reasoning,
                r.id as result_id, r.pnl, r.pnl_percent, r.exit_price,
                r.exit_timestamp_ms, r.duration_ms
            FROM trades t
            LEFT JOIN trade_results r ON t.id = r.trade_id
            WHERE t.symbol = ?1
            ORDER BY t.timestamp_ms DESC
            LIMIT ?2",
        )?;

        let rows = stmt.query_map(params![symbol, limit], |row| {
            let trade_id: i64 = row.get(0)?;
            let result_id: Option<i64> = row.get(14)?;
            let result = if result_id.is_some() {
                Some(TradeResult {
                    id: row.get(14)?,
                    trade_id,
                    pnl: row.get(15)?,
                    pnl_percent: row.get(16)?,
                    exit_price: row.get(17)?,
                    exit_timestamp_ms: row.get(18)?,
                    duration_ms: row.get(19)?,
                })
            } else {
                None
            };

            Ok(TradingContext {
                id: trade_id,
                symbol: row.get(1)?,
                timestamp_ms: row.get(2)?,
                signal_price: row.get(3)?,
                market_regime: row.get(4)?,
                confluence_score: row.get(5)?,
                decision: row.get(6)?,
                action: row.get(7)?,
                leverage: row.get(8)?,
                stop_loss: row.get(9)?,
                take_profit: row.get(10)?,
                risk_percent: row.get(11)?,
                reasoning: row.get(12)?,
                result,
            })
        })?;

        let mut contexts = Vec::new();
        for row in rows {
            contexts.push(row?);
        }

        Ok(contexts)
    }

    /// Recall recent trading history (sync wrapper)
    pub fn recall_history(&self, symbol: &str, limit: usize) -> Result<Vec<TradingContext>> {
        let conn = self.conn.try_lock().context("Failed to acquire lock")?;
        
        let mut stmt = conn.prepare(
            "SELECT 
                t.id, t.symbol, t.timestamp_ms, t.signal_price, t.market_regime,
                t.confluence_score, t.decision, t.action, t.leverage,
                t.stop_loss, t.take_profit, t.risk_percent, t.reasoning,
                r.id as result_id, r.pnl, r.pnl_percent, r.exit_price,
                r.exit_timestamp_ms, r.duration_ms
            FROM trades t
            LEFT JOIN trade_results r ON t.id = r.trade_id
            WHERE t.symbol = ?1
            ORDER BY t.timestamp_ms DESC
            LIMIT ?2",
        )?;

        let rows = stmt.query_map(params![symbol, limit], |row| {
            let trade_id: i64 = row.get(0)?;
            let result_id: Option<i64> = row.get(14)?;
            let result = if result_id.is_some() {
                Some(TradeResult {
                    id: row.get(14)?,
                    trade_id,
                    pnl: row.get(15)?,
                    pnl_percent: row.get(16)?,
                    exit_price: row.get(17)?,
                    exit_timestamp_ms: row.get(18)?,
                    duration_ms: row.get(19)?,
                })
            } else {
                None
            };

            Ok(TradingContext {
                id: trade_id,
                symbol: row.get(1)?,
                timestamp_ms: row.get(2)?,
                signal_price: row.get(3)?,
                market_regime: row.get(4)?,
                confluence_score: row.get(5)?,
                decision: row.get(6)?,
                action: row.get(7)?,
                leverage: row.get(8)?,
                stop_loss: row.get(9)?,
                take_profit: row.get(10)?,
                risk_percent: row.get(11)?,
                reasoning: row.get(12)?,
                result,
            })
        })?;

        let mut contexts = Vec::new();
        for row in rows {
            contexts.push(row?);
        }

        Ok(contexts)
    }

    /// Get trading statistics (async)
    pub async fn get_stats_async(&self) -> Result<TradingStats> {
        let conn = self.conn.lock().await;
        self.get_stats_internal(&conn)
    }

    /// Get trading statistics (sync wrapper)
    pub fn get_stats(&self) -> Result<TradingStats> {
        let conn = self.conn.try_lock().context("Failed to acquire lock")?;
        self.get_stats_internal(&conn)
    }

    /// Internal stats retrieval
    fn get_stats_internal(&self, conn: &Connection) -> Result<TradingStats> {
        let mut stats = TradingStats::default();

        stats.total_trades = conn.query_row(
            "SELECT COUNT(*) FROM trade_results",
            [],
            |row| row.get(0),
        )?;

        if stats.total_trades == 0 {
            return Ok(stats);
        }

        stats.winning_trades = conn.query_row(
            "SELECT COUNT(*) FROM trade_results WHERE pnl > 0",
            [],
            |row| row.get(0),
        )?;

        stats.losing_trades = conn.query_row(
            "SELECT COUNT(*) FROM trade_results WHERE pnl < 0",
            [],
            |row| row.get(0),
        )?;

        stats.win_rate = stats.winning_trades as f64 / stats.total_trades as f64;
        stats.total_pnl = conn.query_row(
            "SELECT COALESCE(SUM(pnl), 0) FROM trade_results",
            [],
            |row| row.get(0),
        )?;
        stats.avg_pnl = conn.query_row(
            "SELECT COALESCE(AVG(pnl), 0) FROM trade_results",
            [],
            |row| row.get(0),
        )?;
        stats.avg_pnl_percent = conn.query_row(
            "SELECT COALESCE(AVG(pnl_percent), 0) FROM trade_results",
            [],
            |row| row.get(0),
        )?;

        if stats.winning_trades > 0 {
            stats.avg_win = conn.query_row(
                "SELECT COALESCE(AVG(pnl), 0) FROM trade_results WHERE pnl > 0",
                [],
                |row| row.get(0),
            )?;
        }

        if stats.losing_trades > 0 {
            stats.avg_loss = conn.query_row(
                "SELECT COALESCE(AVG(pnl), 0) FROM trade_results WHERE pnl < 0",
                [],
                |row| row.get::<_, f64>(0),
            )?.abs();
        }

        if stats.avg_loss > 0.0 {
            stats.profit_factor = stats.avg_win / stats.avg_loss;
        }

        stats.max_drawdown = conn.query_row(
            "SELECT COALESCE(MIN(pnl_percent), 0) FROM trade_results",
            [],
            |row| row.get::<_, f64>(0),
        )?;

        stats.avg_trade_duration_ms = conn.query_row(
            "SELECT COALESCE(AVG(duration_ms), 0) FROM trade_results",
            [],
            |row| row.get::<_, i64>(0),
        )?;

        Ok(stats)
    }

    /// Get statistics for a specific symbol
    pub fn get_stats_for_symbol(&self, symbol: &str) -> Result<TradingStats> {
        let conn = self.conn.try_lock().context("Failed to acquire lock")?;
        let mut stats = TradingStats::default();

        stats.total_trades = conn.query_row(
            "SELECT COUNT(*) FROM trades t 
             INNER JOIN trade_results r ON t.id = r.trade_id 
             WHERE t.symbol = ?1",
            [symbol],
            |row| row.get(0),
        )?;

        if stats.total_trades == 0 {
            return Ok(stats);
        }

        stats.winning_trades = conn.query_row(
            "SELECT COUNT(*) FROM trades t 
             INNER JOIN trade_results r ON t.id = r.trade_id 
             WHERE t.symbol = ?1 AND r.pnl > 0",
            [symbol],
            |row| row.get(0),
        )?;

        stats.losing_trades = conn.query_row(
            "SELECT COUNT(*) FROM trades t 
             INNER JOIN trade_results r ON t.id = r.trade_id 
             WHERE t.symbol = ?1 AND r.pnl < 0",
            [symbol],
            |row| row.get(0),
        )?;

        stats.win_rate = stats.winning_trades as f64 / stats.total_trades as f64;
        stats.total_pnl = conn.query_row(
            "SELECT COALESCE(SUM(r.pnl), 0) FROM trades t 
             INNER JOIN trade_results r ON t.id = r.trade_id 
             WHERE t.symbol = ?1",
            [symbol],
            |row| row.get(0),
        )?;
        stats.avg_pnl = conn.query_row(
            "SELECT COALESCE(AVG(r.pnl), 0) FROM trades t 
             INNER JOIN trade_results r ON t.id = r.trade_id 
             WHERE t.symbol = ?1",
            [symbol],
            |row| row.get(0),
        )?;

        Ok(stats)
    }

    /// Format stats for LLM context
    pub fn format_stats_for_prompt(&self) -> Result<String> {
        let stats = self.get_stats()?;
        
        Ok(format!(
            r#"**Trading Performance Statistics**

- Total Trades: {total}
- Win Rate: {win_rate:.1}%
- Total PnL: ${total_pnl:.2}
- Average PnL: ${avg_pnl:.2} ({avg_pnl_pct:.2}%)
- Average Win: ${avg_win:.2}
- Average Loss: ${avg_loss:.2}
- Profit Factor: {pf:.2}
- Max Drawdown: {dd:.2}%
- Avg Trade Duration: {duration:.0}ms"#,
            total = stats.total_trades,
            win_rate = stats.win_rate * 100.0,
            total_pnl = stats.total_pnl,
            avg_pnl = stats.avg_pnl,
            avg_pnl_pct = stats.avg_pnl_percent,
            avg_win = stats.avg_win,
            avg_loss = stats.avg_loss,
            pf = stats.profit_factor,
            dd = stats.max_drawdown,
            duration = stats.avg_trade_duration_ms,
        ))
    }

    /// Format recent history for LLM context
    pub fn format_history_for_prompt(&self, symbol: &str, limit: usize) -> Result<String> {
        let history = self.recall_history(symbol, limit)?;
        
        if history.is_empty() {
            return Ok("No recent trading history.".to_string());
        }

        let mut lines = Vec::new();
        lines.push("**Recent Trading History**".to_string());

        for trade in history {
            let result_str = if let Some(r) = trade.result {
                format!("PnL: ${:.2} ({:.2}%)", r.pnl, r.pnl_percent)
            } else {
                "Open".to_string()
            };

            lines.push(format!(
                "- {} {} @ {}: {} | Confluence: {}/10 | {}",
                trade.timestamp_ms,
                trade.symbol,
                trade.signal_price,
                trade.action,
                trade.confluence_score,
                result_str,
            ));
        }

        Ok(lines.join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zeroclaw_common::{IndicatorSnapshot, MarketRegime, Signal};

    fn create_test_signal() -> SignalOutput {
        SignalOutput {
            price: 50000.0,
            timestamp_ms: 1000,
            market_regime: MarketRegime::Trending,
            confluence_score: 7,
            indicator_snapshot: IndicatorSnapshot::default(),
            trade_advice: Signal::long(0.8),
            ingestion_latency_ms: 100,
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
            reasoning: "Test reasoning".to_string(),
        }
    }

    #[test]
    fn test_memory_creation() {
        let memory = TradingMemory::new_in_memory().unwrap();
        assert!(memory.get_stats().is_ok());
    }

    #[test]
    fn test_store_and_recall_decision() {
        let memory = TradingMemory::new_in_memory().unwrap();
        
        let signal = create_test_signal();
        let decision = create_test_decision();
        
        let trade_id = memory.store_decision("BTCUSDT", &signal, &decision).unwrap();
        assert!(trade_id > 0);
        
        let history = memory.recall_history("BTCUSDT", 10).unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].symbol, "BTCUSDT");
        assert_eq!(history[0].action, "LONG");
    }

    #[test]
    fn test_record_trade_result() {
        let memory = TradingMemory::new_in_memory().unwrap();
        
        let signal = create_test_signal();
        let decision = create_test_decision();
        
        let trade_id = memory.store_decision("BTCUSDT", &signal, &decision).unwrap();
        
        let result_id = memory.record_trade_result(
            trade_id,
            100.0,
            0.2,
            50100.0,
            2000,
        ).unwrap();
        
        assert!(result_id > 0);
        
        let history = memory.recall_history("BTCUSDT", 10).unwrap();
        assert!(history[0].result.is_some());
        assert_eq!(history[0].result.as_ref().unwrap().pnl, 100.0);
    }

    #[tokio::test]
    async fn test_async_operations() {
        let memory = TradingMemory::new_in_memory().unwrap();
        
        let signal = create_test_signal();
        let decision = create_test_decision();
        
        let trade_id = memory.store_decision_async("BTCUSDT", &signal, &decision).await.unwrap();
        assert!(trade_id > 0);
        
        let history = memory.recall_history_async("BTCUSDT", 10).await.unwrap();
        assert_eq!(history.len(), 1);
    }
}
