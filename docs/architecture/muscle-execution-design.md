# Muscle-Kit & Execution-Kit Binary Design

## Tổng Quan

Tài liệu này mô tả thiết kế chi tiết cho 3 binaries độc lập của hệ thống ZeroClaw Trading:

1. **Muscle-Kit** (`muscle`) - Signal Generation Engine
2. **ZeroClaw** (`zeroclaw`) - AI Decision Engine
3. **Execution-Kit** (`execution`) - Order Execution Engine

---

## Kiến Trúc Tổng Thể

```
┌─────────────────────────────────────────────────────────────────────┐
│                     ZeroClaw Trading System                          │
├─────────────────────────────────────────────────────────────────────┤
│                                                                      │
│  ┌─────────────────┐     ┌─────────────────┐     ┌────────────────┐ │
│  │   Muscle-Kit    │────▶│    ZeroClaw     │────▶│ Execution-Kit  │ │
│  │   (muscle)      │Redis│   (zeroclaw)    │Redis│  (execution)   │ │
│  │                 │     │                 │     │                │ │
│  │ - Binance WS    │     │ - LLM Analysis  │     │ - Binance API  │ │
│  │ - Polars DF     │     │ - Safety Check  │     │ - Order Mgmt   │ │
│  │ - Indicators    │     │ - Decision      │     │ - Position Mgmt│ │
│  │ - Signal Gen    │     │ - Risk Monitor  │     │ - PnL Tracking │ │
│  └─────────────────┘     └─────────────────┘     └────────────────┘ │
│         │                       │                       │           │
│         ▼                       ▼                       ▼           │
│  ┌─────────────────────────────────────────────────────────────────┐│
│  │                         Redis Broker                             ││
│  │                                                                  ││
│  │  Input Channels:                    Output Channels:             ││
│  │  - market:signal:{SYMBOL}           - execution:queue:{SYMBOL}   ││
│  │  - market:kline:{SYMBOL}            - market:position:{SYMBOL}   ││
│  │                                   - trading:stats               ││
│  └─────────────────────────────────────────────────────────────────┘│
│                                                                      │
└─────────────────────────────────────────────────────────────────────┘
```

---

## 1. Muscle-Kit Binary Design

### 1.1. Responsibilities

- Kết nối Binance WebSocket để nhận real-time klines
- Tính toán indicators sử dụng Polars DataFrame
- Phát hiện market regime
- Generate signals từ multiple strategies
- Aggregate signals với Gatekeeper
- Export signals to Redis

### 1.2. Architecture

```
┌──────────────────────────────────────────────────────────────────┐
│                      Muscle-Kit Binary                            │
├──────────────────────────────────────────────────────────────────┤
│                                                                   │
│  ┌─────────────────┐     ┌─────────────────┐     ┌─────────────┐ │
│  │  WebSocket      │────▶│  Data Processor │────▶│  Signal     │ │
│  │  Client         │     │  (Polars DF)    │     │  Generator  │ │
│  │                 │     │                 │     │             │ │
│  │ - Binance WS    │     │ - US Smoother   │     │ - Regime    │ │
│  │ - Kline Stream  │     │ - Bollinger     │     │ - Strategy  │ │
│  │ - Auto Reconnect│     │ - Channel       │     │ - Gatekeeper│ │
│  └─────────────────┘     │ - RSI, MACD     │     └──────┬──────┘ │
│                          │ - ADX, ATR      │            │        │
│                          └─────────────────┘            ▼        │
│                                                  ┌─────────────┐ │
│                                                  │  Redis      │ │
│                                                  │  Publisher  │ │
│                                                  │             │ │
│                                                  │ market:     │ │
│                                                  │ signal:{SYM}│ │
│                                                  └─────────────┘ │
└──────────────────────────────────────────────────────────────────┘
```

### 1.3. Components

#### 1.3.1. WebSocket Client

```rust
pub struct MuscleWebSocketClient {
    symbols: Vec<String>,
    intervals: Vec<String>,
    testnet: bool,
    reconnect_attempts: u32,
}

impl MuscleWebSocketClient {
    pub fn new(symbols: Vec<String>, intervals: Vec<String>, testnet: bool) -> Self;
    pub async fn start_stream(&self, tx: mpsc::Sender<Kline>) -> Result<()>;
    pub async fn reconnect(&self) -> Result<()>;
}
```

