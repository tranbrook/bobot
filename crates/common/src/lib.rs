//! Common data structures for ZeroClaw ecosystem.
//!
//! This crate provides shared types used across muscle-kit, brain, and other crates.

pub mod signal;
pub mod risk;

pub use signal::{
    ExecutionCommand, IndicatorSnapshot, LLMDecision, MarketRegime, Signal, SignalOutput,
    DecisionParams,
};
pub use risk::{PositionInfo, RiskConfig, AccountBalance};
