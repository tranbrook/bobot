//! Muscle-Kit Campaign Binary
//! 
//! Campaign-based trading with multi-symbol support.
//! A campaign can contain multiple symbols with shared risk management.

use anyhow::{Context, Result};
use clap::Parser;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio_stream::StreamExt;
use tracing::{debug, error, info, warn};

use zeroclaw_muscle_kit::{
    Gatekeeper, MuscleConfig, RedisSignalStore,
    data::{KlineInterval, MarketRegime, Signal, SignalOutput, IndicatorSnapshot, fetch_bootstrap_klines, spawn_ws_client},
    engine::dataframe::DataFrameManager,
    engine::compute_ultimate_smoother_default,
    campaign::{CampaignManager, CampaignConfig},
};

#[derive(Parser, Debug)]
struct Args {
    /// Campaign config file
    #[arg(short, long)]
    campaign: Option<String>,
    
    /// Legacy mode: symbols comma-separated
    #[arg(short, long, default_value = "BTCUSDT,ETHUSDT")]
    symbols: String,
    
    /// Legacy mode: interval
    #[arg(long, default_value = "1m")]
    interval: String,
    
    /// Redis URL
    #[arg(long)]
    redis_url: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    let args = Args::parse();

    info!("🏛️  Muscle-Kit Campaign Mode Starting...");

    let campaign_manager = Arc::new(CampaignManager::new());
    
    // Load campaign or use legacy mode
    if let Some(campaign_file) = args.campaign {
        // Campaign mode
        let campaign_name = campaign_manager.load_from_file(&campaign_file).await?;
        info!("✓ Campaign loaded: {}", campaign_name);
        
        // Start campaign
        campaign_manager.start_campaign(&campaign_name).await?;
        info!("✓ Campaign started");
        
        // Run campaign mode
        run_campaign_mode(&campaign_manager, &campaign_name).await?;
    } else {
        // Legacy mode: direct symbol processing
        info!("📊 Legacy Mode: Direct symbol processing");
        run_legacy_mode(&args).await?;
    }
    
    // Wait for Ctrl+C
    tokio::signal::ctrl_c().await?;
    info!("Shutdown");
    Ok(())
}

