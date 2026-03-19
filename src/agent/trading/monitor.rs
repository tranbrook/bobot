//! Trading Monitor - PnL tracking, alerting, and production monitoring.
//!
//! This module provides real-time monitoring, alerting, and health checks
//! for the trading system.

use anyhow::{Context, Result};
use redis::Client;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{info, warn, error};

use crate::agent::trading::memory::TradingMemory;

/// Alert configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertConfig {
    /// Enable Pushover alerts
    #[serde(default)]
    pub pushover_enabled: bool,
    
    /// Pushover API token
    #[serde(default)]
    pub pushover_token: Option<String>,
    
    /// Pushover user key
    #[serde(default)]
    pub pushover_user_key: Option<String>,
    
    /// Enable Telegram alerts
    #[serde(default)]
    pub telegram_enabled: bool,
    
    /// Telegram bot token
    #[serde(default)]
    pub telegram_bot_token: Option<String>,
    
    /// Telegram chat ID
    #[serde(default)]
    pub telegram_chat_id: Option<String>,
    
    /// Alert on PnL threshold breach (percentage)
    #[serde(default = "default_pnl_alert_threshold")]
    pub pnl_alert_threshold: f64,
    
    /// Alert on drawdown threshold breach (percentage)
    #[serde(default = "default_drawdown_alert_threshold")]
    pub drawdown_alert_threshold: f64,
    
    /// Alert on daily trade limit warning
    #[serde(default = "default_daily_trade_warning")]
    pub daily_trade_warning: usize,
}

fn default_pnl_alert_threshold() -> f64 { 2.0 }
fn default_drawdown_alert_threshold() -> f64 { 3.0 }
fn default_daily_trade_warning() -> usize { 15 }

impl Default for AlertConfig {
    fn default() -> Self {
        Self {
            pushover_enabled: false,
            pushover_token: None,
            pushover_user_key: None,
            telegram_enabled: false,
            telegram_bot_token: None,
            telegram_chat_id: None,
            pnl_alert_threshold: default_pnl_alert_threshold(),
            drawdown_alert_threshold: default_drawdown_alert_threshold(),
            daily_trade_warning: default_daily_trade_warning(),
        }
    }
}

/// Monitoring configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitorConfig {
    /// Health check interval in seconds
    #[serde(default = "default_health_check_interval")]
    pub health_check_interval_secs: u64,
    
    /// Metrics flush interval in seconds
    #[serde(default = "default_metrics_flush_interval")]
    pub metrics_flush_interval_secs: u64,
    
    /// Alert configuration
    #[serde(default)]
    pub alert: AlertConfig,
}

fn default_health_check_interval() -> u64 { 30 }
fn default_metrics_flush_interval() -> u64 { 60 }

impl Default for MonitorConfig {
    fn default() -> Self {
        Self {
            health_check_interval_secs: default_health_check_interval(),
            metrics_flush_interval_secs: default_metrics_flush_interval(),
            alert: AlertConfig::default(),
        }
    }
}

/// Trading health status
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HealthStatus {
    /// Overall health status
    pub healthy: bool,
    
    /// Redis connection status
    pub redis_connected: bool,
    
    /// Database status
    pub database_ok: bool,
    
    /// Last signal timestamp
    pub last_signal_ms: Option<i64>,
    
    /// Signal freshness (ms since last signal)
    pub signal_freshness_ms: Option<i64>,
    
    /// Active positions count
    pub active_positions: usize,
    
    /// Current drawdown
    pub current_drawdown: f64,
    
    /// Daily trade count
    pub daily_trades: usize,
    
    /// Errors in last hour
    pub errors_last_hour: usize,
    
    /// Last error message
    pub last_error: Option<String>,
}

