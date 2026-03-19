//! Command Dispatcher - pushes execution commands to Redis queue.

use anyhow::{Context, Result};
use redis::aio::ConnectionManager;
use tracing::{debug, info};
use zeroclaw_common::ExecutionCommand;

/// Command Dispatcher that pushes to Redis execution:queue:{SYMBOL}
pub struct CommandDispatcher {
    redis: ConnectionManager,
    key_prefix: String,
}

impl CommandDispatcher {
    /// Create a new Command Dispatcher
    pub fn new(redis: ConnectionManager, key_prefix: Option<&str>) -> Self {
        Self {
            redis,
            key_prefix: key_prefix.unwrap_or("execution:queue").to_string(),
        }
    }

    /// Get the Redis key for a symbol's execution queue
    fn get_queue_key(&self, symbol: &str) -> String {
        format!("{}:{}", self.key_prefix, symbol.to_uppercase())
    }

    /// Push execution command to Redis queue using LPUSH
    pub async fn dispatch(&mut self, symbol: &str, command: &ExecutionCommand) -> Result<()> {
        let key = self.get_queue_key(symbol);
        let json = command.to_json().context("Failed to serialize command")?;

        redis::cmd("LPUSH")
            .arg(&key)
            .arg(&json)
            .query_async::<()>(&mut self.redis)
            .await
            .context("Failed to push command to Redis queue")?;

        info!("Dispatched command to {}: {:?}", key, command);
        debug!("Command JSON: {}", json);

        Ok(())
    }

    /// Dispatch multiple commands
    pub async fn dispatch_batch(
        &mut self,
        commands: &[(&str, ExecutionCommand)],
    ) -> Result<()> {
        for (symbol, command) in commands {
            if let Err(e) = self.dispatch(symbol, command).await {
                debug!("Failed to dispatch for {}: {}", symbol, e);
            }
        }
        Ok(())
    }

    /// Get queue length
    pub async fn get_queue_length(&mut self, symbol: &str) -> Result<usize> {
        let key = self.get_queue_key(symbol);
        
        let len: usize = redis::cmd("LLEN")
            .arg(&key)
            .query_async(&mut self.redis)
            .await
            .context("Failed to get queue length")?;

        Ok(len)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_queue_key() {
        let dispatcher = CommandDispatcher::new(
            ConnectionManager::ignore(),
            Some("execution:queue"),
        );
        
        assert_eq!(dispatcher.get_queue_key("BTCUSDT"), "execution:queue:BTCUSDT");
        assert_eq!(dispatcher.get_queue_key("ethusdt"), "execution:queue:ETHUSDT");
    }
}
