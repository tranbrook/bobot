//! Redis Command Listener - BRPOP from execution:queue.

use anyhow::{Context, Result};
use redis::Client;
use tracing::{debug, error};
use zeroclaw_common::ExecutionCommand;

/// Redis Listener for execution commands
pub struct RedisListener {
    client: Client,
    key_prefix: String,
}

impl RedisListener {
    /// Create a new Redis Listener
    pub fn new(redis_url: &str) -> Result<Self> {
        let client = Client::open(redis_url)
            .context("Failed to create Redis client")?;
        
        Ok(Self {
            client,
            key_prefix: "execution:queue".to_string(),
        })
    }

    /// Get the Redis key for a symbol's queue
    fn get_queue_key(&self, symbol: &str) -> String {
        format!("{}:{}", self.key_prefix, symbol.to_uppercase())
    }

    /// Listen for commands on a specific symbol queue
    pub async fn listen_symbol(&mut self, symbol: &str) -> Result<ExecutionCommand> {
        let key = self.get_queue_key(symbol);
        
        let mut conn = self.client.get_connection_manager().await
            .context("Failed to get Redis connection manager")?;

        loop {
            // BRPOP with 1 second timeout
            let result: Option<(String, String)> = redis::cmd("BRPOP")
                .arg(&key)
                .arg(1)
                .query_async(&mut conn)
                .await
                .context("Failed to BRPOP from Redis")?;

            if let Some((_key, json)) = result {
                debug!("Received command from {}: {}", key, json);
                
                match serde_json::from_str::<ExecutionCommand>(&json) {
                    Ok(cmd) => return Ok(cmd),
                    Err(e) => {
                        error!("Failed to deserialize command: {}", e);
                        continue;
                    }
                }
            }
        }
    }

    /// Listen for commands on any symbol queue
    pub async fn listen_any(&mut self, symbols: &[&str]) -> Result<ExecutionCommand> {
        let keys: Vec<String> = symbols.iter()
            .map(|s| self.get_queue_key(s))
            .collect();

        let mut conn = self.client.get_connection_manager().await
            .context("Failed to get Redis connection manager")?;

        loop {
            let key_refs: Vec<&str> = keys.iter().map(|s| s.as_str()).collect();
            
            let result: Option<(String, String)> = redis::cmd("BRPOP")
                .arg(&key_refs)
                .arg(1)
                .query_async(&mut conn)
                .await?;

            if let Some((key, json)) = result {
                debug!("Received command from {}: {}", key, json);
                
                match serde_json::from_str::<ExecutionCommand>(&json) {
                    Ok(cmd) => return Ok(cmd),
                    Err(e) => {
                        error!("Failed to deserialize command: {}", e);
                        continue;
                    }
                }
            }
        }
    }

    /// Get queue length for a symbol
    pub async fn queue_length(&self, symbol: &str) -> Result<usize> {
        let key = self.get_queue_key(symbol);
        let mut conn = self.client.get_connection_manager().await?;
        
        let len: usize = redis::cmd("LLEN")
            .arg(&key)
            .query_async(&mut conn)
            .await?;

        Ok(len)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_queue_key() {
        let listener = RedisListener::new("redis://localhost:6379").unwrap();
        assert_eq!(listener.get_queue_key("BTCUSDT"), "execution:queue:BTCUSDT");
    }
}