async fn run_campaign_mode(
    campaign_manager: &CampaignManager,
    campaign_name: &str,
) -> Result<()> {
    use zeroclaw_muscle_kit::campaign::Campaign;
    
    info!("📊 Campaign Mode: Multi-symbol processing");
    
    // Get campaign - we need to load config again since Campaign doesn't expose config
    let campaign_config: CampaignConfig = {
        let content = std::fs::read_to_string(format!("campaigns/{}.toml", campaign_name))
            .or_else(|_| std::fs::read_to_string(campaign_name))
            .context("Failed to read campaign config")?;
        toml::from_str(&content).context("Failed to parse campaign config")?
    };
    
    let symbols = campaign_config.symbols;
    let default_interval = campaign_config.default_interval.clone();
    let symbol_intervals = campaign_config.symbol_intervals.clone();
    
    info!("Campaign: {}", campaign_name);
    info!("Symbols: {:?}", symbols);
    
    let mut config = load_config()?;
    let store = Arc::new(RedisSignalStore::new(&config.redis_url, Some("market:signal"))?);
    let gatekeeper = Arc::new(Gatekeeper::new());

    // Bootstrap and create DataFrame managers per symbol
    let mut dataframes: HashMap<String, DataFrameManager> = HashMap::new();
    for sym in &symbols {
        let interval_str = symbol_intervals.get(sym).unwrap_or(&default_interval);
        let interval = KlineInterval::from_str(interval_str).context("Invalid interval")?;
        
        let klines = fetch_bootstrap_klines(sym, interval.clone()).await?;
        let mut df = DataFrameManager::new();
        for kline in &klines {
            df.add_kline(kline)?;
        }
        dataframes.insert(sym.clone(), df);
        info!("✓ {} bootstrapped with {} candles (interval: {})", sym, klines.len(), interval_str);
    }

    info!("Starting real-time processing for {} symbols...", symbols.len());
    
    // Spawn a dedicated task for each symbol
    for sym in &symbols {
        let interval_str = symbol_intervals.get(sym).unwrap_or(&default_interval);
        let interval = KlineInterval::from_str(interval_str).context("Invalid interval")?;
        
        let (rx, _, _) = spawn_ws_client(sym, interval.clone(), None).await;
        info!("✓ {} streaming (interval: {})", sym, interval_str);
        
        let sym_clone = sym.clone();
        let df_clone = dataframes.remove(&sym_clone).unwrap();
        let df_arc = Arc::new(Mutex::new(df_clone));
        let store_clone = store.clone();
        let gk_clone = gatekeeper.clone();
        
        tokio::spawn(async move {
            let mut stream = tokio_stream::wrappers::ReceiverStream::new(rx);
            let mut count = 0u64;
            
            while let Some(kline) = stream.next().await {
                count += 1;
                
                // Verify symbol matches
                if kline.symbol != sym_clone {
                    error!("Symbol mismatch: expected {}, got {}", sym_clone, kline.symbol);
                    continue;
                }
                
                // Add kline to DataFrame
                {
                    let mut df = df_arc.lock().await;
                    if let Err(e) = df.add_kline(&kline) {
                        error!("{} add_kline error: {}", sym_clone, e);
                        continue;
                    }
                    
                    // Skip if not warmed up
                    if !df.is_warmed_up() {
                        continue;
                    }
                    
                    // Calculate indicators and add columns to DataFrame, then generate signal
                    let snap = match indicators(df.df_mut()) {
                        Ok(s) => s,
                        Err(e) => { error!("{} indicators error: {}", sym_clone, e); continue; }
                    };
                    
                    // DEBUG: Log DataFrame columns
                    if count <= 5 || count % 50 == 0 {
                        let df_cols: Vec<String> = df.df().get_column_names().iter().map(|s| s.to_string()).collect();
                        info!("{} [INFO] DataFrame columns: {:?}", sym_clone, df_cols);
                        info!("{} [INFO] Indicators: rsi={:.2}, bb_upper={:.2}, bb_lower={:.2}, us_value={:.2}, adx={:.2}",
                              sym_clone, snap.rsi, snap.bb_upper, snap.bb_lower, snap.us_value, snap.adx);
                    }
                    
                    let regime = regime(&snap);
                    let agg = gk_clone.aggregate(df.df(), &regime);
                    
                    // Log aggregation result
                    if count <= 5 || count % 50 == 0 {
                        info!("{} [INFO] Regime={}, Confluence={}, Active Strategies={}, Signal={}",
                              sym_clone, regime, agg.confluence_score, agg.strategy_count,
                              match agg.signal { Signal::Long{..} => "LONG", Signal::Short{..} => "SHORT", _ => "WAIT" });
                    }
                    let out = SignalOutput::new(kline.close, kline.timestamp_ms, regime, agg.confluence_score, snap, agg.signal, kline.timestamp_ms);
                    
                    if let Err(e) = store_clone.store_signal_async(&sym_clone, &out).await {
                        error!("{} store_signal error: {}", sym_clone, e);
                    }
                    
                    if count % 100 == 0 {
                        info!("{} processed {} klines, signal: {} c={}", sym_clone, count, action(&out.trade_advice), out.confluence_score);
                    }
                }
            }
        });
    }
    
    // Wait for Ctrl+C
    tokio::signal::ctrl_c().await?;
    info!("Campaign {} shutdown", campaign_name);
    Ok(())
}

