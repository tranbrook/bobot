# Hướng Dẫn Sử Dụng Hệ Thống ZeroClaw Trading

## 📋 Mục Lục

1. [Tổng Quan](#tổng-quan)
2. [Yêu Cầu Hệ Thống](#yêu-cầu-hệ-thống)
3. [Cài Đặt](#cài-đặt)
4. [Cấu Hình](#cấu-hình)
5. [Khởi Động Hệ Thống](#khởi-động-hệ-thống)
6. [Giám Sát](#giám-sát)
7. [Xử Lý Sự Cố](#xử-lý-sự-cố)

---

## 🎯 Tổng Quan

Hệ thống ZeroClaw Trading bao gồm 3 components chính:

```
┌─────────────────┐     ┌─────────────────┐     ┌─────────────────┐
│   Muscle-Kit    │────▶│    ZeroClaw     │────▶│ Execution-Kit   │
│  (Signal Gen)   │Redis│  (AI Decision)  │Redis│  (Execution)    │
└─────────────────┘     └─────────────────┘     └─────────────────┘
```

1. **Muscle-Kit**: Tạo tín hiệu trading từ Binance data
2. **ZeroClaw**: AI decision making với LLM
3. **Execution-Kit**: Thực thi lệnh trên Binance

---

## 💻 Yêu Cầu Hệ Thống

### Tối Thiểu
- CPU: 4 cores
- RAM: 8GB
- Storage: 50GB SSD
- Network: 10Mbps+

### Khuyến Nghị
- CPU: 8 cores
- RAM: 16GB
- Storage: 100GB NVMe SSD
- Network: 50Mbps+

### Phần Mềm
- Rust 1.87+
- Redis 7.0+
- (Optional) Docker 24.0+

---

## 📦 Cài Đặt

### Bước 1: Cài Đặt Redis

```bash
# Ubuntu/Debian
sudo apt update
sudo apt install -y redis-server
sudo systemctl start redis
sudo systemctl enable redis

# Kiểm tra
redis-cli ping
# Output: PONG
```

### Bước 2: Build ZeroClaw

```bash
# Clone repository
cd ~
git clone https://github.com/zeroclaw-labs/zeroclaw.git
cd zeroclaw

# Build tất cả components
cargo build --release
```

### Bước 3: Tạo File Cấu Hình

```bash
# Tạo thư mục config
mkdir -p ~/.zeroclaw

# Tạo file config chính
cat > ~/.zeroclaw/config.toml << 'EOF'
# ZeroClaw Trading Configuration

[provider]
default_provider = "anthropic"
default_model = "claude-sonnet-4-20250514"
api_key = "your-anthropic-api-key"

[trading]
enabled = true
symbols = ["BTCUSDT", "ETHUSDT"]
check_interval_ms = 5000
redis_url = "redis://127.0.0.1:6379"

[trading.risk]
max_leverage = 5
max_risk_per_trade = 0.02
max_daily_trades = 15
max_drawdown_percent = 3.0
min_confluence_score = 6

[memory]
backend = "sqlite"
auto_save = true

[gateway]
port = 42617
host = "127.0.0.1"
EOF
```

---

## ⚙️ Cấu Hình Chi Tiết

### Muscle-Kit Config

```bash
cat > ~/zeroclaw/muscle-config.toml << 'EOF'
# Muscle-Kit Configuration

[general]
enabled = true
redis_url = "redis://127.0.0.1:6379"

[symbols]
pairs = ["BTCUSDT", "ETHUSDT"]
intervals = ["1m"]

[strategy]
mean_reversion_weight = 0.5
trend_following_weight = 0.5

[smoother]
c1 = 0.07
c2 = 0.05
c3 = 0.03

[performance]
max_allowed_latency_ms = 500
warmup_candles = 100
total_candles = 1100

[binance]
testnet = true
EOF
```

### Execution-Kit Config

```bash
cat > ~/zeroclaw/execution-config.toml << 'EOF'
# Execution-Kit Configuration

redis_url = "redis://127.0.0.1:6379"
symbols = ["BTCUSDT", "ETHUSDT"]

[binance]
testnet = true
api_key = "your-testnet-api-key"
api_secret = "your-testnet-api-secret"

[risk]
max_position_size_usd = 1000.0
max_daily_trades = 20
emergency_pnl_threshold = -5.0
EOF
```

---

## 🚀 Khởi Động Hệ Thống

### Terminal 1: Start Muscle-Kit

```bash
cd ~/zeroclaw
cargo run --release -p zeroclaw-muscle-kit -- \
  --config muscle-config.toml
```

**Output:**
```
🏋️  Muscle-Kit Real-Time Signal Engine Starting...
Config loaded: Redis=redis://127.0.0.1:6379, Symbols=["BTCUSDT"]
✅ Redis connected
📊 Bootstrapping...
✓ BTCUSDT bootstrapped (1100 candles)
🚀 Starting WebSocket streaming...
✓ BTCUSDT streaming
📈 Starting real-time signal generation...
```

### Terminal 2: Start ZeroClaw

```bash
cd ~/zeroclaw
source ~/.zeroclaw/.env  # Load API keys
cargo run --release -- daemon
```

**Output:**
```
🤖 ZeroClaw Trading Daemon Starting...
✅ Redis connected
✅ Trading module initialized
📈 Monitoring signals...
```

### Terminal 3: Start Execution-Kit

```bash
cd ~/zeroclaw
cargo run --release -p zeroclaw-execution-kit -- \
  --config execution-config.toml \
  --testnet
```

**Output:**
```
⚡ Execution-Kit Order Engine Starting...
Config loaded: Redis=redis://127.0.0.1:6379, Symbols=["BTCUSDT"]
✅ Redis connected
🚀 Starting order execution loop...
```

---

## 📊 Giám Sát

### Kiểm Tra Redis Signals

```bash
# Xem signal hiện tại
redis-cli GET market:signal:BTCUSDT | jq

# Output:
{
  "price": 74000.50,
  "market_regime": "TRENDING",
  "confluence_score": 7,
  "trade_advice": {"Long": {"strength": 0.8}},
  "ingestion_latency_ms": 45
}
```

### Kiểm Tra Orders Queue

```bash
# Xem độ dài queue
redis-cli LLEN execution:queue:BTCUSDT

# Xem lệnh đang chờ
redis-cli LRANGE execution:queue:BTCUSDT 0 -1 | jq
```

### Kiểm Tra Positions

```bash
# Xem position hiện tại
redis-cli GET market:position:BTCUSDT | jq
```

### Logs

```bash
# Muscle-Kit logs
tail -f ~/zeroclaw/logs/muscle-kit.log

# ZeroClaw logs
tail -f ~/zeroclaw/logs/zeroclaw.log

# Execution-Kit logs
tail -f ~/zeroclaw/logs/execution-kit.log
```

---

## 🔧 Xử Lý Sự Cố

### 1. Redis Connection Failed

**Triệu chứng:**
```
Error: Connection refused (os error 111)
```

**Giải pháp:**
```bash
# Start Redis
sudo systemctl start redis

# Kiểm tra
redis-cli ping
```

### 2. Binance API Error

**Triệu chứng:**
```
Order failed: {"code":-2014,"msg":"API-key format invalid"}
```

**Giải pháp:**
- Kiểm tra API key trong config
- Đảm bảo API key có permission đúng
- Với testnet, dùng https://testnet.binance.vision

### 3. High Latency Warning

**Triệu chứng:**
```
WARN Signal latency too high: 600ms > 500ms
```

**Giải pháp:**
- Kiểm tra network connection
- Giảm số symbols theo dõi
- Tăng `max_allowed_latency_ms` trong config

### 4. Daily Trade Limit Reached

**Triệu chứng:**
```
Error: Daily trade limit reached
```

**Giải pháp:**
- Đợi ngày mới (counter reset tự động)
- Hoặc tăng `max_daily_trades` trong config

---

## 🎛️ Commands Reference

### Muscle-Kit

```bash
# Run với config
cargo run --release -p zeroclaw-muscle-kit -- --config muscle-config.toml

# Run với custom Redis
cargo run --release -p zeroclaw-muscle-kit -- --redis-url redis://localhost:6379

# Run với symbols cụ thể
cargo run --release -p zeroclaw-muscle-kit -- --symbols BTCUSDT,ETHUSDT
```

### ZeroClaw

```bash
# Start daemon
cargo run --release -- daemon

# Start agent interactive
cargo run --release -- agent

# Check status
cargo run --release -- status
```

### Execution-Kit

```bash
# Run với testnet
cargo run --release -p zeroclaw-execution-kit -- --testnet

# Run với production
cargo run --release -p zeroclaw-execution-kit -- \
  --binance-api-key YOUR_KEY \
  --binance-api-secret YOUR_SECRET
```

---

## 📈 Best Practices

### 1. Bắt Đầu Với Testnet

```toml
# Luôn bắt đầu với testnet
[binance]
testnet = true
```

### 2. Cấu Hình Risk Conservative

```toml
[trading.risk]
max_leverage = 3          # Thấp cho an toàn
max_risk_per_trade = 0.01 # 1% risk
max_daily_trades = 5      # Giới hạn số lệnh
```

### 3. Monitoring

- Check logs thường xuyên
- Set up alerts cho PnL threshold
- Theo dõi Redis queue độ dài

### 4. Backup

```bash
# Backup Redis data
redis-cli SAVE

# Backup config
cp -r ~/.zeroclaw ~/.zeroclaw.backup
```

---

## 🎓 Ví Dụ End-to-End

### 1. Muscle-Kit Tạo Signal

```
Binance WebSocket → Muscle-Kit → Redis (market:signal:BTCUSDT)
```

### 2. ZeroClaw Phân Tích

```
Redis signal → ZeroClaw AI → Decision → Redis (execution:queue:BTCUSDT)
```

### 3. Execution-Kit Thực Thi

```
Redis queue → Execution-Kit → Binance API → Order Filled
```

### 4. Theo Dõi Kết Quả

```bash
# Check signal
redis-cli GET market:signal:BTCUSDT | jq

# Check queue
redis-cli LLEN execution:queue:BTCUSDT

# Check position
redis-cli GET market:position:BTCUSDT | jq
```

---

## 📞 Hỗ Trợ

- Documentation: `docs/`
- Issues: GitHub Issues
- Discord: [Link Discord]

---

*Lưu ý: Trading có rủi ro. Chỉ trade với số vốn bạn có thể chấp nhận mất.*