**Features:**
- Auto-reconnect với exponential backoff
- Multi-symbol, multi-interval support
- Heartbeat monitoring
- Error handling và recovery

#### 1.3.2. Data Processor (Polars DataFrame)

```rust
pub struct MuscleDataProcessor {
    warmup_candles: usize,
    working_candles: usize,
    dataframes: HashMap<String, DataFrame>,
}

impl MuscleDataProcessor {
    pub fn new(warmup: usize, working: usize) -> Self;
    pub fn process_kline(&mut self, kline: &Kline) -> Result<IndicatorSnapshot>;
    pub fn compute_indicators(&mut self, symbol: &str) -> Result<IndicatorSnapshot>;
}
```

**Indicators:**
- **Ultimate Smoother**: Recursive digital filter
- **UltimateBands**: Bollinger Bands variant
- **UltimateChannel**: Keltner Channel variant
- **RSI**: Relative Strength Index (14)
- **MACD**: Moving Average Convergence Divergence
- **ADX**: Average Directional Index
- **ATR**: Average True Range

#### 1.3.3. Signal Generator

```rust
pub struct MuscleSignalGenerator {
    gatekeeper: Gatekeeper,
    strategies: Vec<Box<dyn TradingStrategy>>,
}

impl MuscleSignalGenerator {
    pub fn new() -> Self;
    pub fn generate_signal(
        &self,
        symbol: &str,
        df: &DataFrame,
        regime: MarketRegime,
    ) -> Result<SignalOutput>;
}
```

**Strategies:**
- **MeanReversionStrategy**: RSI + Bollinger Bands
- **TrendFollowingStrategy**: MACD + ADX
- **RegimeDetector**: Market regime classification

### 1.4. Data Flow

```
1. WebSocket Client nhận kline từ Binance
   ↓
2. Data Processor cập nhật DataFrame
   ↓
3. Tính toán indicators (US, BB, RSI, MACD, ADX)
   ↓
4. Signal Generator tạo signal từ strategies
   ↓
5. Gatekeeper aggregate signals
   ↓
6. Redis Publisher export signal to Redis
```

### 1.5. Redis Keys

| Key | Type | Description |
|-----|------|-------------|
| `market:signal:{SYMBOL}` | String | Current signal (JSON) |
| `market:kline:{SYMBOL}` | List | Recent klines |
| `trading:signal_count:{DATE}` | Integer | Daily signal count |

### 1.6. CLI Interface

```bash
cargo run --release -p zeroclaw-muscle-kit -- \
  --config muscle-config.toml \
  --symbols BTCUSDT,ETHUSDT \
  --redis-url redis://localhost:6379 \
  --testnet
```

---

## 2. Execution-Kit Binary Design

### 2.1. Responsibilities

- Listen Redis execution queues
- Validate orders với risk checks
- Execute orders trên Binance (Spot/Futures)
- Track positions và PnL
- Update position status to Redis

### 2.2. Architecture

```
┌──────────────────────────────────────────────────────────────────┐
│                    Execution-Kit Binary                           │
├──────────────────────────────────────────────────────────────────┤
│                                                                   │
│  ┌─────────────────┐     ┌─────────────────┐     ┌─────────────┐ │
│  │  Redis          │────▶│  Order          │────▶│  Binance    │ │
│  │  Listener       │     │  Validator      │     │  Executor   │ │
│  │                 │     │                 │     │             │ │
│  │ - BRPOP queue   │     │ - Risk Check    │     │ - Spot API  │ │
│  │ - Multi-symbol  │     │ - Position Check│     │ - Futures   │ │
│  │ - Auto-reconnect│     │ - Balance Check │     │ - WebSocket │ │
│  └─────────────────┘     └─────────────────┘     └──────┬──────┘ │
│                                                         │        │
│                          ┌─────────────────┐            ▼        │
│                          │  Position       │◀──────────────┐    │
│                          │  Manager        │               │    │
│                          │                 │    Fill Updates │   │
│                          │ - Track PnL     │               │    │
│                          │ - Update Redis  │───────────────┘    │
│                          └─────────────────┘                     │
└──────────────────────────────────────────────────────────────────┘
```

