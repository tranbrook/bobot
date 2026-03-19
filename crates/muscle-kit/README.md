# zeroclaw-muscle-kit

High-frequency trading signal engine with Polars.rs DataFrame processing.

## Overview

"The Muscle" is a high-frequency trading signal engine that:
- Fetches historical market data from Binance (REST API)
- Streams real-time klines via WebSocket
- Processes data using Polars.rs DataFrames
- Implements the Ultimate Smoother recursive digital filter
- Generates trading signals using multiple strategies
- Exports signals to Redis for downstream consumption

## Architecture

```
Data Ingestion → Polars DataFrame → Ultimate Smoother → Strategies → Gatekeeper → Redis
```

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
zeroclaw-muscle-kit = "0.1.0"
```

Or use the workspace version:

```toml
[dependencies]
zeroclaw-muscle-kit = { path = "crates/muscle-kit" }
```

## Quick Start

```rust
use zeroclaw_muscle_kit::{MuscleConfig, Gatekeeper, RedisSignalStore, BinanceRestClient};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Create configuration
    let config = MuscleConfig::default();
    
    // Create Gatekeeper for signal aggregation
    let gatekeeper = Gatekeeper::new();
    
    // Create Redis store for signal export
    let store = RedisSignalStore::new(&config.redis_url, Some("market:signal"))?;
    
    // Fetch historical data
    let client = BinanceRestClient::new()?;
    let klines = client.fetch_klines("BTCUSDT", KlineInterval::Minute1, Some(1100)).await?;
    
    // ... process and generate signals
    
    Ok(())
}
```

## Features

### Data Pipeline
- **Bootstrap**: Fetch 1100 historical klines (100 warmup + 1000 working)
- **Real-time Stream**: WebSocket subscription with auto-reconnect
- **Normalization**: Zero-null DataFrame with f64 types

### Technical Indicators
- **Ultimate Smoother**: Recursive digital filter with minimal lag
- **UltimateBands**: Bollinger Bands variant using US-smoothed std dev
- **UltimateChannel**: Keltner Channel variant using ATR

### Trading Strategies
- **Strategy A (Mean Reversion)**: UltimateBands exhaustion + RSI
- **Strategy B (Trend Following)**: US-smoothed EMA/MACD crossover

### Market Regime Detection
| Regime | Conditions |
|--------|------------|
| TRENDING | ADX > 25 |
| RANGING | ADX < 20 AND BB Width < 5% |
| VOLATILE | BB Width > 10% OR ADX rising rapidly |

### Signal Aggregation
- Weighted voting system
- Regime-based dynamic weights
- Confluence score (-10 to +10)
- Latency safety checks

## Configuration

```toml
[muscle]
enabled = true
pairs = ["BTCUSDT", "ETHUSDT"]
intervals = ["1m", "5m"]
redis_url = "redis://localhost:6379"

[muscle.strategy_weights]
mean_reversion = 0.5
trend_following = 0.5

[muscle.smoother]
c1 = 0.07
c2 = 0.05
c3 = 0.03

[muscle.performance]
max_allowed_latency_ms = 500
warmup_candles = 100
total_candles = 1100
```

## Redis Output

Signals are published to Redis hashes:

**Key**: `market:signal:{SYMBOL}`

**Value** (JSON):
```json
{
  "price": 95432.50,
  "timestamp_ms": 1710288000000,
  "market_regime": "TRENDING",
  "confluence_score": 7,
  "indicator_snapshot": {
    "rsi": 62.5,
    "bb_upper": 96000.0,
    "bb_lower": 94000.0,
    "us_value": 95200.0,
    "adx": 28.5,
    "bb_width": 0.042
  },
  "trade_advice": {"Long": {"strength": 0.8}},
  "ingestion_latency_ms": 45,
  "override_reason": null
}
```

## Performance

| Operation | Latency |
|-----------|---------|
| Ultimate Smoother (1000 candles) | < 1ms |
| DataFrame update | < 5ms |
| Signal aggregation | < 2ms |
| Redis store (async) | < 10ms |
| End-to-end (candle close → Redis) | < 50ms |

## Technical Optimizations

1. **Iterative Recursive Filter**: O(n) complexity, NOT using Polars LazyFrame
2. **Zero-Copy Data Extraction**: `.rechunk()` + `.cont_slice()` for Polars
3. **Warm-up Buffer**: 100 candles for indicator convergence
4. **Latency Monitoring**: Real-time tracking with safety triggers

## Testing

```bash
cargo test -p zeroclaw-muscle-kit
```

## License

MIT OR Apache-2.0

## Contributing

See the main ZeroClaw repository for contribution guidelines.
