# Hướng Dẫn Chạy Full Hệ Thống ZeroClaw Trading

## Tổng Quan Kiến Trúc

```
┌─────────────────────────────────────────────────────────────────────┐
│                     ZeroClaw Trading System                          │
│                     (Single Workspace)                               │
├─────────────────────────────────────────────────────────────────────┤
│                                                                      │
│  ┌─────────────────┐     ┌─────────────────┐     ┌────────────────┐ │
│  │  Muscle-Kit     │────▶│   ZeroClaw      │────▶│  Execution-Kit │ │
│  │  (crates/)      │Redis│   (Main App)    │Redis│  (crates/)     │ │
│  │                 │     │                 │     │                │ │
│  │ - Binance WS    │     │ - LLM Analysis  │     │ - Binance API  │ │
│  │ - Indicators    │     │ - Safety Check  │     │ - Order Mgmt   │ │
│  │ - Signal Gen    │     │ - Decision      │     │ - Position Mgmt│ │
│  └─────────────────┘     └─────────────────┘     └────────────────┘ │
│         │                       │                       │           │
│         ▼                       ▼                       ▼           │
│  ┌─────────────────────────────────────────────────────────────────┐│
│  │                         Redis Broker                             ││
│  │  - market:signal:{SYMBOL}    - execution:queue:{SYMBOL}         ││
│  │  - market:position:{SYMBOL}  - trading:stats                    ││
│  └─────────────────────────────────────────────────────────────────┘│
│                                                                      │
└─────────────────────────────────────────────────────────────────────┘
```

## Workspace Structure

```
/home/tranbrook/zero_os/          # Workspace root
├── Cargo.toml                     # Workspace definition
├── src/                           # Main ZeroClaw application
│   ├── agent/
│   │   └── trading/              # Trading integration (new)
│   ├── tools/
│   │   └── trading_decision.rs   # Trading tool
│   └── ...
├── crates/
│   ├── muscle-kit/               # Muscle-Kit crate
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── data/             # Binance data ingestion
│   │       ├── engine/           # Signal processing
│   │       ├── strategies/       # Trading strategies
│   │       └── mod.rs
│   ├── execution-kit/            # Execution-Kit crate
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── spot_client.rs    # Binance Spot API
│   │       ├── futures_client.rs # Binance Futures API
│   │       └── mod.rs
│   ├── brain/                    # Brain crate (optional)
│   └── common/                   # Shared types
└── docs/
    └── i18n/vi/
        └── full-system-setup-guide.md
```

---

## Phần 1: Chuẩn Bị

### 1.1. Yêu Cầu Hệ Thống

**Tối Thiểu:**
- CPU: 4 cores
- RAM: 8GB
- Storage: 50GB SSD
- Network: 10Mbps+

**Khuyến Nghị:**
- CPU: 8 cores
- RAM: 16GB
- Storage: 100GB NVMe SSD
- Network: 50Mbps+

### 1.2. Cài Đặt Dependencies

```bash
# Update system
sudo apt update && sudo apt upgrade -y

# Install required packages
sudo apt install -y \
    curl \
    git \
    wget \
    build-essential \
    pkg-config \
    libssl-dev \
    redis-server \
    jq

# Install Rust (if not installed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env

# Install Docker (optional)
curl -fsSL https://get.docker.com | sh
sudo usermod -aG docker $USER

# Verify installations
cd /home/tranbrook/zero_os
rustc --version
cargo --version
redis-cli --version
```

---

## Phần 2: Cài Đặt Redis

### 2.1. Cài Đặt Redis Server

```bash
# Install Redis
sudo apt install -y redis-server

# Configure Redis (optional)
sudo nano /etc/redis/redis.conf
```

**Redis Configuration:**
```conf
# Network
bind 127.0.0.1
port 6379
protected-mode yes

# Memory
maxmemory 512mb
maxmemory-policy allkeys-lru

# Persistence
appendonly yes
appendfsync everysec
```

### 2.2. Start Redis

```bash
# Start Redis
sudo systemctl start redis-server
sudo systemctl enable redis-server

# Check status
sudo systemctl status redis-server

# Test connection
redis-cli ping
# Output: PONG
```