### 2.3. Components

#### 2.3.1. Redis Listener

```rust
pub struct ExecutionRedisListener {
    redis: ConnectionManager,
    symbols: Vec<String>,
}

impl ExecutionRedisListener {
    pub fn new(redis: ConnectionManager, symbols: Vec<String>) -> Self;
    pub async fn listen(&mut self) -> Result<ExecutionCommand>;
    pub async fn listen_any(&mut self) -> Result<ExecutionCommand>;
}
```

**Features:**
- BRPOP với timeout
- Multi-symbol support
- Auto-reconnect
- Queue priority handling

#### 2.3.2. Order Validator

```rust
pub struct ExecutionOrderValidator {
    config: ExecutionRiskConfig,
}

impl ExecutionOrderValidator {
    pub fn new(config: ExecutionRiskConfig) -> Self;
    pub fn validate(&self, order: &ExecutionCommand, position: Option<&PositionInfo>) -> Result<()>;
}
```

**Validations:**
- Daily trade limit
- Position size limit
- Leverage limit
- Balance check
- Risk percentage check
- Duplicate order check

#### 2.3.3. Binance Executor

```rust
pub struct ExecutionBinanceExecutor {
    spot_client: SpotClient,
    futures_client: FuturesClient,
    testnet: bool,
}

impl ExecutionBinanceExecutor {
    pub fn new(api_key: String, api_secret: String, testnet: bool) -> Self;
    pub async fn execute(&self, order: &ExecutionCommand) -> Result<OrderResponse>;
    pub async fn close_position(&self, symbol: &str) -> Result<()>;
}
```

**Features:**
- Spot và Futures support
- Market và Limit orders
- Stop loss và Take profit
- Position close functionality
- WebSocket fill updates

#### 2.3.4. Position Manager

```rust
pub struct ExecutionPositionManager {
    redis: ConnectionManager,
    positions: HashMap<String, PositionInfo>,
}

impl ExecutionPositionManager {
    pub fn new(redis: ConnectionManager) -> Self;
    pub fn update_position(&mut self, symbol: &str, fill: &OrderFill) -> Result<()>;
    pub fn get_position(&self, symbol: &str) -> Option<&PositionInfo>;
    pub async fn publish_position(&self, symbol: &str) -> Result<()>;
}
```

**Features:**
- Real-time PnL calculation
- Position tracking
- Redis position updates
- Fill history

### 2.4. Data Flow

```
1. Redis Listener BRPOP execution:queue:{SYMBOL}
   ↓
2. Order Validator kiểm tra risk limits
   ↓
3. Binance Executor place order
   ↓
4. WebSocket nhận fill updates
   ↓
5. Position Manager cập nhật position
   ↓
6. Publish position status to Redis
```

### 2.5. Redis Keys

| Key | Type | Description |
|-----|------|-------------|
| `execution:queue:{SYMBOL}` | List | Order queue (LPUSH/BRPOP) |
| `market:position:{SYMBOL}` | String | Current position (JSON) |
| `trading:daily_count:{DATE}` | Integer | Daily trade count |
| `trading:current_drawdown` | Float | Current drawdown % |
| `trading:account_balance` | Float | Account balance |

### 2.6. CLI Interface

```bash
cargo run --release -p zeroclaw-execution-kit -- \
  --config execution-config.toml \
  --symbols BTCUSDT,ETHUSDT \
  --redis-url redis://localhost:6379 \
  --api-key YOUR_API_KEY \
  --api-secret YOUR_API_SECRET \
  --testnet
```

---

## 3. ZeroClaw Main Binary

### 3.1. Status

✅ **Đã hoàn thành** với Trading Integration

### 3.2. Existing Features

- TradingDecisionTool
- SafetyLayer (3 layers)
- TradingMemory (SQLite)
- TradingMonitor
- BacktestEngine

### 3.3. Redis Integration

```rust
// Read signal from Muscle-Kit
let signal = redis::cmd("GET")
    .arg("market:signal:BTCUSDT")
    .query_async(&mut conn)
    .await?;

// Push decision to Execution-Kit
redis::cmd("LPUSH")
    .arg("execution:queue:BTCUSDT")
    .arg(&command_json)
    .query_async(&mut conn)
    .await?;
```

---

