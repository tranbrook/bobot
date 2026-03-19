//! Campaign Manager for managing multiple trading campaigns

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use anyhow::{Context, Result};
use super::config::CampaignConfig;
use super::campaign::{Campaign, CampaignStatus};

/// Campaign Manager
pub struct CampaignManager {
    campaigns: Arc<RwLock<HashMap<String, Campaign>>>,
}

impl CampaignManager {
    /// Create a new Campaign Manager
    pub fn new() -> Self {
        Self {
            campaigns: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    
    /// Create a new campaign
    pub async fn create_campaign(&self, config: CampaignConfig) -> Result<String> {
        let mut campaigns = self.campaigns.write().await;
        
        if campaigns.contains_key(&config.name) {
            return Err(anyhow::anyhow!("Campaign '{}' already exists", config.name));
        }
        
        let name = config.name.clone();
        let campaign = Campaign::new(config);
        campaigns.insert(name.clone(), campaign);
        
        Ok(name)
    }
    
    /// Start a campaign
    pub async fn start_campaign(&self, name: &str) -> Result<()> {
        let mut campaigns = self.campaigns.write().await;
        
        let campaign = campaigns.get_mut(name)
            .ok_or_else(|| anyhow::anyhow!("Campaign '{}' not found", name))?;
        
        campaign.start();
        Ok(())
    }
    
    /// Stop a campaign
    pub async fn stop_campaign(&self, name: &str) -> Result<()> {
        let mut campaigns = self.campaigns.write().await;
        
        let campaign = campaigns.get_mut(name)
            .ok_or_else(|| anyhow::anyhow!("Campaign '{}' not found", name))?;
        
        campaign.stop();
        Ok(())
    }
    
    /// Get campaign status
    pub async fn get_campaign_status(&self, name: &str) -> Option<CampaignStatus> {
        let campaigns = self.campaigns.read().await;
        campaigns.get(name).map(|c| c.status)
    }
    
    /// List all campaigns
    pub async fn list_campaigns(&self) -> Vec<String> {
        let campaigns = self.campaigns.read().await;
        campaigns.keys().cloned().collect()
    }
    
    /// Get campaign by name
    pub async fn get_campaign(&self, name: &str) -> Option<Arc<Campaign>> {
        let campaigns = self.campaigns.read().await;
        campaigns.get(name).map(|c| Arc::new(c.clone()))
    }
    
    /// Delete a campaign
    pub async fn delete_campaign(&self, name: &str) -> Result<()> {
        let mut campaigns = self.campaigns.write().await;
        
        if !campaigns.contains_key(name) {
            return Err(anyhow::anyhow!("Campaign '{}' not found", name));
        }
        
        campaigns.remove(name);
        Ok(())
    }
    
    /// Load campaign from config file
    pub async fn load_from_file(&self, path: &str) -> Result<String> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read config file: {}", path))?;
        
        let config: CampaignConfig = toml::from_str(&content)
            .with_context(|| format!("Failed to parse config file: {}", path))?;
        
        self.create_campaign(config).await
    }
}

impl Default for CampaignManager {
    fn default() -> Self {
        Self::new()
    }
}

// Note: Campaign needs to implement Clone for this to work
// We'll need to add #[derive(Clone)] to Campaign struct