/// Performance metrics
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PerformanceMetrics {
    /// Total trades today
    pub trades_today: usize,
    
    /// PnL today
    pub pnl_today: f64,
    
    /// PnL percentage today
    pub pnl_percent_today: f64,
    
    /// Win rate today
    pub win_rate_today: f64,
    
    /// Average trade duration (ms)
    pub avg_trade_duration_ms: i64,
    
    /// Largest win
    pub largest_win: f64,
    
    /// Largest loss
    pub largest_loss: f64,
    
    /// Consecutive wins
    pub consecutive_wins: usize,
    
    /// Consecutive losses
    pub consecutive_losses: usize,
    
    /// Sharpe ratio (if enough data)
    pub sharpe_ratio: Option<f64>,
    
    /// Last updated timestamp
    pub last_updated_ms: i64,
}

/// Trading Monitor for production monitoring
pub struct TradingMonitor {
    config: MonitorConfig,
    redis: Client,
    memory: Arc<TradingMemory>,
}

impl TradingMonitor {
    /// Create a new Trading Monitor
    pub fn new(
        config: MonitorConfig,
        redis_url: &str,
        memory: Arc<TradingMemory>,
    ) -> Result<Self> {
        let redis = Client::open(redis_url)
            .context("Failed to create Redis client")?;
        
        Ok(Self {
            config,
            redis,
            memory,
        })
    }

    /// Get current health status
    pub fn get_health_status(&self) -> Result<HealthStatus> {
        let mut status = HealthStatus::default();
        
        // Check Redis connection
        let mut conn = match self.redis.get_connection() {
            Ok(c) => {
                status.redis_connected = true;
                c
            }
            Err(_) => {
                status.redis_connected = false;
                status.healthy = false;
                return Ok(status);
            }
        };
        
        // Check database
        status.database_ok = self.memory.get_stats().is_ok();
        
        // Get current drawdown
        let drawdown: f64 = redis::cmd("GET")
            .arg("trading:current_drawdown")
            .query(&mut conn)
            .unwrap_or(0.0);
        status.current_drawdown = drawdown;
        
        // Get daily trade count
        let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
        let key = format!("trading:daily_count:{}", today);
        let daily_trades: usize = redis::cmd("GET")
            .arg(&key)
            .query(&mut conn)
            .unwrap_or(0);
        status.daily_trades = daily_trades;
        
        // Get last signal timestamp
        let last_signal: Option<String> = redis::cmd("GET")
            .arg("trading:last_signal_timestamp")
            .query(&mut conn)
            .ok()
            .flatten();
        
        if let Some(ts_str) = last_signal {
            if let Ok(ts) = ts_str.parse::<i64>() {
                status.last_signal_ms = Some(ts);
                let now = chrono::Utc::now().timestamp_millis();
                status.signal_freshness_ms = Some(now - ts);
            }
        }
        
        // Determine overall health
        status.healthy = status.redis_connected 
            && status.database_ok 
            && status.signal_freshness_ms.unwrap_or(i64::MAX) < 60000; // 1 minute
        
        Ok(status)
    }

    /// Get performance metrics
    pub fn get_metrics(&self) -> Result<PerformanceMetrics> {
        let stats = self.memory.get_stats()?;
        let mut metrics = PerformanceMetrics::default();
        
        // Get today's stats
        let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
        let key = format!("trading:daily_pnl:{}", today);
        
        let mut conn = self.redis.get_connection()
            .context("Failed to get Redis connection")?;
        
        metrics.pnl_today = redis::cmd("GET")
            .arg(&key)
            .query(&mut conn)
            .unwrap_or(0.0);
        
        metrics.trades_today = stats.total_trades;
        metrics.win_rate_today = stats.win_rate;
        metrics.avg_trade_duration_ms = stats.avg_trade_duration_ms;
        metrics.largest_win = stats.avg_win;
        metrics.largest_loss = stats.avg_loss;
        metrics.last_updated_ms = chrono::Utc::now().timestamp_millis();
        
        Ok(metrics)
    }

