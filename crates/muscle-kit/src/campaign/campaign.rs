//! Campaign struct for multi-symbol trading

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use crate::engine::dataframe::DataFrameManager;
use super::config::CampaignConfig;

/// Campaign status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CampaignStatus {
    Created,
    Running,
    Stopped,
    Error,
}

/// Symbol state within a campaign
pub struct SymbolState {
    pub symbol: String,
    pub interval: String,
    pub dataframe: DataFrameManager,
    pub daily_trade_count: usize,
}

/// Campaign for multi-symbol trading
pub struct Campaign {
    pub config: CampaignConfig,
    pub status: CampaignStatus,
    pub symbol_states: Arc<Mutex<HashMap<String, SymbolState>>>,
    pub daily_trade_count: Arc<Mutex<usize>>,
    pub total_pnl: Arc<Mutex<f64>>,
}

impl Clone for Campaign {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            status: self.status,
            symbol_states: self.symbol_states.clone(),
            daily_trade_count: self.daily_trade_count.clone(),
            total_pnl: self.total_pnl.clone(),
        }
    }
}

impl Campaign {
    /// Create a new campaign from config
    pub fn new(config: CampaignConfig) -> Self {
        let mut symbol_states = HashMap::new();
        
        for symbol in &config.symbols {
            let interval = config.symbol_intervals
                .get(symbol)
                .cloned()
                .unwrap_or(config.default_interval.clone());
            
            symbol_states.insert(symbol.clone(), SymbolState {
                symbol: symbol.clone(),
                interval,
                dataframe: DataFrameManager::new(),
                daily_trade_count: 0,
            });
        }
        
        Self {
            config,
            status: CampaignStatus::Created,
            symbol_states: Arc::new(Mutex::new(symbol_states)),
            daily_trade_count: Arc::new(Mutex::new(0)),
            total_pnl: Arc::new(Mutex::new(0.0)),
        }
    }
    
    /// Start campaign
    pub fn start(&mut self) {
        self.status = CampaignStatus::Running;
    }
    
    /// Stop campaign
    pub fn stop(&mut self) {
        self.status = CampaignStatus::Stopped;
    }
}
