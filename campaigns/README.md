# Muscle-Kit Campaigns

Campaign-based trading with multi-symbol support.

## Quick Start

### Run Campaign Mode

```bash
# Run with campaign config
cargo run --release -p zeroclaw-muscle-kit --bin muscle-campaign -- \
  --campaign campaigns/crypto-momentum.toml

# Run in legacy mode (direct symbols)
cargo run --release -p zeroclaw-muscle-kit --bin muscle-campaign -- \
  --symbols BTCUSDT,ETHUSDT,SOLUSDT \
  --interval 1m
```

## Available Campaigns

### 1. Crypto Momentum (`crypto-momentum.toml`)

Multi-symbol trend following strategy for BTC, ETH, and SOL.

**Symbols:** BTCUSDT, ETHUSDT, SOLUSDT  
**Intervals:** 1m (BTC, ETH), 5m (SOL)  
**Strategy:** Trend Following (70% weight)  
**Risk:** $10,000 total, $3,000 per symbol

### 2. Create Your Own Campaign

```toml
# campaigns/my-campaign.toml
[campaign]
name = "my-campaign"
description = "My custom strategy"
enabled = true

# Multiple symbols
symbols = ["BTCUSDT", "ETHUSDT"]

# Different intervals per symbol
symbol_intervals = { "BTCUSDT" = "1m", "ETHUSDT" = "5m" }
default_interval = "1m"

[strategies]
trend_following = true
trend_following_weight = 0.7
mean_reversion = false

[risk]
max_total_position_usd = 5000.0
max_per_symbol_usd = 2500.0
max_daily_trades = 20
max_daily_trades_per_symbol = 10
emergency_pnl_threshold = -5.0

[correlation]
enabled = false
max_correlation_threshold = 0.8
max_correlated_symbols = 2
```

## Campaign Configuration Reference

### `[campaign]` Section

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `name` | string | - | Campaign name (unique) |
| `description` | string? | null | Campaign description |
| `enabled` | bool | true | Enable/disable campaign |
| `symbols` | string[] | - | List of symbols to trade |
| `symbol_intervals` | object? | {} | Symbol-specific intervals |
| `default_interval` | string | "1m" | Default interval |

### `[strategies]` Section

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `trend_following` | bool | true | Enable trend following |
| `trend_following_weight` | float | 0.5 | Trend following weight |
| `mean_reversion` | bool | false | Enable mean reversion |
| `mean_reversion_weight` | float | 0.0 | Mean reversion weight |

### `[risk]` Section

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `max_total_position_usd` | float | 10000.0 | Max total position (all symbols) |
| `max_per_symbol_usd` | float | 2500.0 | Max position per symbol |
| `max_daily_trades` | int | 20 | Max daily trades (all symbols) |
| `max_daily_trades_per_symbol` | int | 5 | Max daily trades per symbol |
| `emergency_pnl_threshold` | float | -5.0 | Emergency PnL threshold (%) |
| `max_single_symbol_ratio` | float | 0.5 | Max single symbol risk ratio |

### `[correlation]` Section

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `enabled` | bool | false | Enable correlation checking |
| `max_correlation_threshold` | float | 0.8 | Max correlation threshold |
| `max_correlated_symbols` | int | 2 | Max correlated symbols to trade |

## Multi-Symbol Benefits

### 1. Diversification
Spread risk across multiple symbols instead of concentrating on one.

### 2. Shared Risk Management
Single risk limit for all symbols in the campaign.

### 3. Correlation Monitoring
Avoid over-exposure to highly correlated assets.

### 4. Efficient Resource Usage
Shared WebSocket connections and DataFrame managers.

## Example Use Cases

### Use Case 1: Blue Chip Crypto
```toml
symbols = ["BTCUSDT", "ETHUSDT"]
max_total_position_usd = 10000.0
max_per_symbol_usd = 5000.0
```

### Use Case 2: Altcoin Portfolio
```toml
symbols = ["SOLUSDT", "BNBUSDT", "ADAUSDT", "DOTUSDT"]
max_total_position_usd = 8000.0
max_per_symbol_usd = 2000.0
```

### Use Case 3: Sector Rotation
```toml
# DeFi tokens
symbols = ["UNIUSDT", "AAVEUSDT", "MKRUSDT"]
max_total_position_usd = 6000.0
```

## Monitoring

### Check Campaign Status

```bash
# Check Redis for signals
redis-cli KEYS 'market:signal:*'

# Check specific symbol signal
redis-cli GET market:signal:BTCUSDT | jq
```

### View Logs

```bash
# Run with verbose logging
RUST_LOG=info cargo run --release -p zeroclaw-muscle-kit --bin muscle-campaign -- \
  --campaign campaigns/crypto-momentum.toml
```

## Troubleshooting

### Campaign Not Starting

1. Check config file syntax: `toml lint campaigns/my-campaign.toml`
2. Verify campaign name is unique
3. Check Redis connection

### Symbols Not Trading

1. Verify symbols are in config
2. Check WebSocket connection
3. Verify bootstrap completed successfully

### Risk Limits Hit

1. Check daily trade count: `redis-cli GET campaign:my-campaign:daily_trades`
2. Review risk configuration
3. Wait for daily reset or increase limits

## Best Practices

1. **Start Small**: Begin with 2-3 symbols
2. **Monitor Correlation**: Avoid highly correlated symbols
3. **Set Conservative Limits**: Start with lower risk limits
4. **Test on Testnet**: Always test new campaigns on Binance testnet
5. **Monitor Performance**: Track PnL per symbol and total

## Advanced Features (Coming Soon)

- [ ] Dynamic symbol addition/removal
- [ ] Real-time correlation calculation
- [ ] Automatic rebalancing
- [ ] Performance analytics
- [ ] CLI campaign management commands