async fn run_legacy_mode(args: &Args) -> Result<()> {
    let symbols: Vec<String> = args.symbols.split(',').map(|s| s.trim().to_string()).collect();
    let mut config = load_config()?;
    if let Some(ref redis_url) = args.redis_url {
        config.redis_url = redis_url.clone();
    }

    let store = Arc::new(RedisSignalStore::new(&config.redis_url, Some("market:signal"))?);
    let gatekeeper = Arc::new(Gatekeeper::new());
    let interval = KlineInterval::from_str(&args.interval).context("Invalid interval")?;

    info!("Symbols: {:?}, Interval: {:?}", symbols, interval);

    // Bootstrap and create DataFrame managers per symbol
    let mut dataframes: HashMap<String, DataFrameManager> = HashMap::new();
    for sym in &symbols {
        let klines = fetch_bootstrap_klines(sym, interval.clone()).await?;
        let mut df = DataFrameManager::new();
        for kline in &klines {
            df.add_kline(kline)?;
        }
        dataframes.insert(sym.clone(), df);
        info!("✓ {} bootstrapped with {} candles", sym, klines.len());
    }

    info!("Starting real-time processing for {} symbols...", symbols.len());
    
    // Spawn a dedicated task for each symbol
    for sym in &symbols {
        let (rx, _, _) = spawn_ws_client(sym, interval.clone(), None).await;
        info!("✓ {} streaming", sym);
        
        let sym_clone = sym.clone();
        let df_clone = dataframes.remove(&sym_clone).unwrap();
        let df_arc = Arc::new(Mutex::new(df_clone));
        let store_clone = store.clone();
        let gk_clone = gatekeeper.clone();
        
        tokio::spawn(async move {
            let mut stream = tokio_stream::wrappers::ReceiverStream::new(rx);
            let mut count = 0u64;
            
            while let Some(kline) = stream.next().await {
                count += 1;
                
                // Verify symbol matches
                if kline.symbol != sym_clone {
                    error!("Symbol mismatch: expected {}, got {}", sym_clone, kline.symbol);
                    continue;
                }
                
                // Add kline to DataFrame
                {
                    let mut df = df_arc.lock().await;
                    if let Err(e) = df.add_kline(&kline) {
                        error!("{} add_kline error: {}", sym_clone, e);
                        continue;
                    }
                    
                    // Skip if not warmed up
                    if !df.is_warmed_up() {
                        continue;
                    }
                    
                    // Calculate indicators and generate signal
                    let snap = match indicators(df.df_mut()) {
                        Ok(s) => s,
                        Err(e) => { error!("{} indicators error: {}", sym_clone, e); continue; }
                    };
                    
                    let regime = regime(&snap);
                    let agg = gk_clone.aggregate(df.df(), &regime);
                    let out = SignalOutput::new(kline.close, kline.timestamp_ms, regime, agg.confluence_score, snap, agg.signal, kline.timestamp_ms);
                    
                    if let Err(e) = store_clone.store_signal_async(&sym_clone, &out).await {
                        error!("{} store_signal error: {}", sym_clone, e);
                    }
                    
                    if count % 100 == 0 {
                        info!("{} processed {} klines, signal: {} c={}", sym_clone, count, action(&out.trade_advice), out.confluence_score);
                    }
                }
            }
        });
    }
    
    // Wait for Ctrl+C
    tokio::signal::ctrl_c().await?;
    info!("Shutdown");
    Ok(())
}

fn load_config() -> Result<MuscleConfig> {
    let mut cfg = MuscleConfig::default();
    if let Ok(c) = std::fs::read_to_string("muscle-config.toml") {
        cfg = toml::from_str(&c)?;
    }
    Ok(cfg)
}