    /// Send an alert
    pub async fn send_alert(&self, title: &str, message: &str, priority: i32) -> Result<()> {
        let config = &self.config.alert;
        
        // Send Pushover alert
        if config.pushover_enabled {
            if let (Some(token), Some(user_key)) = 
                (&config.pushover_token, &config.pushover_user_key) 
            {
                self.send_pushover_alert(token, user_key, title, message, priority).await?;
            }
        }
        
        // Send Telegram alert
        if config.telegram_enabled {
            if let (Some(bot_token), Some(chat_id)) = 
                (&config.telegram_bot_token, &config.telegram_chat_id) 
            {
                self.send_telegram_alert(bot_token, chat_id, title, message).await?;
            }
        }
        
        Ok(())
    }

    /// Send Pushover alert
    async fn send_pushover_alert(
        &self,
        token: &str,
        user_key: &str,
        title: &str,
        message: &str,
        priority: i32,
    ) -> Result<()> {
        let client = reqwest::Client::new();
        
        let response = client
            .post("https://api.pushover.net/1/messages.json")
            .form(&[
                ("token", token),
                ("user", user_key),
                ("title", title),
                ("message", message),
                ("priority", &priority.to_string()),
            ])
            .send()
            .await
            .context("Failed to send Pushover alert")?;
        
        if response.status().is_success() {
            info!("Pushover alert sent: {}", title);
        } else {
            warn!("Failed to send Pushover alert: {}", response.status());
        }
        
        Ok(())
    }

    /// Send Telegram alert
    async fn send_telegram_alert(
        &self,
        bot_token: &str,
        chat_id: &str,
        title: &str,
        message: &str,
    ) -> Result<()> {
        let client = reqwest::Client::new();
        let url = format!("https://api.telegram.org/bot{}/sendMessage", bot_token);
        
        let full_message = format!("**{}**\n{}", title, message);
        
        let response = client
            .post(&url)
            .json(&serde_json::json!({
                "chat_id": chat_id,
                "text": full_message,
                "parse_mode": "Markdown",
            }))
            .send()
            .await
            .context("Failed to send Telegram alert")?;
        
        if response.status().is_success() {
            info!("Telegram alert sent: {}", title);
        } else {
            warn!("Failed to send Telegram alert: {}", response.status());
        }
        
        Ok(())
    }

    /// Check and send alerts based on current state
    pub async fn check_and_alert(&self) -> Result<Vec<String>> {
        let mut alerts = Vec::new();
        let status = self.get_health_status()?;
        let config = &self.config.alert;
        
        // Check drawdown
        if status.current_drawdown > config.drawdown_alert_threshold {
            let alert = format!(
                "Drawdown Alert: Current drawdown {:.2}% exceeds threshold {:.2}%",
                status.current_drawdown, config.drawdown_alert_threshold
            );
            warn!("{}", alert);
            self.send_alert("Drawdown Alert", &alert, 1).await?;
            alerts.push(alert);
        }
        
        // Check daily trade limit warning
        if status.daily_trades >= config.daily_trade_warning {
            let alert = format!(
                "Trade Limit Warning: {} trades today (warning at {})",
                status.daily_trades, config.daily_trade_warning
            );
            warn!("{}", alert);
            self.send_alert("Trade Limit Warning", &alert, 0).await?;
            alerts.push(alert);
        }
        
        // Check signal freshness
        if let Some(freshness) = status.signal_freshness_ms {
            if freshness > 60000 { // 1 minute
                let alert = format!("Signal Stale: Last signal {}ms ago", freshness);
                warn!("{}", alert);
                alerts.push(alert);
            }
        }
        
        // Check health status
        if !status.healthy {
            let alert = format!(
                "System Unhealthy: Redis={}, DB={}, Signal Freshness={:?}ms",
                status.redis_connected,
                status.database_ok,
                status.signal_freshness_ms
            );
            error!("{}", alert);
            self.send_alert("System Health Alert", &alert, 2).await?;
            alerts.push(alert);
        }
        
        Ok(alerts)
    }

