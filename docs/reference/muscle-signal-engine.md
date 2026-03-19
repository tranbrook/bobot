# Muscle Signal Engine Reference

## Overview

"The Muscle" is a high-frequency trading signal engine for ZeroClaw that implements a complete data pipeline from Binance, processes market data using Polars.rs DataFrames, and exports trading signals to Redis.

## Architecture

```
Data Ingestion → Polars DataFrame → Ultimate Smoother → Strategies → Gatekeeper → Redis
```

### Data Flow

1. **Bootstrap**: Fetch 1100 historical klines from Binance REST API (100 warmup + 1000 working)
2. **Real-time Stream**: Subscribe to Binance WebSocket for live kline updates
3. **Processing**: Calculate indicators using Polars DataFrames with zero-copy optimizations
4. **Strategy Analysis**: Multiple strategies analyze the data and produce signals
5. **Aggregation**: Gatekeeper combines signals using regime-based weighted voting
6. **Export**: Signals published to Redis as JSON

## Module Structure

```
src/muscle/
├── mod.rs                    # Module exports and constants
├── config.rs                 # Configuration schema
├── traits.rs                 # TradingStrategy trait
├── aggregator.rs             # Gatekeeper voting system
├── redis_store.rs            # Redis client
├── data/
│   ├── models.rs             # Kline, Signal, MarketRegime structs
│   ├── binance_rest.rs       # Historical klines fetch
│   └── binance_ws.rs         # WebSocket stream with auto-reconnect
├── engine/
│   ├── dataframe.rs          # Polars DataFrame manager
│   ├── smoother.rs           # Ultimate Smoother recursive filter
│   ├── bands.rs              # UltimateBands (BB variant)
│   └── channel.rs            # UltimateChannel (Keltner variant)
└── strategies/
    ├── regime.rs             # Market Regime Detector
    ├── mean_reversion.rs     # Strategy A
    └── trend_following.rs    # Strategy B
```

## Configuration

Enable the muscle module in your config:

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

## Technical Optimizations

### 1. High-Performance Recursive Filter

The Ultimate Smoother uses an iterative Rust loop (NOT Polars LazyFrame) for O(n) complexity:

```rust
pub fn compute_ultimate_smoother(
    input: &[f64],
    c1: f64,
    c2: f64,
    c3: f64,
) -> Vec<f64> {
    let n = input.len();
    let mut output = Vec::with_capacity(n);
    
    // Boundary conditions
    if n > 0 { output.push(input[0]); }
    if n > 1 { output.push(input[1]); }
    
    // Iterative calculation
    for i in 2..n {
        let us_n = (1.0 - c1) * input[i]
                 + (2.0 * c1 - c2) * input[i - 1]
                 - (c1 + c3) * input[i - 2]
                 + c2 * output[i - 1]
                 + c3 * output[i - 2];
        output.push(us_n);
    }
    
    output
}
```

### 2. Zero-Copy Data Extraction

Data is extracted from Polars Series using `.rechunk()` and `.cont_slice()`:

```rust
let rechunked = series.rechunk();
let chunked = rechunked.f64()?;
let data_slice: &[f64] = chunked.cont_slice()?;
```

### 3. Mathematical Convergence (Warm-up)

- Fetch 1100 candles from REST API
- First 100 candles used for indicator warm-up
- Signals only exported from candle 101 onwards

### 4. Latency Monitoring

- `ingestion_latency_ms` tracked for every signal
- Configurable `max_allowed_latency_ms` (default: 500ms)
- Exceeding latency forces `WAIT` with `LATENCY_SPIKE` reason

## Indicators

### Ultimate Smoother (US)

Recursive digital filter for minimal lag and noise:

```
US[n] = (1-c1)*D[n] + (2c1-c2)*D[n-1] - (c1+c3)*D[n-2] + c2*US[n-1] + c3*US[n-2]
```

Default coefficients: c1=0.07, c2=0.05, c3=0.03

### UltimateBands

Bollinger Bands variant based on US-smoothed standard deviation:

- Center = US(typical_price)
- Upper = Center + (multiplier × std_dev)
- Lower = Center - (multiplier × std_dev)

### UltimateChannel

Keltner Channel variant using US-smoothed ATR:

- Center = US(typical_price)
- Upper = Center + (multiplier × ATR)
- Lower = Center - (multiplier × ATR)

## Strategies

### Strategy A: Mean Reversion

**Entry Conditions (Long)**:
- Price touches/breaks below UltimateBands Lower
- RSI < 30 (oversold)
- Market Regime: RANGING (preferred) or VOLATILE