## 4. Implementation Plan

### Phase 1: Muscle-Kit Binary (2-3 weeks)

**Week 1: WebSocket Client**
- [ ] Implement Binance WebSocket client
- [ ] Add auto-reconnect logic
- [ ] Multi-symbol, multi-interval support
- [ ] Kline parsing and validation

**Week 2: Data Processor**
- [ ] Polars DataFrame integration
- [ ] Ultimate Smoother implementation
- [ ] Bollinger Bands calculation
- [ ] RSI, MACD, ADX indicators

**Week 3: Signal Generator**
- [ ] Market regime detection
- [ ] Mean Reversion strategy
- [ ] Trend Following strategy
- [ ] Gatekeeper aggregation
- [ ] Redis signal export

### Phase 2: Execution-Kit Binary (2-3 weeks)

**Week 1: Redis Listener & Validator**
- [ ] Redis BRPOP listener
- [ ] Order validation logic
- [ ] Risk check implementation
- [ ] Position limit checks

**Week 2: Binance Executor**
- [ ] Spot client implementation
- [ ] Futures client implementation
- [ ] Order placement logic
- [ ] Fill handling

**Week 3: Position Manager**
- [ ] Position tracking
- [ ] PnL calculation
- [ ] Redis position updates
- [ ] Emergency close functionality

### Phase 3: Integration Testing (1 week)

- [ ] End-to-end testing
- [ ] Performance benchmarking
- [ ] Error handling testing
- [ ] Failover testing

---

## 5. Configuration Examples

### Muscle-Kit Config (`muscle-config.toml`)

```toml
[general]
redis_url = "redis://127.0.0.1:6379"
testnet = true

[symbols]
pairs = ["BTCUSDT", "ETHUSDT"]
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
```

### Execution-Kit Config (`execution-config.toml`)

```toml
[general]
redis_url = "redis://127.0.0.1:6379"
symbols = ["BTCUSDT", "ETHUSDT"]
testnet = true

[binance]
api_key = "your-api-key"
api_secret = "your-api-secret"

[risk]
max_position_size_usd = 1000
max_daily_trades = 20
max_leverage = 10
emergency_pnl_threshold = -5.0
```

---

## 6. Monitoring & Observability

### Metrics to Track

| Metric | Description | Target |
|--------|-------------|--------|
| Signal Latency | Time from kline close to signal export | < 100ms |
| Order Execution Time | Time from queue to order placed | < 500ms |
| Fill Rate | Percentage of orders filled | > 95% |
| System Uptime | Time without crashes | > 99.9% |

### Logging

```rust
// Muscle-Kit
info!("Signal exported: BTCUSDT LONG confluence=7");
warn!("High latency detected: 600ms > 500ms threshold");
error!("WebSocket connection lost, reconnecting...");

// Execution-Kit
info!("Order placed: BTCUSDT BUY 0.1 @ 50000");
warn!("Daily trade limit approaching: 18/20");
error!("Order failed: Insufficient balance");
```

---

## 7. Error Handling & Recovery

### Muscle-Kit

| Error | Recovery |
|-------|----------|
| WebSocket disconnect | Auto-reconnect with backoff |
| Redis unavailable | Retry with exponential backoff |
| Indicator calculation error | Skip kline, log error |
| Binance API rate limit | Throttle requests |

### Execution-Kit

| Error | Recovery |
|-------|----------|
| Redis disconnect | Reconnect and resume BRPOP |
| Order rejected | Log error, notify via alert |
| Position sync error | Re-sync from Binance |
| Balance insufficient | Skip order, alert user |

---

## 8. Security Considerations

### API Key Management

```bash
# Use environment variables
export BINANCE_API_KEY="your-key"
export BINANCE_API_SECRET="your-secret"

# Or use secrets manager
aws secretsmanager get-secret-value --secret-id zeroclaw/binance
```

### Network Security

- Bind Redis to localhost only
- Use TLS for external connections
- Enable API key IP whitelist on Binance
- Use firewall rules to restrict access

### Risk Limits

- Set conservative position limits
- Enable emergency stop functionality
- Monitor drawdown in real-time
- Implement circuit breakers

---

*Tài liệu thiết kế Version 1.0 - Cập nhật cho ZeroClaw v0.1.9*
