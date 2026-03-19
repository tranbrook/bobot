//! Decision Handler - LLM integration and decision processing.

use anyhow::{Context, Result};
use reqwest::Client;
use serde::ser::Error;
use tracing::{debug, info};
use zeroclaw_common::LLMDecision;

/// Decision Handler for LLM inference
pub struct DecisionHandler {
    client: Client,
    endpoint: String,
    api_key: String,
    model: String,
}

impl DecisionHandler {
    /// Create a new Decision Handler
    pub fn new(endpoint: &str, api_key: &str, model: &str) -> Self {
        Self {
            client: Client::new(),
            endpoint: endpoint.to_string(),
            api_key: api_key.to_string(),
            model: model.to_string(),
        }
    }

    /// Send prompt to LLM and get decision
    pub async fn get_decision(&self, prompt: &str, system_prompt: &str) -> Result<LLMDecision> {
        let response = self.client
            .post(&self.endpoint)
            .json(&serde_json::json!({
                "model": self.model,
                "prompt": prompt,
                "system": system_prompt,
                "stream": false,
                "format": "json"
            }))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .send()
            .await
            .context("Failed to send LLM request")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!("LLM API error: {} - {}", status, body));
        }

        let result: serde_json::Value = response.json().await
            .context("Failed to parse LLM response")?;

        debug!("LLM response: {:?}", result);

        // Extract response text
        let response_text = result.get("response")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        // Parse JSON from response
        let decision: LLMDecision = serde_json::from_str(response_text)
            .or_else(|_| {
                // Try to find JSON in response text
                if let Some(start) = response_text.find('{') {
                    if let Some(end) = response_text.rfind('}') {
                        serde_json::from_str(&response_text[start..=end])
                    } else {
                        Err(serde_json::Error::custom("No JSON found".to_string()))
                    }
                } else {
                    Err(serde_json::Error::custom("No JSON found".to_string()))
                }
            })
            .context("Failed to parse LLM decision JSON")?;

        info!("LLM decision: {:?} - {:?}", decision.decision, decision.action);
        Ok(decision)
    }

    /// Validate decision format
    pub fn validate_decision(&self, decision: &LLMDecision) -> Result<()> {
        if !["EXECUTE", "WATCH"].contains(&decision.decision.as_str()) {
            return Err(anyhow::anyhow!("Invalid decision: {}", decision.decision));
        }

        if decision.should_execute() && !["LONG", "SHORT"].contains(&decision.action.as_str()) {
            return Err(anyhow::anyhow!("Invalid action for EXECUTE: {}", decision.action));
        }

        if decision.params.leverage < 1 || decision.params.leverage > 10 {
            return Err(anyhow::anyhow!("Invalid leverage: {}", decision.params.leverage));
        }

        if decision.params.risk_percent < 0.0 || decision.params.risk_percent > 0.1 {
            return Err(anyhow::anyhow!("Invalid risk_percent: {}", decision.params.risk_percent));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zeroclaw_common::DecisionParams;

    #[test]
    fn test_validate_decision_valid() {
        let handler = DecisionHandler::new("http://test", "", "test");
        
        let decision = LLMDecision {
            decision: "EXECUTE".to_string(),
            action: "LONG".to_string(),
            params: DecisionParams {
                leverage: 5,
                stop_loss: 49000.0,
                take_profit: 52000.0,
                risk_percent: 0.02,
            },
        };

        assert!(handler.validate_decision(&decision).is_ok());
    }

    #[test]
    fn test_validate_decision_invalid() {
        let handler = DecisionHandler::new("http://test", "", "test");
        
        let decision = LLMDecision {
            decision: "INVALID".to_string(),
            action: "LONG".to_string(),
            params: DecisionParams {
                leverage: 5,
                stop_loss: 49000.0,
                take_profit: 52000.0,
                risk_percent: 0.02,
            },
        };

        assert!(handler.validate_decision(&decision).is_err());
    }
}