### 2.3. Redis với Docker (Alternative)

```bash
# Run Redis container
docker run -d \
  --name redis-zeroclaw \
  -p 127.0.0.1:6379:6379 \
  -v ~/redis-data:/data \
  --restart unless-stopped \
  redis:7-alpine \
  redis-server --appendonly yes

# Test
docker exec -it redis-zeroclaw redis-cli ping
```

---

## Phần 3: Build Workspace

### 3.1. Build Toàn Bộ Workspace

```bash
cd /home/tranbrook/zero_os

# Build all crates
cargo build --release --workspace

# Or build specific crates
cargo build --release -p zeroclaw-muscle-kit
cargo build --release -p zeroclaw-execution-kit
cargo build --release -p zeroclaw
```

### 3.2. Test Build

```bash
# Test all crates
cargo test --release --workspace

# Test specific crate
cargo test --release -p zeroclaw-muscle-kit
```

---

## Phần 4: Cấu Hình

### 4.1. Tạo Thư Mục Config

```bash
# Create config directory
mkdir -p ~/.zeroclaw

# Create environment file
cat > ~/.zeroclaw/.env << 'EOF'
# AI Provider (for ZeroClaw)
ZEROCLAW_API_KEY="your-anthropic-api-key"

# Redis
REDIS_URL="redis://127.0.0.1:6379"

# Binance (for Muscle-Kit and Execution-Kit)
BINANCE_API_KEY="your-binance-api-key"
BINANCE_API_SECRET="your-binance-api-secret"
BINANCE_TESTNET=true

# Logging
RUST_LOG=info
EOF

# Load environment
source ~/.zeroclaw/.env
```

### 4.2. Cấu Hình Muscle-Kit

```bash
# Create Muscle-Kit config
cat > /home/tranbrook/zero_os/muscle-config.toml << 'EOF'
# Muscle-Kit Configuration

[general]
enabled = true
redis_url = "redis://127.0.0.1:6379"

[symbols]
pairs = ["BTCUSDT", "ETHUSDT", "SOLUSDT"]
intervals = ["1m", "5m"]

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
# Use testnet for testing
testnet = true
EOF
```

### 4.3. Cấu Hình Execution-Kit

```bash
# Create Execution-Kit config
cat > /home/tranbrook/zero_os/execution-config.toml << 'EOF'
# Execution-Kit Configuration

[general]
redis_url = "redis://127.0.0.1:6379"
symbols = ["BTCUSDT", "ETHUSDT"]

[binance]
testnet = true

[risk]
max_position_size_usd = 1000
max_daily_trades = 20
emergency_pnl_threshold = -5.0
EOF
```

### 4.4. Cấu Hình ZeroClaw

```bash
# Copy example config
cp /home/tranbrook/zero_os/dev/config.trading.example.toml ~/.zeroclaw/config.toml

# Edit config
nano ~/.zeroclaw/config.toml
```

**ZeroClaw Configuration:**
```toml
# ~/.zeroclaw/config.toml

[provider]
default_provider = "anthropic"
default_model = "claude-sonnet-4-20250514"

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
```

---

## Phần 5: Chạy Muscle-Kit

### 5.1. Chạy Muscle-Kit Binary

```bash
# Check if muscle-kit binary exists
ls -la /home/tranbrook/zero_os/target/release/ | grep muscle

# Run Muscle-Kit
cd /home/tranbrook/zero_os
source ~/.zeroclaw/.env

cargo run --release -p zeroclaw-muscle-kit -- \
  --config muscle-config.toml \
  --symbols BTCUSDT,ETHUSDT
```

### 5.2. Chạy Muscle-Kit Từ Main App

```bash
# Run ZeroClaw daemon with muscle feature
cd /home/tranbrook/zero_os
source ~/.zeroclaw/.env

cargo run --release --features muscle -- \
  daemon \
  --muscle-config muscle-config.toml
```

### 5.3. Kiểm Tra Muscle-Kit

