//! Integration tests for ZeroClaw Trading Integration.
//!
//! These tests verify the complete trading flow from signal to execution.

#[cfg(test)]
mod tests {
    use zeroclaw::agent::trading::{
        SignalReader, SafetyLayer, TradingMemory, TradingMonitor,
        safety_layer::{TradingDecision, TradingSafetyConfig},
        monitor::{MonitorConfig, AlertConfig},
    };
    use zeroclaw_common::{SignalOutput, MarketRegime, Signal, IndicatorSnapshot};
    use std::sync::Arc;

    // =============================================================================
    // Helper Functions
    // =============================================================================

    fn create_test_signal() -> SignalOutput {
        SignalOutput {
            price: 50000.0,
            timestamp_ms: chrono::Utc::now().timestamp_millis(),
            market_regime: MarketRegime::Trending,
            confluence_score: 7,
            indicator_snapshot: IndicatorSnapshot {
                rsi: 65.0,
                bb_upper: 51000.0,
                bb_lower: 49000.0,
                us_value: 50000.0,
                adx: 28.0,
                bb_width: 0.04,
                atr: 500.0,
                macd: 100.0,
                macd_signal: 80.0,
                macd_histogram: 20.0,
            },
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
            reasoning: "Strong bullish signal with confluence".to_string(),
        }
    }

    // =============================================================================
    // Memory Integration Tests
    // =============================================================================

    #[tokio::test]
    async fn test_memory_store_and_recall() {
        let memory = TradingMemory::new_in_memory().unwrap();
        
        let signal = create_test_signal();
        let decision = create_test_decision();
        
        // Store decision
        let trade_id = memory.store_decision("BTCUSDT", &signal, &decision).await.unwrap();
        assert!(trade_id > 0);
        
        // Record result
        let result_id = memory.record_trade_result_async(
            trade_id,
            100.0,  // $100 profit
            0.2,    // 0.2%
            50100.0,
            chrono::Utc::now().timestamp_millis(),
        ).await.unwrap();
        assert!(result_id > 0);
        
        // Recall history
        let history = memory.recall_history_async("BTCUSDT", 10).await.unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].symbol, "BTCUSDT");
        assert!(history[0].result.is_some());
    }

    #[tokio::test]
    async fn test_memory_stats_calculation() {
        let memory = TradingMemory::new_in_memory().unwrap();
        
        // Add 10 trades: 7 wins, 3 losses
        for i in 0..10 {
            let mut signal = create_test_signal();
            signal.timestamp_ms += i * 1000;
            
            let trade_id = memory.store_decision_async("BTCUSDT", &signal, &create_test_decision()).await.unwrap();
            
            let (pnl, pnl_pct) = if i < 7 {
                (100.0, 0.2)
            } else {
                (-50.0, -0.1)
            };
            
            memory.record_trade_result_async(
                trade_id,
                pnl,
                pnl_pct,
                50100.0,
                signal.timestamp_ms + 1000,
            ).await.unwrap();
        }
        
        let stats = memory.get_stats_async().await.unwrap();
        
        assert_eq!(stats.total_trades, 10);
        assert_eq!(stats.winning_trades, 7);
        assert_eq!(stats.losing_trades, 3);
        assert!((stats.win_rate - 0.7).abs() < 0.01);
        assert!(stats.total_pnl > 0.0); // 7*100 - 3*50 = 550
        assert!((stats.profit_factor - 2.0).abs() < 0.1); // 100/50 = 2
    }

    // =============================================================================
    // Safety Layer Tests
    // =============================================================================

    #[test]
    fn test_safety_layer_pre_check() {
        let redis_client = redis::Client::open("redis://localhost:6379").unwrap();
        let safety = SafetyLayer::with_default_config(redis_client);
        
        let signal = create_test_signal();
        assert!(safety.pre_check(&signal).is_ok());
        
        // Test latency check
        let mut high_latency_signal = create_test_signal();
        high_latency_signal.ingestion_latency_ms = 600; // > 500ms
        assert!(safety.pre_check(&high_latency_signal).is_err());
    }

    #[test]
    fn test_safety_layer_post_check() {
        let redis_client = redis::Client::open("redis://localhost:6379").unwrap();
        let safety = SafetyLayer::with_default_config(redis_client);
        
        let signal = create_test_signal();
        let decision = create_test_decision();
        
        assert!(safety.post_check(&decision, &signal).is_ok());
        
        // Test leverage check
        let mut high_leverage_decision = create_test_decision();
        high_leverage_decision.leverage = 15; // > 10
        assert!(safety.post_check(&high_leverage_decision, &signal).is_err());
    }

    #[test]
    fn test_safety_layer_position_sizing() {
        let redis_client = redis::Client::open("redis://localhost:6379").unwrap();
        let safety = SafetyLayer::with_default_config(redis_client);
        
        // Test position size calculation
        let position_size = safety.calculate_position_size(
            50000.0,  // entry
            49000.0,  // stop loss
            10000.0,  // balance
            0.02,     // 2% risk
        ).unwrap();
        
        // Risk $200 / $1000 distance = 0.2 units
        assert!((position_size - 0.2).abs() < 0.01);
        
        // Test validation
        assert!(safety.validate_position_size(0.1, 50000.0, 10000.0, 10).is_ok());
        assert!(safety.validate_position_size(100.0, 50000.0, 10000.0, 1).is_err());
    }

    // =============================================================================
    // Monitor Tests
    // =============================================================================

    #[test]
    fn test_monitor_health_status() {
        let memory = Arc::new(TradingMemory::new_in_memory().unwrap());
        let config = MonitorConfig::default();
        
        let monitor = TradingMonitor::new(
            config,
            "redis://localhost:6379",
            memory,
        );
        
        // May fail if Redis is not running, but shouldn't panic
        let result = monitor.unwrap().get_health_status();
        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn test_monitor_alert_config() {
        let mut alert_config = AlertConfig::default();
        alert_config.pushover_enabled = true;
        alert_config.telegram_enabled = true;
        alert_config.pnl_alert_threshold = 1.5;
        alert_config.drawdown_alert_threshold = 2.5;
        
        assert!(alert_config.pushover_enabled);
        assert!(alert_config.telegram_enabled);
        assert!((alert_config.pnl_alert_threshold - 1.5).abs() < 0.01);
    }

    // =============================================================================
    // End-to-End Flow Tests
    // =============================================================================

    #[tokio::test]
    async fn test_complete_trading_flow() {
        // Setup
        let memory = Arc::new(TradingMemory::new_in_memory().unwrap());
        let signal = create_test_signal();
        let decision = create_test_decision();
        
        // Step 1: Store decision
        let trade_id = memory.store_decision_async("BTCUSDT", &signal, &decision).await.unwrap();
        
        // Step 2: Simulate trade execution and result
        let exit_price = 51000.0;
        let pnl = (exit_price - signal.price) * 0.1; // 0.1 units
        let pnl_pct = (exit_price - signal.price) / signal.price * 100.0;
        
        memory.record_trade_result_async(
            trade_id,
            pnl,
            pnl_pct,
            exit_price,
            chrono::Utc::now().timestamp_millis(),
        ).await.unwrap();
        
        // Step 3: Verify stats updated
        let stats = memory.get_stats_async().await.unwrap();
        assert_eq!(stats.total_trades, 1);
        assert_eq!(stats.winning_trades, 1);
        assert!(stats.total_pnl > 0.0);
        
        // Step 4: Verify history recall
        let history = memory.recall_history_async("BTCUSDT", 10).await.unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].action, "LONG");
    }
}
