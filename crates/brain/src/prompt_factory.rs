//! Prompt Factory - generates AI-ready prompts from signal data.

use zeroclaw_common::{AccountBalance, PositionInfo, SignalOutput};

/// Prompt Factory for generating LLM context
pub struct PromptFactory;

impl PromptFactory {
    /// Create a new Prompt Factory
    pub fn new() -> Self {
        Self
    }

    /// Generate prompt from signal and account data
    pub fn create_prompt(
        &self,
        signal: &SignalOutput,
        balance: Option<&AccountBalance>,
        position: Option<&PositionInfo>,
    ) -> String {
        let snapshot = &signal.indicator_snapshot;

        let mut prompt = format!(
            "Market Regime: {} | Confluence Score: {}/10\n",
            signal.market_regime, signal.confluence_score
        );

        prompt.push_str(&format!(
            "Indicators: RSI({:.1}), BB_Width({:.2}%), US_Value({:.2})\n",
            snapshot.rsi, snapshot.bb_width * 100.0, snapshot.us_value
        ));

        prompt.push_str(&format!(
            "MACD: {:.2} | Signal: {:.2} | Histogram: {:.2}\n",
            snapshot.macd, snapshot.macd_signal, snapshot.macd_histogram
        ));

        prompt.push_str(&format!("Muscle Advice: {}\n", signal.action()));

        if let Some(bal) = balance {
            prompt.push_str(&format!("Account Balance: ${:.2}\n", bal.total));
            prompt.push_str(&format!("Unrealized PnL: ${:.2} ({:.2}%)\n", bal.unrealized_pnl, bal.unrealized_pnl / bal.total * 100.0));
        }

        if let Some(pos) = position {
            prompt.push_str(&format!(
                "Active Position: {} {} @ {:.2}\n",
                pos.side, pos.quantity, pos.entry_price
            ));
            prompt.push_str(&format!(
                "Position PnL: ${:.2} ({:.2}%)\n",
                pos.unrealized_pnl, pos.unrealized_pnl_percent
            ));
        }

        prompt.push_str("\nWhat is your decision?\n");
        prompt.push_str("Return JSON: {\"decision\": \"EXECUTE\"|\"WATCH\", \"action\": \"LONG\"|\"SHORT\"|\"NONE\", \"params\": {\"leverage\": N, \"stop_loss\": N, \"take_profit\": N, \"risk_percent\": N}}");

        prompt
    }

    /// Generate system prompt for JSON schema enforcement
    pub fn system_prompt() -> &'static str {
        "You are a trading decision AI. Analyze the market data and return a JSON decision.\n\n\
         REQUIRED JSON FORMAT:\n\
         {\n\
           \"decision\": \"EXECUTE\" or \"WATCH\",\n\
           \"action\": \"LONG\", \"SHORT\", or \"NONE\",\n\
           \"params\": {\n\
             \"leverage\": 1-10,\n\
             \"stop_loss\": price_level,\n\
             \"take_profit\": price_level,\n\
             \"risk_percent\": 0.01-0.05\n\
           }\n\
         }\n\n\
         Rules:\n\
         - Only EXECUTE if confluence score >= 5 and data is reliable\n\
         - Use appropriate stop loss and take profit levels\n\
         - Never risk more than 5% per trade"
    }
}

impl Default for PromptFactory {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zeroclaw_common::{IndicatorSnapshot, MarketRegime, Signal};

    #[test]
    fn test_create_prompt_basic() {
        let factory = PromptFactory::new();
        let signal = SignalOutput {
            price: 50000.0,
            timestamp_ms: 1000,
            market_regime: MarketRegime::Trending,
            confluence_score: 7,
            indicator_snapshot: IndicatorSnapshot {
                rsi: 65.0,
                bb_width: 0.05,
                us_value: 49800.0,
                macd: 100.0,
                macd_signal: 80.0,
                macd_histogram: 20.0,
                ..Default::default()
            },
            trade_advice: Signal::long(0.8),
            ingestion_latency_ms: 50,
            override_reason: None,
        };

        let prompt = factory.create_prompt(&signal, None, None);
        
        assert!(prompt.contains("TRENDING"));
        assert!(prompt.contains("Confluence Score: 7/10"));
        assert!(prompt.contains("RSI(65.0)"));
        assert!(prompt.contains("Muscle Advice: LONG"));
    }

    #[test]
    fn test_system_prompt() {
        let prompt = PromptFactory::system_prompt();
        assert!(prompt.contains("EXECUTE"));
        assert!(prompt.contains("WATCH"));
        assert!(prompt.contains("JSON"));
    }
}
