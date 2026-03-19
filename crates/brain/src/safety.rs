//! Safety Gate - validates signal reliability before processing.

use zeroclaw_common::{RiskConfig, SignalOutput};
use tracing::warn;

/// Safety Gate that validates signal reliability
pub struct SafetyGate {
    config: RiskConfig,
}

impl SafetyGate {
    /// Create a new Safety Gate with default config
    pub fn new() -> Self {
        Self {
            config: RiskConfig::default(),
        }
    }

    /// Create with custom config
    pub fn with_config(config: RiskConfig) -> Self {
        Self { config }
    }

    /// Check if signal data is reliable
    ///
    /// Returns false if:
    /// - ingestion_latency_ms > max_latency_ms
    /// - override_reason == "LATENCY_SPIKE"
    pub fn is_data_reliable(&self, signal: &SignalOutput) -> bool {
        // Check latency
        if signal.ingestion_latency_ms > self.config.max_latency_ms {
            warn!(
                "Signal latency too high: {}ms > {}ms",
                signal.ingestion_latency_ms, self.config.max_latency_ms
            );
            return false;
        }

        // Check override reason
        if signal.override_reason.as_deref() == Some("LATENCY_SPIKE") {
            warn!("Signal has LATENCY_SPIKE override");
            return false;
        }

        true
    }

    /// Get the recommended action based on safety check
    ///
    /// If data is unreliable, returns "WAIT" regardless of signal advice
    pub fn get_safe_action(&self, signal: &SignalOutput) -> &'static str {
        if self.is_data_reliable(signal) {
            signal.action()
        } else {
            "WAIT"
        }
    }

    /// Get the config
    pub fn config(&self) -> &RiskConfig {
        &self.config
    }

    /// Update the config
    pub fn set_config(&mut self, config: RiskConfig) {
        self.config = config;
    }
}

impl Default for SafetyGate {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zeroclaw_common::{IndicatorSnapshot, MarketRegime, Signal};

    fn create_test_signal(latency: i64, override_reason: Option<&str>) -> SignalOutput {
        SignalOutput {
            price: 50000.0,
            timestamp_ms: 1000,
            market_regime: MarketRegime::Trending,
            confluence_score: 5,
            indicator_snapshot: IndicatorSnapshot::default(),
            trade_advice: Signal::long(0.8),
            ingestion_latency_ms: latency,
            override_reason: override_reason.map(String::from),
        }
    }

    #[test]
    fn test_is_data_reliable_ok() {
        let gate = SafetyGate::new();
        let signal = create_test_signal(100, None);
        assert!(gate.is_data_reliable(&signal));
    }

    #[test]
    fn test_is_data_reliable_latency_too_high() {
        let gate = SafetyGate::new();
        let signal = create_test_signal(600, None); // > 500ms default
        assert!(!gate.is_data_reliable(&signal));
    }

    #[test]
    fn test_is_data_reliable_latency_spike() {
        let gate = SafetyGate::new();
        let signal = create_test_signal(100, Some("LATENCY_SPIKE"));
        assert!(!gate.is_data_reliable(&signal));
    }

    #[test]
    fn test_get_safe_action() {
        let gate = SafetyGate::new();
        
        // Reliable signal - should return actual action
        let signal = create_test_signal(100, None);
        assert_eq!(gate.get_safe_action(&signal), "LONG");

        // Unreliable signal - should return WAIT
        let signal = create_test_signal(600, None);
        assert_eq!(gate.get_safe_action(&signal), "WAIT");
    }

    #[test]
    fn test_custom_config() {
        let config = RiskConfig {
            max_latency_ms: 1000, // More lenient
            ..Default::default()
        };
        let mut gate = SafetyGate::with_config(config.clone());
        
        let signal = create_test_signal(800, None);
        assert!(gate.is_data_reliable(&signal));

        gate.set_config(RiskConfig::default());
        assert!(!gate.is_data_reliable(&signal));
    }
}
