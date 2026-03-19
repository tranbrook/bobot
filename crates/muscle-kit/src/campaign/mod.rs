//! Campaign module for Muscle-Kit
//! 
//! This module provides campaign-based trading with multi-symbol support.
//! A campaign can contain multiple symbols with shared risk management.

pub mod config;
pub mod manager;
pub mod campaign;

pub use config::CampaignConfig;
pub use manager::CampaignManager;
pub use campaign::Campaign;