fn indicators(df: &mut polars::prelude::DataFrame) -> Result<IndicatorSnapshot> {
    use polars::prelude::*;
    
    let close: Vec<f64> = df.column("close")?.f64()?.into_no_null_iter().collect();
    let high: Vec<f64> = df.column("high")?.f64()?.into_no_null_iter().collect();
    let low: Vec<f64> = df.column("low")?.f64()?.into_no_null_iter().collect();
    
    let n = close.len();
    if n == 0 {
        return Ok(IndicatorSnapshot::default());
    }
    
    // Calculate Ultimate Smoother - full series
    let us_series = compute_ultimate_smoother_default(&close);
    let us = us_series.last().copied().unwrap_or(0.0);
    
    // Calculate RSI (14-period) - full series
    let rsi_series = calculate_rsi_series(&close, 14);
    let rsi = rsi_series.last().copied().unwrap_or(50.0);
    
    // Calculate Bollinger Bands (20-period, 2 std) - full series
    let (bb_upper_series, bb_lower_series, bb_width_series) = calculate_bollinger_bands_series(&close, 20, 2.0);
    let bb_upper = bb_upper_series.last().copied().unwrap_or(0.0);
    let bb_lower = bb_lower_series.last().copied().unwrap_or(0.0);
    let bb_width = bb_width_series.last().copied().unwrap_or(0.0);
    
    // Calculate ADX (14-period) - simplified for latest value
    let adx = calculate_adx(&high, &low, &close, 14);
    
    // Calculate MACD (12, 26, 9) - simplified for latest value
    let (macd, macd_signal, macd_histogram) = calculate_macd(&close, 12, 26, 9);
    
    // Calculate ATR (14-period) - simplified for latest value
    let atr = calculate_atr(&high, &low, &close, 14);
    
    // Add columns to DataFrame if they don't exist, or update them
    // Helper to add or update f64 column
    fn add_or_update_column(df: &mut DataFrame, name: &str, values: Vec<f64>) -> Result<()> {
        // Check if column exists
        if df.column(name).is_ok() {
            // Drop and re-add
            let _ = df.drop(name);
        }
        let series = Series::from_vec(name.into(), values);
        df.with_column(series)?;
        Ok(())
    }
    
    // Add US series column
    add_or_update_column(df, "us_value", us_series)?;
    
    // Add BB columns
    add_or_update_column(df, "bb_upper", bb_upper_series)?;
    add_or_update_column(df, "bb_lower", bb_lower_series)?;
    
    Ok(IndicatorSnapshot {
        rsi,
        bb_upper,
        bb_lower,
        us_value: us,
        adx,
        bb_width,
        atr,
        macd,
        macd_signal,
        macd_histogram,
    })
}

/// Calculate RSI series (all values, not just latest)
fn calculate_rsi_series(close: &[f64], period: usize) -> Vec<f64> {
    let n = close.len();
    let mut rsi = vec![50.0; n];
    
    if n < period + 1 {
        return rsi;
    }
    
    let mut gains = Vec::with_capacity(n);
    let mut losses = Vec::with_capacity(n);
    
    gains.push(0.0);
    losses.push(0.0);
    
    for i in 1..n {
        let change = close[i] - close[i - 1];
        if change > 0.0 {
            gains.push(change);
            losses.push(0.0);
        } else {
            gains.push(0.0);
            losses.push(-change);
        }
    }
    
    // First average gain/loss (simple average)
    let mut avg_gain: f64 = gains[..=period].iter().sum::<f64>() / period as f64;
    let mut avg_loss: f64 = losses[..=period].iter().sum::<f64>() / period as f64;
    
    // First RSI
    for i in 0..=period {
        rsi[i] = 50.0;
    }
    
    // Calculate RSI using Wilder's smoothing
    for i in (period + 1)..n {
        avg_gain = (avg_gain * (period - 1) as f64 + gains[i]) / period as f64;
        avg_loss = (avg_loss * (period - 1) as f64 + losses[i]) / period as f64;
        
        let rs = if avg_loss.abs() > 1e-10 {
            avg_gain / avg_loss
        } else {
            100.0
        };
        
        rsi[i] = 100.0 - (100.0 / (1.0 + rs));
    }
    
    rsi
}

/// Calculate Bollinger Bands series (all values, not just latest)
fn calculate_bollinger_bands_series(close: &[f64], period: usize, std_mult: f64) -> (Vec<f64>, Vec<f64>, Vec<f64>) {
    let n = close.len();
    let mut bb_upper = vec![0.0; n];
    let mut bb_lower = vec![0.0; n];
    let mut bb_width = vec![0.0; n];
    
    if n < period {
        return (bb_upper, bb_lower, bb_width);
    }
    
    for i in (period - 1)..n {
        let slice = &close[i - period + 1..=i];
        let sma: f64 = slice.iter().sum::<f64>() / period as f64;
        
        let variance: f64 = slice.iter()
            .map(|x| (x - sma).powi(2))
            .sum::<f64>() / period as f64;
        let std_dev = variance.sqrt();
        
        bb_upper[i] = sma + std_mult * std_dev;
        bb_lower[i] = sma - std_mult * std_dev;
        bb_width[i] = if sma != 0.0 { (bb_upper[i] - bb_lower[i]) / sma } else { 0.0 };
    }
    
    (bb_upper, bb_lower, bb_width)
}

