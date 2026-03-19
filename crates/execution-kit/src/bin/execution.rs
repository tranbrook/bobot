//! Execution-Kit Binary - Binance Order Execution Engine
//!
//! Full implementation with:
//! - Redis queue listener (BRPOP)
//! - Binance Spot/Futures order placement
//! - Position management
//! - PnL tracking
//! - Fill handling

use anyhow::{Context, Result};
use clap::Parser;
use redis::{Client, Commands, Connection};
use reqwest::{Client as HttpClient, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::signal;
use tracing::{debug, error, info, warn};

#[derive(Parser, Debug)]
struct Args {
    #[arg(short, long, default_value = "execution-config.toml")]
    config: String,
    #[arg(long)]
    redis_url: Option<String>,
    #[arg(long)]
    binance_api_key: Option<String>,
    #[arg(long)]
    binance_api_secret: Option<String>,
    #[arg(long, default_value = "true")]
    testnet: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ExecutionConfig {
    redis_url: String,
    symbols: Vec<String>,
    binance: BinanceConfig,
    risk: RiskConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BinanceConfig {
    testnet: bool,
    api_key: Option<String>,
    api_secret: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RiskConfig {
    max_position_size_usd: f64,
    max_daily_trades: usize,
    emergency_pnl_threshold: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ExecutionCommand {
    symbol: String,
    side: String,
    order_type: String,
    quantity: f64,
    stop_loss: Option<f64>,
    take_profit: Option<f64>,
    timestamp_ms: i64,
    market_type: String,
    price: Option<f64>,
    post_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Position {
    symbol: String,
    side: String,
    quantity: f64,
    entry_price: f64,
    stop_loss: Option<f64>,
    take_profit: Option<f64>,
    opened_at_ms: i64,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    let args = Args::parse();

    info!("⚡ Execution-Kit Order Engine Starting...");

    // Load config
    let config = load_config(&args.config, &args)?;
    info!("Config loaded: Redis={}, Symbols={:?}", config.redis_url, config.symbols);

    // Connect to Redis
    let redis_client = Client::open(config.redis_url.as_str())?;
    let mut conn = redis_client.get_connection()?;
    info!("✅ Redis connected");

    // Create HTTP client for Binance API
    let http_client = HttpClient::new();

    // Position tracking
    let mut positions: HashMap<String, Position> = HashMap::new();
    let mut daily_trades = 0usize;

    info!("🚀 Starting order execution loop...");

    // Main execution loop
    loop {
        tokio::select! {
            _ = tokio::time::sleep(tokio::time::Duration::from_millis(100)) => {
                // Check for new orders in queue
                match check_orders(&mut conn, &config.symbols) {
                    Ok(Some(cmd)) => {
                        if let Err(e) = execute_order(
                            &http_client,
                            &mut conn,
                            &config,
                            &mut positions,
                            &mut daily_trades,
                            &cmd,
                        ).await {
                            error!("Order execution failed: {}", e);
                        }
                    }
                    Ok(None) => {}
                    Err(e) => error!("Failed to check orders: {}", e),
                }

                // Update PnL for open positions
                if let Err(e) = update_pnl(&http_client, &config, &positions).await {
                    debug!("PnL update error: {}", e);
                }
            }
            _ = signal::ctrl_c() => {
                info!("\n🛑 Shutting down... Daily trades: {}", daily_trades);
                break;
            }
        }
    }

    Ok(())
}

fn load_config(path: &str, args: &Args) -> Result<ExecutionConfig> {
    let mut config: ExecutionConfig = if let Ok(content) = std::fs::read_to_string(path) {
        toml::from_str(&content).context("Failed to parse config")?
    } else {
        ExecutionConfig {
            redis_url: "redis://127.0.0.1:6379".to_string(),
            symbols: vec!["BTCUSDT".to_string()],
            binance: BinanceConfig {
                testnet: true,
                api_key: None,
                api_secret: None,
            },
            risk: RiskConfig {
                max_position_size_usd: 1000.0,
                max_daily_trades: 20,
                emergency_pnl_threshold: -5.0,
            },
        }
    };

    // Override with CLI args
    if let Some(redis_url) = &args.redis_url {
        config.redis_url = redis_url.clone();
    }
    if let Some(api_key) = &args.binance_api_key {
        config.binance.api_key = Some(api_key.clone());
    }
    if let Some(api_secret) = &args.binance_api_secret {
        config.binance.api_secret = Some(api_secret.clone());
    }
    config.binance.testnet = args.testnet;

    Ok(config)
}

fn check_orders(conn: &mut Connection, symbols: &[String]) -> Result<Option<ExecutionCommand>> {
    for symbol in symbols {
        let key = format!("execution:queue:{}", symbol);
        // Use brpop with 1 second timeout, returns Option<(key, value)>
        let result: Option<(String, String)> = conn.brpop(&key, 1.0)?;
        
        if let Some((_, json_str)) = result {
            if let Ok(cmd) = serde_json::from_str::<ExecutionCommand>(&json_str) {
                info!("📥 Received order: {} {} {}", cmd.symbol, cmd.side, cmd.quantity);
                return Ok(Some(cmd));
            }
        }
    }
    Ok(None)
}

async fn execute_order(
    http_client: &HttpClient,
    conn: &mut Connection,
    config: &ExecutionConfig,
    positions: &mut HashMap<String, Position>,
    daily_trades: &mut usize,
    cmd: &ExecutionCommand,
) -> Result<()> {
    // Check daily trade limit
    if *daily_trades >= config.risk.max_daily_trades {
        return Err(anyhow::anyhow!("Daily trade limit reached"));
    }

    // Check if we already have a position
    if positions.contains_key(&cmd.symbol) {
        return Err(anyhow::anyhow!("Position already exists for {}", cmd.symbol));
    }

    // Place order on Binance
    let base_url = if config.binance.testnet {
        // Check if trading futures (symbol ends with USDT and market_type is FUTURES)
        if cmd.market_type == "FUTURES" {
            "https://demo-fapi.binance.com"      // Futures demo (new testnet)
        } else {
            "https://testnet.binance.vision"     // Spot testnet
        }
    } else {
        if cmd.market_type == "FUTURES" {
            "https://fapi.binance.com"           // Futures mainnet
        } else {
            "https://api.binance.com"            // Spot mainnet
        }
    };

    let api_key = config.binance.api_key.as_ref()
        .ok_or_else(|| anyhow::anyhow!("Binance API key not configured"))?;
    let api_secret = config.binance.api_secret.as_ref()
        .ok_or_else(|| anyhow::anyhow!("Binance API secret not configured"))?;

    // Create order parameters with owned Strings
    let mut params: HashMap<String, String> = HashMap::new();
    params.insert("symbol".to_string(), cmd.symbol.clone());
    params.insert("side".to_string(), cmd.side.clone());
    params.insert("type".to_string(), cmd.order_type.clone());
    params.insert("quantity".to_string(), cmd.quantity.to_string());
    params.insert("newOrderRespType".to_string(), "FULL".to_string());
    
    if cmd.order_type == "LIMIT" {
        if let Some(price) = cmd.price {
            params.insert("price".to_string(), price.to_string());
            params.insert("timeInForce".to_string(), "GTC".to_string());
        }
    }

    // Add signature
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)?
        .as_millis() as i64;
    params.insert("timestamp".to_string(), timestamp.to_string());

    let params_str = serialize_params(&params);
    let signature = hmac_sha256(api_secret, &params_str);
    params.insert("signature".to_string(), signature);

    // Place order
    // Use correct endpoint for Futures vs Spot
    let order_endpoint = if cmd.market_type == "FUTURES" {
        "/fapi/v1/order"
    } else {
        "/api/v3/order"
    };
    let url = format!("{}{}", base_url, order_endpoint);
    
    info!("📤 Placing order: {} {} {} on {}", cmd.symbol, cmd.side, cmd.quantity, url);
    
    let response = http_client
        .post(&url)
        .header("X-MBX-APIKEY", api_key)
        .form(&params)
        .send()
        .await?;

    let status = response.status();
    let response_text = response.text().await?;
    info!("📥 Response status: {}, body: {}", status, response_text);
    
    // Try to parse as JSON
    let order_info: Value = serde_json::from_str(&response_text)
        .map_err(|e| anyhow::anyhow!("Failed to parse response JSON: {}. Body: {}", e, response_text))?;
    
    // Check for error response
    if order_info.get("code").is_some() {
        let error_code = &order_info["code"];
        let error_msg = order_info.get("msg").and_then(|v| v.as_str()).unwrap_or("Unknown error");
        return Err(anyhow::anyhow!("Binance API error {}: {}", error_code, error_msg));
    }
    
    info!("✅ Order placed: {} (status: {})",
          order_info["orderId"], order_info["status"]);

    // Track position
    positions.insert(cmd.symbol.clone(), Position {
        symbol: cmd.symbol.clone(),
        side: cmd.side.clone(),
        quantity: cmd.quantity,
        entry_price: order_info["price"].as_f64().unwrap_or(0.0),
        stop_loss: cmd.stop_loss,
        take_profit: cmd.take_profit,
        opened_at_ms: cmd.timestamp_ms,
    });

    *daily_trades += 1;

    // Update Redis with position info
    let position_key = format!("market:position:{}", cmd.symbol);
    let position_json = serde_json::to_string(&positions[&cmd.symbol])?;
    conn.set::<_, _, ()>(&position_key, &position_json)?;

    Ok(())
}

async fn update_pnl(
    http_client: &HttpClient,
    config: &ExecutionConfig,
    positions: &HashMap<String, Position>,
) -> Result<()> {
    let base_url = if config.binance.testnet {
        "https://testnet.binance.vision"
    } else {
        "https://api.binance.com"
    };

    for (symbol, position) in positions {
        // Get current price
        let url = format!("{}/api/v3/ticker/price?symbol={}", base_url, symbol);
        let response = http_client.get(&url).send().await?;
        
        if response.status() == StatusCode::OK {
            let price_info: Value = response.json().await?;
            let current_price: f64 = price_info["price"].as_str()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0.0);

            // Calculate PnL
            let pnl = if position.side == "BUY" {
                (current_price - position.entry_price) * position.quantity
            } else {
                (position.entry_price - current_price) * position.quantity
            };

            let pnl_percent = (pnl / (position.entry_price * position.quantity)) * 100.0;

            debug!("{} PnL: ${:.2} ({:.2}%)", symbol, pnl, pnl_percent);

            // Check emergency close
            if pnl_percent < config.risk.emergency_pnl_threshold {
                warn!("🚨 Emergency close triggered for {} (PnL: {:.2}%)", symbol, pnl_percent);
                // TODO: Implement emergency close
            }
        }
    }

    Ok(())
}

fn hmac_sha256(secret: &str, message: &str) -> String {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    use hex::encode;

    type HmacSha256 = Hmac<Sha256>;
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(message.as_bytes());
    encode(mac.finalize().into_bytes())
}

fn serialize_params(params: &HashMap<String, String>) -> String {
    let mut pairs: Vec<_> = params.iter().collect();
    pairs.sort_by(|a, b| a.0.cmp(b.0));
    pairs
        .iter()
        .map(|(k, v)| format!("{}={}", k, v))
        .collect::<Vec<_>>()
        .join("&")
}