```bash
# Check Redis for signals
redis-cli GET market:signal:BTCUSDT | jq

# Check signal freshness
watch -n 1 'redis-cli GET market:signal:BTCUSDT | jq .timestamp_ms'

# View logs
tail -f /home/tranbrook/zero_os/logs/muscle-kit.log
```

---

## Phần 6: Chạy ZeroClaw

### 6.1. Chạy ZeroClaw Agent

```bash
cd /home/tranbrook/zero_os
source ~/.zeroclaw/.env

# Interactive mode
cargo run --release -- agent

# Or with specific provider
cargo run --release -- agent \
  --provider anthropic \
  --model claude-sonnet-4-20250514
```

### 6.2. Chạy ZeroClaw Daemon

```bash
cd /home/tranbrook/zero_os
source ~/.zeroclaw/.env

# Daemon mode (background)
cargo run --release -- daemon

# Or with config
cargo run --release -- daemon \
  --config ~/.zeroclaw/config.toml
```

### 6.3. Kiểm Tra ZeroClaw

```bash
# Check status
cargo run --release -- status

# Check health
cargo run --release -- doctor

# View memory stats
cargo run --release -- memory stats

# Check Redis for decisions
redis-cli GET market:decision:BTCUSDT | jq
```

---

## Phần 7: Chạy Execution-Kit

### 7.1. Chạy Execution-Kit Binary

```bash
# Check if execution-kit binary exists
ls -la /home/tranbrook/zero_os/target/release/ | grep execution

# Run Execution-Kit
cd /home/tranbrook/zero_os
source ~/.zeroclaw/.env

cargo run --release -p zeroclaw-execution-kit -- \
  --config execution-config.toml \
  --symbols BTCUSDT,ETHUSDT
```

### 7.2. Chạy Execution-Kit Từ Main App

```bash
# Run ZeroClaw with execution feature
cd /home/tranbrook/zero_os
source ~/.zeroclaw/.env

cargo run --release --features execution -- \
  execution-engine \
  --config execution-config.toml
```

### 7.3. Kiểm Tra Execution-Kit

```bash
# Check Redis queue
redis-cli LLEN execution:queue:BTCUSDT

# View pending orders
redis-cli LRANGE execution:queue:BTCUSDT 0 -1 | jq

# Check positions
redis-cli GET market:position:BTCUSDT | jq

# View logs
tail -f /home/tranbrook/zero_os/logs/execution.log
```

---

## Phần 8: Chạy Full Hệ Thống

### 8.1. Script Khởi Động

```bash
#!/bin/bash
# /home/tranbrook/zero_os/start-system.sh

set -e

cd /home/tranbrook/zero_os

echo "🚀 Starting ZeroClaw Trading System..."

# Colors
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m'

# Check Redis
echo -e "${YELLOW}Checking Redis...${NC}"
if ! redis-cli ping > /dev/null 2>&1; then
    echo -e "${RED}Redis is not running! Starting Redis...${NC}"
    sudo systemctl start redis-server
fi
echo -e "${GREEN}✓ Redis is running${NC}"

# Create log directory
mkdir -p logs

# Load environment
source ~/.zeroclaw/.env

# Start Muscle-Kit
echo -e "${YELLOW}Starting Muscle-Kit...${NC}"
nohup cargo run --release -p zeroclaw-muscle-kit -- \
  --config muscle-config.toml \
  > logs/muscle-kit.log 2>&1 &
MUSCLE_PID=$!
echo -e "${GREEN}✓ Muscle-Kit started (PID: $MUSCLE_PID)${NC}"

# Wait for Muscle-Kit to warm up
echo "Waiting for Muscle-Kit to warm up (30 seconds)..."
sleep 30

# Start ZeroClaw
echo -e "${YELLOW}Starting ZeroClaw...${NC}"
nohup cargo run --release -- daemon \
  --config ~/.zeroclaw/config.toml \
  > logs/zeroclaw.log 2>&1 &
ZEROCLAW_PID=$!
echo -e "${GREEN}✓ ZeroClaw started (PID: $ZEROCLAW_PID)${NC}"

# Wait for ZeroClaw to initialize
sleep 10

# Start Execution-Kit
echo -e "${YELLOW}Starting Execution-Kit...${NC}"
nohup cargo run --release -p zeroclaw-execution-kit -- \
  --config execution-config.toml \
  > logs/execution.log 2>&1 &
EXECUTION_PID=$!
echo -e "${GREEN}✓ Execution-Kit started (PID: $EXECUTION_PID)${NC}"

# Save PIDs
echo "$MUSCLE_PID" > logs/muscle-kit.pid
echo "$ZEROCLAW_PID" > logs/zeroclaw.pid
echo "$EXECUTION_PID" > logs/execution-kit.pid

echo -e "${GREEN}================================${NC}"
echo -e "${GREEN}System Started Successfully!${NC}"
echo -e "${GREEN}================================${NC}"
echo ""
echo "PIDs:"
echo "  Muscle-Kit:     $MUSCLE_PID"
echo "  ZeroClaw:       $ZEROCLAW_PID"
echo "  Execution-Kit:  $EXECUTION_PID"
echo ""
echo "Logs:"
echo "  Muscle-Kit:     logs/muscle-kit.log"
echo "  ZeroClaw:       logs/zeroclaw.log"
echo "  Execution-Kit:  logs/execution.log"
echo ""
echo "To stop: ./stop-system.sh"
```

