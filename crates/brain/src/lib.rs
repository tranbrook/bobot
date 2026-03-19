//! Brain - AI Decision Layer for ZeroClaw.
//!
//! This crate consumes signals from Muscle-Kit, generates AI-ready prompts,
//! and dispatches execution commands back to Redis.
//!
//! ## Architecture
//!
//! ```text
//! Muscle-Kit → Redis (market:signal) → Brain → Redis (execution:queue) → Execution Engine
//! ```
//!
//! ## Modules
//!
//! - `consumer` - Signal Consumer (reads from Redis)
//! - `safety` - Safety Gate (validates signal reliability)
//! - `prompt_factory` - AI Prompt Generator
//! - `decision` - Decision Handler & LLM Integration
//! - `dispatcher` - Command Dispatcher (pushes to execution queue)
//! - `risk_monitor` - PnL monitoring & emergency close

pub mod consumer;
pub mod safety;
pub mod prompt_factory;
pub mod decision;
pub mod dispatcher;
pub mod risk_monitor;

pub use consumer::SignalConsumer;
pub use safety::SafetyGate;
pub use prompt_factory::PromptFactory;
pub use decision::DecisionHandler;
pub use dispatcher::CommandDispatcher;
pub use risk_monitor::RiskMonitor;

/// Brain configuration
#[derive(Debug, Clone)]
pub struct BrainConfig {
    /// Redis connection URL
    pub redis_url: String,
    /// LLM API endpoint
    pub llm_endpoint: String,
    /// LLM API key
    pub llm_api_key: String,
    /// LLM model name
    pub llm_model: String,
}

impl Default for BrainConfig {
    fn default() -> Self {
        Self {
            redis_url: "redis://localhost:6379".to_string(),
            llm_endpoint: "http://localhost:11434/api/generate".to_string(),
            llm_api_key: String::new(),
            llm_model: "llama2".to_string(),
        }
    }
}
