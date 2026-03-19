# 🧪 Campaign Testing Guide

## 🎯 Quick Start - Test với 200 Symbols

### Option 1: Test Với 200 Symbols (Full Test)

```bash
# Chạy campaign với 200 symbols
./target/release/muscle-campaign --campaign campaigns/top200-test.toml
```

**Thời gian bootstrap:** ~10 phút  
**RAM usage:** ~150 MB  
**Signals expected:** 50-200 signals/giờ

### Option 2: Test Với 50 Symbols (Fast Test) ⭐ RECOMMENDED

```bash
# Chạy campaign với 50 symbols - nhiều signals nhất
./target/release/muscle-campaign --campaign campaigns/top50-fast-test.toml
```

**Thời gian bootstrap:** ~2 phút  
**RAM usage:** ~40 MB  
**Signals expected:** 20-100 signals/giờ

## 📊 Expected Output

```
🏛️  Muscle-Kit Campaign Mode Starting...
✓ Campaign loaded: top50-fast-test
✓ Campaign started
📊 Campaign Mode: Multi-symbol processing
Campaign: top50-fast-test
Symbols: ["BTCUSDT", "ETHUSDT", ...] (50 symbols)

✓ BTCUSDT bootstrapped with 1000 candles (interval: 1m)
✓ ETHUSDT bootstrapped with 1000 candles (interval: 1m)
...
✓ SUSHIUSDT bootstrapped with 1000 candles (interval: 1m)

Starting real-time processing for 50 symbols...
✓ BTCUSDT streaming (interval: 1m)
✓ ETHUSDT streaming (interval: 1m)
...

# Real-time signal updates:
BTCUSDT processed 100 klines, signal: LONG c=7
ETHUSDT processed 150 klines, signal: WAIT c=2
SOLUSDT processed 120 klines, signal: SHORT c=-8
```

## 🔍 Monitoring Signals

### Check All Signals

```bash
# Xem tất cả signals trong Redis
redis-cli KEYS 'market:signal:*'

# Xem chi tiết signal của BTC
redis-cli GET market:signal:BTCUSDT | jq

# Output:
{
  "price": 74072.08,
  "market_regime": "RANGING",
  "confluence_score": 0,
  "trade_advice": {"direction": "Wait"},
  "ingestion_latency_ms": -13743
}
```

### Watch Real-Time Signals

```bash
# Watch signals updating in real-time
watch -n 5 "redis-cli KEYS 'market:signal:*' | wc -l"

# Xem signal mạnh (|c| >= 7)
redis-cli KEYS 'market:signal:*' | while read key; do
  score=$(redis-cli HGET $key confluence_score 2>/dev/null)
  if [ "$score" -ge 7 ] || [ "$score" -le -7 ]; then
    echo "$key: $score"
  fi
done
```

## 🎯 Tips Để Có Nhiều Signals

### 1. Dùng Interval Ngắn

```toml
# campaigns/my-test.toml
default_interval = "1m"  # 1 phút = nhiều signals hơn
```

### 2. Giảm Confluence Threshold

```toml
[signal_thresholds]
min_confluence_score = 2      # Giảm từ 7 xuống 2
strong_signal_threshold = 4   # Giảm từ 7 xuống 4
```

### 3. Chọn Coins Volatile

```toml
symbols = [
    "BTCUSDT", "ETHUSDT", "SOLUSDT",  # High volume
    "SHIBUSDT", "DOGEUSDT",           # High volatility
    "PEPEUSDT", "FLOKIUSDT"           # Meme coins = volatile
]
```

### 4. Enable Cả 2 Strategies

```toml
[strategies]
trend_following = true
trend_following_weight = 0.5
mean_reversion = true      # Mean reversion = nhiều signals hơn
mean_reversion_weight = 0.5
```

## 📈 Signal Frequency Estimates

| Campaign | Symbols | Interval | Signals/Hour | RAM |
|----------|---------|----------|--------------|-----|
| top50-fast-test | 50 | 1m | 20-100 | 40 MB |
| top200-test | 200 | 5m/1m | 50-200 | 150 MB |
| crypto-momentum | 3 | 1m/5m | 1-5 | 5 MB |

## ⚠️ Troubleshooting

### Problem: No Signals After 1 Hour

**Cause:** Market too quiet or thresholds too high

**Solution:**
```toml
[signal_thresholds]
min_confluence_score = 1  # Very low
```

### Problem: Too Many Signals (Spam)

**Cause:** Thresholds too low

**Solution:**
```toml
[signal_thresholds]
min_confluence_score = 5  # Higher
```

### Problem: High RAM Usage

**Cause:** Too many symbols with 1m interval

**Solution:**
```toml
default_interval = "5m"  # Use 5m instead of 1m
```

## 🚀 Advanced Testing

### Test với Custom Symbol List

```toml
# campaigns/my-custom.toml
name = "my-custom"
symbols = [
    "BTCUSDT",
    "ETHUSDT",
    "YOUR_SYMBOL_HERE"
]
default_interval = "1m"

[risk]
max_total_position_usd = 100.0
max_per_symbol_usd = 50.0
```

### Run Multiple Campaigns

```bash
# Terminal 1
./target/release/muscle-campaign --campaign campaigns/top50-fast-test.toml

# Terminal 2
./target/release/muscle-campaign --campaign campaigns/crypto-momentum.toml
```

## 📊 Performance Metrics

### Check System Resources

```bash
# RAM usage
ps aux | grep muscle-campaign | awk '{print $6}'

# CPU usage
top -p $(pgrep muscle-campaign)

# Network
iftop -Pn
```

### Redis Performance

```bash
# Redis memory
redis-cli INFO memory | grep used_memory_human

# Redis ops/sec
redis-cli INFO stats | grep ops_per_sec
```

## 🎉 Success Criteria

Test thành công khi:

- ✅ Tất cả symbols bootstrapped
- ✅ WebSocket connections established
- ✅ Signals được tạo vào Redis
- ✅ RAM usage < 200 MB (cho 200 symbols)
- ✅ CPU usage < 5%
- ✅ No errors in logs

## 📞 Need Help?

Check logs:
```bash
RUST_LOG=debug ./target/release/muscle-campaign --campaign campaigns/top50-fast-test.toml 2>&1 | tee test.log
```

---

**Happy Testing! 🚀**