### 8.2. Script Dừng Hệ Thống

```bash
#!/bin/bash
# /home/tranbrook/zero_os/stop-system.sh

set -e

cd /home/tranbrook/zero_os

echo "🛑 Stopping ZeroClaw Trading System..."

# Read PIDs
if [ -f logs/muscle-kit.pid ]; then
    MUSCLE_PID=$(cat logs/muscle-kit.pid)
    echo "Stopping Muscle-Kit (PID: $MUSCLE_PID)..."
    kill $MUSCLE_PID 2>/dev/null || true
fi

if [ -f logs/zeroclaw.pid ]; then
    ZEROCLAW_PID=$(cat logs/zeroclaw.pid)
    echo "Stopping ZeroClaw (PID: $ZEROCLAW_PID)..."
    kill $ZEROCLAW_PID 2>/dev/null || true
fi

if [ -f logs/execution-kit.pid ]; then
    EXECUTION_PID=$(cat logs/execution-kit.pid)
    echo "Stopping Execution-Kit (PID: $EXECUTION_PID)..."
    kill $EXECUTION_PID 2>/dev/null || true
fi

# Wait for processes to stop
sleep 5

# Force kill if still running
for pid in $MUSCLE_PID $ZEROCLAW_PID $EXECUTION_PID; do
    if ps -p $pid > /dev/null 2>&1; then
        echo "Force killing PID: $pid"
        kill -9 $pid 2>/dev/null || true
    fi
done

# Cleanup PID files
rm -f logs/*.pid

echo "✓ System stopped"
```

### 8.3. Sử Dụng Scripts

```bash
# Make scripts executable
chmod +x /home/tranbrook/zero_os/start-system.sh
chmod +x /home/tranbrook/zero_os/stop-system.sh

# Start system
cd /home/tranbrook/zero_os
./start-system.sh

# Check status
ps aux | grep -E "(muscle|zeroclaw|execution)" | grep -v grep

# View logs
tail -f logs/*.log

# Stop system
./stop-system.sh
```

---

## Phần 9: Giám Sát

### 9.1. Dashboard Script

```bash
#!/bin/bash
# /home/tranbrook/zero_os/dashboard.sh

cd /home/tranbrook/zero_os

clear
echo "╔══════════════════════════════════════════════════════════╗"
echo "║         ZeroClaw Trading System Dashboard                ║"
echo "╚══════════════════════════════════════════════════════════╝"
echo ""

# Check processes
echo "📊 Process Status:"
for proc in muscle-kit zeroclaw execution-kit; do
    if pgrep -f $proc > /dev/null; then
        echo "  ✓ $proc: Running"
    else
        echo "  ✗ $proc: Stopped"
    fi
done

echo ""
echo "📈 Redis Stats:"
echo "  Signals: $(redis-cli KEYS 'market:signal:*' | wc -l)"
echo "  Positions: $(redis-cli KEYS 'market:position:*' | wc -l)"
echo "  Queue Length: $(redis-cli LLEN execution:queue:BTCUSDT)"

echo ""
echo "💰 Latest Signal (BTCUSDT):"
redis-cli GET market:signal:BTCUSDT 2>/dev/null | jq -r '
  "  Price: \(.price) | " +
  "Regime: \(.market_regime) | " +
  "Confluence: \(.confluence_score)/10 | " +
  "Advice: \(.trade_advice | keys[0])"
' 2>/dev/null || echo "  No signal available"

echo ""
echo "📜 Recent Logs:"
tail -5 logs/zeroclaw.log 2>/dev/null | sed 's/^/  /'

echo ""
echo "Press Ctrl+C to exit"
```