**Entry Conditions (Short)**:
- Price touches/breaks above UltimateBands Upper
- RSI > 70 (overbought)
- Market Regime: RANGING (preferred) or VOLATILE

**Regime Weights**: Ranging=0.7, Volatile=0.5, Trending=0.2

### Strategy B: Trend Following

**Entry Conditions (Long)**:
- US-smoothed price > EMA(20) > EMA(50)
- MACD histogram > 0 and rising
- Market Regime: TRENDING (preferred)

**Entry Conditions (Short)**:
- US-smoothed price < EMA(20) < EMA(50)
- MACD histogram < 0 and falling
- Market Regime: TRENDING (preferred)

**Regime Weights**: Trending=0.8, Volatile=0.5, Ranging=0.2

## Market Regime Detection

| Regime | Conditions |
|--------|------------|
| **TRENDING** | ADX > 25 |
| **RANGING** | ADX < 20 AND BB Width < 5% |
| **VOLATILE** | BB Width > 10% OR ADX rising rapidly |

## Gatekeeper Voting System

### Confluence Score Calculation

```
score = sum(signal_value × weight) × 10
```

Where:
- `signal_value`: Long=1, Short=-1, Wait=0
- `weight`: Strategy weight × regime adjustment × signal strength

### Signal Thresholds

| Confluence Score | Signal |
|------------------|--------|
| ≥ 7 | Strong LONG |
| 3 to 6 | Weak LONG |
| -2 to 2 | WAIT |
| -6 to -3 | Weak SHORT |
| ≤ -7 | Strong SHORT |

## Redis Output Format

### Key Format

```
market:signal:{SYMBOL}
```

Example: `market:signal:BTCUSDT`

### JSON Structure

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
    "bb_width": 0.042,
    "atr": 500.0,
    "macd": 10.0,
    "macd_signal": 8.0,
    "macd_histogram": 2.0
  },
  "trade_advice": {
    "Long": {
      "strength": 0.8
    }
  },
  "ingestion_latency_ms": 45,
  "override_reason": null
}
```

## API Reference

### Creating a Signal Store

```rust
use zeroclaw::muscle::{RedisSignalStore, Gatekeeper, MuscleConfig};

// Create Redis store
let store = RedisSignalStore::new("redis://localhost:6379", Some("market:signal"))?;

// Create Gatekeeper
let gatekeeper = Gatekeeper::new();

// Generate and store signal
let signal_output = gatekeeper.generate_signal_output(
    &df,
    &regime,
    price,
    timestamp_ms,
    kline_timestamp_ms,
    indicator_snapshot,
);

store.store_signal("BTCUSDT", &signal_output)?;
```

### Using the Config

```rust
use zeroclaw::muscle::MuscleConfig;

let config = MuscleConfig {
    enabled: true,
    pairs: vec!["BTCUSDT".to_string()],
    intervals: vec!["1m".to_string()],
    redis_url: "redis://localhost:6379".to_string(),
    ..Default::default()
};
```

## Testing

Run tests with the muscle feature flag:

```bash
cargo test --features muscle --lib muscle
```

Expected output: 100+ tests passing

## Feature Flag

The muscle module is gated behind the `muscle` feature:

```toml
[features]
muscle = ["dep:polars", "dep:polars-lazy", "dep:polars-core", "dep:polars-ops", "dep:redis"]
```

Enable with:

```bash
cargo build --features muscle
```

## Performance Benchmarks

| Operation | Latency |
|-----------|---------|
| Ultimate Smoother (1000 candles) | < 1ms |
| DataFrame update | < 5ms |
| Signal aggregation | < 2ms |
| Redis store (async) | < 10ms |
| End-to-end (candle close → Redis) | < 50ms |

## Troubleshooting

### High Latency Warnings

If you see `LATENCY_SPIKE` in `override_reason`:
1. Check Redis connectivity
2. Verify system clock synchronization
3. Reduce `max_allowed_latency_ms` threshold if needed

### Indicator Convergence Issues

If signals seem unstable at startup:
1. Increase `warmup_candles` (default: 100)
2. Ensure 1100 candles are fetched during bootstrap
3. Check for data gaps in historical data

### WebSocket Reconnection

If WebSocket disconnects frequently:
1. Check network stability
2. Verify Binance API status
3. Review reconnection logs for error patterns

## See Also

- [ZeroClaw Architecture](../assets/architecture-diagrams.md)
- [Configuration Reference](./config-reference.md)
- [Binance API Documentation](https://binance-docs.github.io/apidocs/)
