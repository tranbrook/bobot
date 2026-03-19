# Hướng Dẫn Sử Dụng ZeroClaw Trading System

## Tổng Quan

ZeroClaw Trading System là hệ thống giao dịch tự động bao gồm 3 thành phần chính:

1. **Muscle-Kit** - Engine tạo tín hiệu giao dịch
2. **Brain** - Lớp ra quyết định AI
3. **Execution-Kit** - Engine thực thi lệnh trên Binance

```
Muscle-Kit → Redis → ZeroClaw (AI) → Redis → Execution-Kit → Binance
```

## Cài Đặt

### 1. Yêu Cầu Hệ Thống

- Rust 1.87+
- Redis server
- Binance API credentials

### 2. Cài Đặt Redis

```bash
# Docker
docker run -d -p 6379:6379 redis:latest

# Hoặc cài trực tiếp
sudo apt install redis-server
sudo systemctl start redis
```

### 3. Cấu Hình Environment

Tạo file `.env`:

```bash
# Binance Spot API
BINANCE_SPOT_API_KEY=your_spot_api_key
BINANCE_SPOT_SECRET_KEY=your_spot_secret_key

# Binance Futures API
BINANCE_FUTURES_API_KEY=your_futures_api_key
BINANCE_FUTURES_SECRET_KEY=your_futures_secret_key

# Redis
REDIS_URL=redis://localhost:6379
```

### 4. Build Project

```bash
# Build toàn bộ workspace
cargo build --workspace

# Test
cargo test -p zeroclaw-muscle-kit -p zeroclaw-execution-kit
```

## Sử Dụng

### 1. Chạy Muscle-Kit (Signal Generator)

Muscle-Kit tự động tạo tín hiệu và đẩy lên Redis:

```bash
cargo run -p zeroclaw-muscle-kit
```

**Output Redis:**
- Key: `market:signal:BTCUSDT`
- Format: JSON với price, regime, confluence_score, indicators

### 2. Chạy ZeroClaw (AI Decision)

```bash
cargo run -- agent
```

**Các tools có sẵn:**

#### get_market_signal
```
User: get_market_signal BTCUSDT
```

**Kết quả:**
```
📊 Market Signal for BTCUSDT

Price: $95432.50
Market Regime: TRENDING
Confluence Score: 7/10
Muscle Advice: LONG

Indicators:
  RSI: 62.5
  BB Width: 4.2%
  US Value: 95200.00
  MACD: 100.00
  ADX: 28.5

Latency: 45ms (OK)
```

#### dispatch_execution
```
User: dispatch_execution 
  symbol="BTCUSDT"
  side="BUY"
  leverage=5
  stop_loss=94000
  take_profit=98000
  risk_percent=0.02
```

**Kết quả:**
```
✅ Command dispatched to Execution Engine for BTCUSDT at 2026-03-14 15:16:19 UTC
Side: BUY | Leverage: 5x | SL: $94000.00 | TP: $98000.00 | Risk: 2.00%
```

### 3. Chạy Execution-Kit

```bash
cargo run -p zeroclaw-execution-kit
```

Execution-Kit sẽ:
1. Lắng nghe Redis queue `execution:queue:BTCUSDT`
2. Thực thi lệnh trên Binance (Spot/Futures)
3. Cập nhật position về Redis

## Redis Keys

| Key | Type | Mô Tả |
|-----|------|-------|
| `market:signal:{SYMBOL}` | String | Tín hiệu từ Muscle-Kit |
| `execution:queue:{SYMBOL}` | List | Queue lệnh thực thi |
| `market:position:{SYMBOL}` | String | Thông tin position |
| `market:account:balance` | String | Số dư tài khoản |

## Kiểm Tra Redis

```bash
# Check signal
redis-cli GET market:signal:BTCUSDT

# Check execution queue
redis-cli LRANGE execution:queue:BTCUSDT 0 -1

# Check queue length
redis-cli LLEN execution:queue:BTCUSDT

# Check position
redis-cli GET market:position:BTCUSDT
```

## Configuration

### Muscle-Kit Config

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

### Execution-Kit Config

```toml
[execution]
redis_url = "redis://localhost:6379"
default_leverage = 5
max_risk_per_trade = 0.02
post_only = true  # Maker fee optimization
```

## Safety Features

### 1. Latency Check
- Nếu latency > 500ms → Cảnh báo "⚠️ DATA STALE"
- Trade advice tự động thành WAIT

### 2. Risk Limits
- Leverage: 1-10x
- Risk per trade: 1-5%
- Stop loss và take profit bắt buộc

### 3. Emergency Close
```
User: dispatch_execution symbol="ALL" action="EMERGENCY_CLOSE"
```
→ Đóng tất cả positions ngay lập tức

## Troubleshooting

### Lỗi: "Failed to connect to Redis"
```bash
# Kiểm tra Redis đang chạy
redis-cli ping
# Expected: PONG

# Start Redis nếu chưa chạy
sudo systemctl start redis
```

### Lỗi: "BINANCE_API_KEY not set"
```bash
# Kiểm tra environment variables
echo $BINANCE_SPOT_API_KEY

# Export nếu chưa có
export BINANCE_SPOT_API_KEY=your_key
```

### Lỗi: "Order failed - Filter failure"
```bash
# Do price/quantity không đúng precision
# Execution-Kit tự động format, nhưng kiểm tra lại:
redis-cli GET market:signal:BTCUSDT
```

## Best Practices

1. **Test với Binance Testnet trước**
   - Spot: `https://testnet.binance.vision`
   - Futures: `https://testnet.binancefuture.com`

2. **Start với risk thấp**
   - Begin với risk_percent = 0.01 (1%)
   - Tăng dần khi tin tưởng hệ thống

3. **Monitor logs**
   ```bash
   # Xem logs real-time
   tail -f logs/execution-kit.log
   ```

4. **Backup API keys**
   - Không commit .env vào git
   - Sử dụng secret management tool

## Architecture Diagram

```
┌─────────────┐     ┌─────────┐     ┌──────────┐     ┌─────────┐     ┌─────────┐
│ Muscle-Kit  │────▶│  Redis  │────▶│ZeroClaw  │────▶│  Redis  │────▶│Execution│
│ (Signals)   │     │(signals)│     │  (AI)    │     │ (queue) │     │  -Kit   │
└─────────────┘     └─────────┘     └──────────┘     └─────────┘     └────┬────┘
                                                                          │
                                                                          ▼
                                                                   ┌──────────┐
                                                                   │  Binance │
                                                                   │(Spot/Fut)│
                                                                   └──────────┘
```

## Support

- Documentation: `/docs/reference/`
- Issues: GitHub Issues
- Community: Discord/Telegram

---

**Lưu ý:** Giao dịch cryptocurrency có rủi ro cao. Chỉ sử dụng số vốn bạn có thể chấp nhận mất.