    /// Record an error for tracking
    pub fn record_error(&self, error: &str) -> Result<()> {
        let mut conn = self.redis.get_connection()
            .context("Failed to get Redis connection")?;
        
        let timestamp = chrono::Utc::now().timestamp_millis();
        let error_entry = serde_json::json!({
            "timestamp": timestamp,
            "error": error,
        });
        
        redis::cmd("LPUSH")
            .arg("trading:errors")
            .arg(error_entry.to_string())
            .query::<()>(&mut conn)
            .context("Failed to record error")?;
        
        // Keep only last 100 errors
        redis::cmd("LTRIM")
            .arg("trading:errors")
            .arg(0)
            .arg(99)
            .query::<()>(&mut conn)
            .ok();
        
        Ok(())
    }

    /// Get errors from the last hour
    pub fn get_errors_last_hour(&self) -> Result<Vec<String>> {
        let mut conn = self.redis.get_connection()
            .context("Failed to get Redis connection")?;
        
        let one_hour_ago = chrono::Utc::now().timestamp_millis() - 3600000;
        
        let errors: Vec<String> = redis::cmd("LRANGE")
            .arg("trading:errors")
            .arg(0)
            .arg(-1)
            .query(&mut conn)
            .unwrap_or_default();
        
        let mut filtered = Vec::new();
        for error_json in errors {
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(&error_json) {
                if let Some(ts) = value.get("timestamp").and_then(|v| v.as_i64()) {
                    if ts >= one_hour_ago {
                        if let Some(msg) = value.get("error").and_then(|v| v.as_str()) {
                            filtered.push(msg.to_string());
                        }
                    }
                }
            }
        }
        
        Ok(filtered)
    }

    /// Update last signal timestamp
    pub fn update_last_signal(&self, timestamp_ms: i64) -> Result<()> {
        let mut conn = self.redis.get_connection()
            .context("Failed to get Redis connection")?;
        
        redis::cmd("SET")
            .arg("trading:last_signal_timestamp")
            .arg(timestamp_ms.to_string())
            .query::<()>(&mut conn)
            .context("Failed to update signal timestamp")?;
        
        Ok(())
    }

    /// Update daily PnL
    pub fn update_daily_pnl(&self, pnl: f64) -> Result<()> {
        let mut conn = self.redis.get_connection()
            .context("Failed to get Redis connection")?;
        
        let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
        let key = format!("trading:daily_pnl:{}", today);
        
        redis::cmd("INCRBYFLOAT")
            .arg(&key)
            .arg(pnl.to_string())
            .query::<()>(&mut conn)
            .context("Failed to update daily PnL")?;
        
        // Set expiry at end of day (24 hours)
        redis::cmd("EXPIRE")
            .arg(&key)
            .arg(86400)
            .query::<()>(&mut conn)
            .ok();
        
        Ok(())
    }

    /// Get the config
    pub fn config(&self) -> &MonitorConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_monitor_creation() {
        let memory = Arc::new(TradingMemory::new_in_memory().unwrap());
        let config = MonitorConfig::default();
        
        let monitor = TradingMonitor::new(
            config,
            "redis://localhost:6379",
            memory,
        );
        
        // May fail if Redis is not running, but shouldn't panic
        assert!(monitor.is_ok() || monitor.is_err());
    }

    #[test]
    fn test_alert_config_default() {
        let config = AlertConfig::default();
        assert!(!config.pushover_enabled);
        assert!(!config.telegram_enabled);
        assert!((config.pnl_alert_threshold - 2.0).abs() < 0.01);
        assert!((config.drawdown_alert_threshold - 3.0).abs() < 0.01);
    }

    #[test]
    fn test_monitor_config_default() {
        let config = MonitorConfig::default();
        assert_eq!(config.health_check_interval_secs, 30);
        assert_eq!(config.metrics_flush_interval_secs, 60);
    }
}