### 9.2. Monitoring Commands

```bash
cd /home/tranbrook/zero_os

# Watch signals
watch -n 2 'redis-cli GET market:signal:BTCUSDT | jq .confluence_score'

# Watch queue
watch -n 2 'redis-cli LLEN execution:queue:BTCUSDT'

# Watch positions
watch -n 5 'redis-cli GET market:position:BTCUSDT | jq .unrealized_pnl'

# Real-time logs
tail -f logs/*.log | grep -E "(EXECUTE|ERROR|ALERT)"
```

---

## Phần 10: Quick Reference

### Build Commands

```bash
cd /home/tranbrook/zero_os

# Build all
cargo build --release --workspace

# Build specific crates
cargo build --release -p zeroclaw-muscle-kit
cargo build --release -p zeroclaw-execution-kit
cargo build --release -p zeroclaw

# Test all
cargo test --release --workspace
```

### Run Commands

```bash
cd /home/tranbrook/zero_os
source ~/.zeroclaw/.env

# Run Muscle-Kit
cargo run --release -p zeroclaw-muscle-kit -- --config muscle-config.toml

# Run ZeroClaw
cargo run --release -- agent
cargo run --release -- daemon

# Run Execution-Kit
cargo run --release -p zeroclaw-execution-kit -- --config execution-config.toml
```

### Management Commands

```bash
cd /home/tranbrook/zero_os

# Start/Stop
./start-system.sh
./stop-system.sh

# Dashboard
./dashboard.sh

# Logs
tail -f logs/muscle-kit.log
tail -f logs/zeroclaw.log
tail -f logs/execution.log
```

### Redis Keys

```bash
# Signals
redis-cli GET market:signal:BTCUSDT

# Positions
redis-cli GET market:position:BTCUSDT

# Queue
redis-cli LLEN execution:queue:BTCUSDT
redis-cli LRANGE execution:queue:BTCUSDT 0 -1

# Stats
redis-cli GET trading:daily_count:2024-01-01
redis-cli GET trading:current_drawdown
```

---

## Troubleshooting

### Muscle-Kit Not Generating Signals

```bash
cd /home/tranbrook/zero_os

# Check logs
tail -100 logs/muscle-kit.log | grep -i error

# Check Binance connection
curl -X GET "https://testnet.binance.vision/api/v3/ping"

# Restart Muscle-Kit
pkill -f muscle-kit
./start-system.sh
```

### ZeroClaw Not Making Decisions

```bash
cd /home/tranbrook/zero_os

# Check API key
echo $ZEROCLAW_API_KEY

# Check Redis connection
redis-cli ping

# Check logs
tail -100 logs/zeroclaw.log | grep -i error

# Restart ZeroClaw
pkill -f zeroclaw
./start-system.sh
```

### Execution-Kit Not Executing

```bash
cd /home/tranbrook/zero_os

# Check queue
redis-cli LRANGE execution:queue:BTCUSDT 0 -1

# Check Binance API
curl -X GET "https://testnet.binance.vision/fapi/v1/account" \
  -H "X-MBX-APIKEY: $BINANCE_API_KEY"

# Check logs
tail -100 logs/execution.log | grep -i error

# Restart Execution-Kit
pkill -f execution-kit
./start-system.sh
```

---

*Hướng dẫn này được cập nhật cho ZeroClaw v0.1.9 với Muscle-Kit và Execution-Kit là crates trong workspace*
