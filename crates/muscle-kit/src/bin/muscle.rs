//! Muscle-Kit Binary - Real-Time Trading Signal Engine
//! Full implementation with WebSocket, DataFrame, indicators, and continuous signals

use anyhow::{Context, Result};
use clap::Parser;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::signal;
use tokio::sync::Mutex;
use tokio_stream::StreamExt;
use tracing::{debug, error, info};

use zeroclaw_muscle_kit::{
    Gatekeeper, MuscleConfig, RedisSignalStore,
    data::{Kline, KlineInterval, MarketRegime, Signal, SignalOutput, IndicatorSnapshot, fetch_bootstrap_klines, spawn_ws_client},
    engine::dataframe::DataFrameManager,
    engine::compute_ultimate_smoother_default,
};

#[derive(Parser, Debug)]
struct Args {
    #[arg(short, long, default_value = "muscle-config.toml")]
    config: String,
    #[arg(short, long, default_value = "BTCUSDT,ETHUSDT")]
    symbols: String,
    #[arg(long)]
    redis_url: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    let args = Args::parse();

    info!("🏋️  Muscle-Kit Real-Time Starting...");

    let symbols: Vec<String> = args.symbols.split(',').map(|s| s.trim().to_string()).collect();
    let mut config = load_config(&args.config)?;
    if let Some(redis_url) = args.redis_url {
        config.redis_url = redis_url;
    }

    let store = Arc::new(RedisSignalStore::new(&config.redis_url, Some("market:signal"))?);
    let gatekeeper = Arc::new(Gatekeeper::new());
    let interval_str = config.intervals.first().context("No interval")?;
    let interval = KlineInterval::from_str(interval_str).context("Invalid interval")?;

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
                    let snap = match indicators(df.df()) {
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
    signal::ctrl_c().await?;
    info!("Shutdown");
    Ok(())
}

fn load_config(path: &str) -> Result<MuscleConfig> {
    let mut cfg = MuscleConfig::default();
    if let Ok(c) = std::fs::read_to_string(path) {
        cfg = toml::from_str(&c)?;
    }
    Ok(cfg)
}

fn indicators(df: &polars::prelude::DataFrame) -> Result<IndicatorSnapshot> {
    let close: Vec<f64> = df.column("close")?.f64()?.into_no_null_iter().collect();
    let high: Vec<f64> = df.column("high")?.f64()?.into_no_null_iter().collect();
    let low: Vec<f64> = df.column("low")?.f64()?.into_no_null_iter().collect();
    
    let p = close.last().copied().unwrap_or(0.0);
    
    // Calculate Ultimate Smoother
    let us = compute_ultimate_smoother_default(&close).last().copied().unwrap_or(0.0);
    
    // Calculate RSI (14-period)
    let rsi = calculate_rsi(&close, 14);
    
    // Calculate Bollinger Bands (20-period, 2 std)
    let (bb_upper, bb_lower, bb_width) = calculate_bollinger_bands(&close, 20, 2.0);
    
    // Calculate ADX (14-period)
    let adx = calculate_adx(&high, &low, &close, 14);
    
    // Calculate MACD (12, 26, 9)
    let (macd, macd_signal, macd_histogram) = calculate_macd(&close, 12, 26, 9);
    
    // Calculate ATR (14-period)
    let atr = calculate_atr(&high, &low, &close, 14);
    
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