fn calculate_rsi(close: &[f64], period: usize) -> f64 {
    if close.len() < period + 1 { return 50.0; }
    
    let mut gains = 0.0;
    let mut losses = 0.0;
    
    for i in (close.len() - period)..close.len() {
        let change = close[i] - close[i - 1];
        if change > 0.0 {
            gains += change;
        } else {
            losses -= change;
        }
    }
    
    let avg_gain = gains / period as f64;
    let avg_loss = losses / period as f64;
    
    if avg_loss == 0.0 { return 100.0; }
    let rs = avg_gain / avg_loss;
    100.0 - (100.0 / (1.0 + rs))
}

fn calculate_bollinger_bands(close: &[f64], period: usize, std_mult: f64) -> (f64, f64, f64) {
    if close.len() < period { return (0.0, 0.0, 0.0); }
    
    let slice = &close[close.len() - period..];
    let sma: f64 = slice.iter().sum::<f64>() / period as f64;
    
    let variance: f64 = slice.iter()
        .map(|x| (x - sma).powi(2))
        .sum::<f64>() / period as f64;
    let std_dev = variance.sqrt();
    
    let bb_upper = sma + std_mult * std_dev;
    let bb_lower = sma - std_mult * std_dev;
    let bb_width = (bb_upper - bb_lower) / sma;
    
    (bb_upper, bb_lower, bb_width)
}

fn calculate_adx(high: &[f64], low: &[f64], close: &[f64], period: usize) -> f64 {
    if close.len() < period + 1 { return 25.0; }
    let max_high = high.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let min_low = low.iter().cloned().fold(f64::INFINITY, f64::min);
    let price_range = max_high - min_low;
    let avg_price = close.iter().sum::<f64>() / close.len() as f64;
    if avg_price == 0.0 { return 25.0; }
    ((price_range / avg_price) * 100.0).min(100.0).max(0.0)
}

fn calculate_macd(close: &[f64], fast: usize, slow: usize, signal_period: usize) -> (f64, f64, f64) {
    if close.len() < slow + signal_period { return (0.0, 0.0, 0.0); }
    
    let ema_fast = calculate_ema(close, fast);
    let ema_slow = calculate_ema(close, slow);
    let macd_line = ema_fast - ema_slow;
    
    let signal_line = macd_line * 0.9;
    let histogram = macd_line - signal_line;
    
    (macd_line, signal_line, histogram)
}

fn calculate_ema(close: &[f64], period: usize) -> f64 {
    if close.is_empty() { return 0.0; }
    
    let multiplier = 2.0 / (period as f64 + 1.0);
    let mut ema = close[0];
    
    for &price in close.iter().skip(1) {
        ema = (price - ema) * multiplier + ema;
    }
    
    ema
}

fn calculate_atr(high: &[f64], low: &[f64], close: &[f64], period: usize) -> f64 {
    if close.len() < period + 1 { return 0.0; }
    
    let mut tr_sum = 0.0;
    for i in (close.len() - period)..close.len() {
        let tr = (high[i] - low[i]).max((high[i] - close[i-1]).abs()).max((low[i] - close[i-1]).abs());
        tr_sum += tr;
    }
    
    tr_sum / period as f64
}

fn regime(s: &IndicatorSnapshot) -> MarketRegime {
    if s.adx > 25.0 { MarketRegime::Trending }
    else if s.bb_width > 0.10 { MarketRegime::Volatile }
    else if s.adx < 20.0 { MarketRegime::Ranging }
    else { MarketRegime::Volatile }
}

fn action(s: &Signal) -> &'static str {
    match s { Signal::Long{..} => "LONG", Signal::Short{..} => "SHORT", _ => "WAIT" }
}
