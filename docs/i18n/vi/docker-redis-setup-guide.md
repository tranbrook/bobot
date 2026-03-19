# Hướng Dẫn Cài Đặt ZeroClaw Trading System với Docker & Redis

## Mục Lục

1. [Tổng Quan](#tổng-quan)
2. [Yêu Cầu](#yêu-cầu)
3. [Cài Đặt Docker](#cài-đặt-docker)
4. [Cài Đặt Redis với Docker](#cài-đặt-redis-với-docker)
5. [Chạy ZeroClaw với Docker](#chạy-zeroclaw-với-docker)
6. [Docker Compose](#docker-compose)
7. [Troubleshooting](#troubleshooting)

---

## Tổng Quan

Hướng dẫn này sẽ giúp bạn chạy toàn bộ hệ thống ZeroClaw Trading với Docker và Redis containerized.

### Kiến Trúc Docker

```
┌─────────────────────────────────────────────────────────┐
│  Docker Host                                            │
│                                                         │
│  ┌─────────────────┐    ┌─────────────────┐            │
│  │  Redis          │    │  ZeroClaw       │            │
│  │  Container      │◀──▶│  Container      │            │
│  │  (Port 6379)    │    │  (App)          │            │
│  └─────────────────┘    └─────────────────┘            │
│         ▲                       ▲                       │
│         │                       │                       │
│         └───────────────────────┘                       │
│                     │                                   │
│         ┌───────────┴───────────┐                       │
│         ▼                       ▼                       │
│  ┌─────────────────┐    ┌─────────────────┐            │
│  │  Muscle-Kit     │    │  Execution-Kit  │            │
│  │  (Signal Gen)   │    │  (Binance API)  │            │
│  └─────────────────┘    └─────────────────┘            │
└─────────────────────────────────────────────────────────┘
```

---

## Yêu Cầu

### Phần Cứng

- CPU: 2 cores+ (4 cores recommended)
- RAM: 4GB+ (8GB recommended)
- Storage: 20GB+ SSD
- Network: Ổn định, latency thấp

### Phần Mềm

- Docker 24.0+
- Docker Compose 2.0+
- Git

---

## Cài Đặt Docker

### Ubuntu/Debian

```bash
# Gỡ Docker cũ (nếu có)
for pkg in docker.io docker-doc docker-compose docker-compose-v2 podman-docker containerd runc; do 
  sudo apt-get remove -y $pkg
done

# Cài đặt Docker
sudo apt-get update
sudo apt-get install -y ca-certificates curl gnupg
sudo install -m 0755 -d /etc/apt/keyrings
curl -fsSL https://download.docker.com/linux/ubuntu/gpg | sudo gpg --dearmor -o /etc/apt/keyrings/docker.gpg
sudo chmod a+r /etc/apt/keyrings/docker.gpg

echo "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/docker.gpg] https://download.docker.com/linux/ubuntu $(. /etc/os-release && echo "$VERSION_CODENAME") stable" | sudo tee /etc/apt/sources.list.d/docker.list > /dev/null

sudo apt-get update
sudo apt-get install -y docker-ce docker-ce-cli containerd.io docker-buildx-plugin docker-compose-plugin

# Thêm user vào docker group
sudo usermod -aG docker $USER
newgrp docker

# Kiểm tra
docker --version
docker compose version
```

### CentOS/RHEL

```bash
sudo yum remove -y docker docker-client docker-client-latest docker-common docker-latest docker-latest-logrotate docker-logrotate docker-engine

sudo yum install -y yum-utils
sudo yum-config-manager --add-repo https://download.docker.com/linux/centos/docker-ce.repo

sudo yum install -y docker-ce docker-ce-cli containerd.io docker-buildx-plugin docker-compose-plugin

sudo systemctl start docker
sudo systemctl enable docker

sudo usermod -aG docker $USER
newgrp docker
```

### macOS

```bash
# Cài đặt Homebrew (nếu chưa có)
/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"

# Cài đặt Docker Desktop
brew install --cask docker

# Hoặc dùng Docker CLI
brew install docker docker-compose
```

### Windows

1. Tải Docker Desktop từ: https://www.docker.com/products/docker-desktop/
2. Cài đặt và khởi động Docker Desktop
3. Kiểm tra trong PowerShell:
```powershell
docker --version
docker compose version
```

---

## Cài Đặt Redis với Docker

### Option 1: Chạy Redis Đơn Giản

```bash
# Chạy Redis container
docker run -d \
  --name redis-zeroclaw \
  -p 6379:6379 \
  -v redis-data:/data \
  --restart unless-stopped \
  redis:7-alpine

# Kiểm tra
docker ps | grep redis
docker logs redis-zeroclaw

# Test kết nối
docker exec -it redis-zeroclaw redis-cli ping
# Output: PONG
```

### Option 2: Chạy Redis với Cấu Hình

```bash
# Tạo thư mục config
mkdir -p ~/zeroclaw/redis

# Tạo redis.conf
cat > ~/zeroclaw/redis/redis.conf << EOF
# Redis configuration for ZeroClaw
port 6379
bind 0.0.0.0
requirepass your-redis-password-here
maxmemory 256mb
maxmemory-policy allkeys-lru
appendonly yes
appendfsync everysec
EOF

# Chạy Redis với config
docker run -d \
  --name redis-zeroclaw \
  -p 6379:6379 \
  -v ~/zeroclaw/redis/redis.conf:/etc/redis/redis.conf:ro \
  -v redis-data:/data \
  --restart unless-stopped \
  redis:7-alpine \
  redis-server /etc/redis/redis.conf
```

### Option 3: Redis với Sentinel (High Availability)

```yaml
# ~/zeroclaw/docker-compose.redis.yml
version: '3.8'

services:
  redis-master:
    image: redis:7-alpine
    container_name: redis-master
    ports:
      - "6379:6379"
    volumes:
      - redis-master-data:/data
      - ./redis/master.conf:/etc/redis/redis.conf:ro
    command: redis-server /etc/redis/redis.conf
    restart: unless-stopped

  redis-slave:
    image: redis:7-alpine
    container_name: redis-slave
    ports:
      - "6380:6379"
    volumes:
      - redis-slave-data:/data
      - ./redis/slave.conf:/etc/redis/redis.conf:ro
    command: redis-server /etc/redis/redis.conf
    depends_on:
      - redis-master
    restart: unless-stopped

volumes:
  redis-master-data:
  redis-slave-data:
```

```bash
# Tạo config files
mkdir -p ~/zeroclaw/redis

cat > ~/zeroclaw/redis/master.conf << EOF
port 6379
bind 0.0.0.0
requirepass your-password
appendonly yes
EOF

cat > ~/zeroclaw/redis/slave.conf << EOF
port 6379
bind 0.0.0.0
requirepass your-password
replicaof redis-master 6379
masterauth your-password
appendonly yes
EOF

# Start Redis cluster
cd ~/zeroclaw
docker compose -f docker-compose.redis.yml up -d
```

---

## Chạy ZeroClaw với Docker

### Tạo Dockerfile

```dockerfile
# Dockerfile
FROM rust:1.87-slim-bookworm as builder

WORKDIR /app

# Install dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Copy source
COPY . .

# Build release
RUN cargo build --release

# Runtime image
FROM debian:bookworm-slim

WORKDIR /app

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

# Copy binary from builder
COPY --from=builder /app/target/release/zeroclaw /usr/local/bin/

# Create non-root user
RUN useradd -m -u 1000 zeroclaw
USER zeroclaw

# Create directories
RUN mkdir -p /home/zeroclaw/.zeroclaw

EXPOSE 42617

ENTRYPOINT ["zeroclaw"]
```

### Build Docker Image

```bash
# Build image
docker build -t zeroclaw:latest .

# Kiểm tra image
docker images | grep zeroclaw
```

### Chạy ZeroClaw Container

```bash
# Tạo thư mục config
mkdir -p ~/zeroclaw/config

# Copy config file
cp dev/config.trading.example.toml ~/zeroclaw/config/config.toml

# Chỉnh sửa config
nano ~/zeroclaw/config/config.toml

# Chạy container
docker run -d \
  --name zeroclaw \
  --network host \
  -v ~/zeroclaw/config:/home/zeroclaw/.zeroclaw:ro \
  -v ~/zeroclaw/logs:/home/zeroclaw/.zeroclaw/logs \
  -v ~/zeroclaw/data:/home/zeroclaw/.zeroclaw/data \
  -e ZEROCLAW_API_KEY="your-api-key" \
  -e RUST_LOG=info \
  --restart unless-stopped \
  zeroclaw:latest \
  daemon
```

---

## Docker Compose

### File Docker Compose Hoàn Chỉnh

```yaml
# ~/zeroclaw/docker-compose.yml
version: '3.8'

services:
  # Redis - Message Broker
  redis:
    image: redis:7-alpine
    container_name: zeroclaw-redis
    ports:
      - "127.0.0.1:6379:6379"
    volumes:
      - redis-data:/data
      - ./redis/redis.conf:/etc/redis/redis.conf:ro
    command: redis-server /etc/redis/redis.conf
    healthcheck:
      test: ["CMD", "redis-cli", "ping"]
      interval: 10s
      timeout: 5s
      retries: 5
    restart: unless-stopped
    networks:
      - zeroclaw-network

  # ZeroClaw Agent
  zeroclaw:
    build:
      context: .
      dockerfile: Dockerfile
    container_name: zeroclaw-agent
    depends_on:
      redis:
        condition: service_healthy
    environment:
      - ZEROCLAW_API_KEY=${ZEROCLAW_API_KEY}
      - REDIS_URL=redis://redis:6379
      - RUST_LOG=${RUST_LOG:-info}
    volumes:
      - ./config:/home/zeroclaw/.zeroclaw:ro
      - ./logs:/home/zeroclaw/.zeroclaw/logs
      - ./data:/home/zeroclaw/.zeroclaw/data
    ports:
      - "127.0.0.1:42617:42617"
    command: ["daemon"]
    restart: unless-stopped
    networks:
      - zeroclaw-network
    healthcheck:
      test: ["CMD", "zeroclaw", "status"]
      interval: 30s
      timeout: 10s
      retries: 3

  # Muscle-Kit (Signal Generator)
  muscle-kit:
    image: zeroclaw-muscle:latest
    container_name: zeroclaw-muscle
    depends_on:
      - redis
    environment:
      - REDIS_URL=redis://redis:6379
      - BINANCE_API_KEY=${BINANCE_API_KEY}
      - BINANCE_API_SECRET=${BINANCE_API_SECRET}
    volumes:
      - ./muscle-config:/app/config
    command: ["--symbols", "BTCUSDT,ETHUSDT"]
    restart: unless-stopped
    networks:
      - zeroclaw-network

  # Execution Engine
  execution-engine:
    image: zeroclaw-execution:latest
    container_name: zeroclaw-execution
    depends_on:
      - redis
    environment:
      - REDIS_URL=redis://redis:6379
      - BINANCE_API_KEY=${BINANCE_API_KEY}
      - BINANCE_API_SECRET=${BINANCE_API_SECRET}
      - BINANCE_TESTNET=${BINANCE_TESTNET:-true}
    volumes:
      - ./execution-config:/app/config
    command: ["--symbols", "BTCUSDT,ETHUSDT"]
    restart: unless-stopped
    networks:
      - zeroclaw-network

  # Dashboard (Optional)
  dashboard:
    image: nginx:alpine
    container_name: zeroclaw-dashboard
    ports:
      - "8080:80"
    volumes:
      - ./docs/trading-dashboard.html:/usr/share/nginx/html/index.html:ro
    depends_on:
      - zeroclaw
    restart: unless-stopped
    networks:
      - zeroclaw-network

volumes:
  redis-data:

networks:
  zeroclaw-network:
    driver: bridge
```

### Environment File

```bash
# ~/zeroclaw/.env
# API Keys
ZEROCLAW_API_KEY="your-zeroclaw-api-key"
BINANCE_API_KEY="your-binance-api-key"
BINANCE_API_SECRET="your-binance-secret"

# Settings
RUST_LOG=info
BINANCE_TESTNET=true

# Redis
REDIS_PASSWORD=your-redis-password
```

### Start Toàn Bộ Hệ Thống

```bash
cd ~/zeroclaw

# Start tất cả services
docker compose up -d

# Kiểm tra status
docker compose ps

# Xem logs
docker compose logs -f

# Xem logs của service cụ thể
docker compose logs -f zeroclaw
docker compose logs -f redis
docker compose logs -f muscle-kit
```

### Stop Hệ Thống

```bash
# Stop tất cả
docker compose down

# Stop và xóa volumes (cẩn thận!)
docker compose down -v
```

---

## Quản Lý Docker

### Xem Logs

```bash
# Logs của tất cả containers
docker compose logs -f

# Logs của Redis
docker logs -f zeroclaw-redis

# Logs của ZeroClaw
docker logs -f zeroclaw-agent

# Logs 100 dòng cuối
docker logs --tail 100 zeroclaw-agent
```

### Truy Cập Container

```bash
# Truy cập Redis CLI
docker exec -it zeroclaw-redis redis-cli

# Với password
docker exec -it zeroclaw-redis redis-cli -a your-password

# Truy cập ZeroClaw shell
docker exec -it zeroclaw-agent sh

# Kiểm tra sức khỏe
docker exec zeroclaw-agent zeroclaw status
```

### Restart Services

```bash
# Restart tất cả
docker compose restart

# Restart service cụ thể
docker compose restart zeroclaw
docker compose restart redis
```

### Update System

```bash
# Pull images mới
docker compose pull

# Rebuild và restart
docker compose up -d --build

# Cleanup old images
docker image prune -f
```

---

## Troubleshooting

### Redis Connection Failed

```bash
# Kiểm tra Redis đang chạy
docker ps | grep redis

# Test kết nối từ host
docker exec zeroclaw-redis redis-cli ping

# Xem Redis logs
docker logs zeroclaw-redis

# Restart Redis
docker compose restart redis
```

### ZeroClaw Container Exit

```bash
# Xem logs
docker logs zeroclaw-agent

# Kiểm tra config
docker exec zeroclaw-agent cat /home/zeroclaw/.zeroclaw/config.toml

# Chạy với debug mode
docker compose up -d zeroclaw
docker logs -f zeroclaw-agent

# Truy cập container debug
docker run -it --rm \
  --network zeroclaw-network \
  -v ~/zeroclaw/config:/home/zeroclaw/.zeroclaw \
  zeroclaw:latest \
  status
```

### Memory Issues

```bash
# Kiểm tra memory usage
docker stats

# Giới hạn memory trong docker-compose.yml
services:
  zeroclaw:
    deploy:
      resources:
        limits:
          memory: 512M
        reservations:
          memory: 256M
```

### Network Issues

```bash
# Kiểm tra network
docker network ls
docker network inspect zeroclaw-network

# Test kết nối giữa containers
docker exec zeroclaw-agent ping redis

# Reset network
docker compose down
docker network prune
docker compose up -d
```

### Volume Issues

```bash
# Kiểm tra volumes
docker volume ls
docker volume inspect zeroclaw_redis-data

# Backup data
docker run --rm \
  -v zeroclaw_redis-data:/data \
  -v $(pwd):/backup \
  alpine tar czf /backup/redis-backup.tar.gz /data

# Restore data
docker run --rm \
  -v zeroclaw_redis-data:/data \
  -v $(pwd):/backup \
  alpine tar xzf /backup/redis-backup.tar.gz -C /
```

---

## Monitoring

### Docker Stats

```bash
# Real-time resource usage
docker stats

# One-time snapshot
docker stats --no-stream
```

### Health Checks

```bash
# Kiểm tra health
docker inspect --format='{{.State.Health.Status}}' zeroclaw-agent

# Xem health logs
docker inspect --format='{{json .State.Health}}' zeroclaw-agent | jq
```

### Prometheus Metrics (Optional)

```yaml
# Thêm vào docker-compose.yml
services:
  prometheus:
    image: prom/prometheus:latest
    container_name: zeroclaw-prometheus
    ports:
      - "9090:9090"
    volumes:
      - ./prometheus.yml:/etc/prometheus/prometheus.yml:ro
    depends_on:
      - zeroclaw
    restart: unless-stopped

  grafana:
    image: grafana/grafana:latest
    container_name: zeroclaw-grafana
    ports:
      - "3000:3000"
    environment:
      - GF_SECURITY_ADMIN_PASSWORD=admin
    volumes:
      - grafana-data:/var/lib/grafana
    depends_on:
      - prometheus
    restart: unless-stopped
```

---

## Best Practices

### Security

1. **Không expose Redis ra public**
   ```yaml
   ports:
     - "127.0.0.1:6379:6379"  # Good
     # - "6379:6379"  # Bad
   ```

2. **Dùng secrets cho sensitive data**
   ```yaml
   secrets:
     - api_key
   
   services:
     zeroclaw:
       secrets:
         - api_key
   ```

3. **Run as non-root user**
   ```dockerfile
   RUN useradd -m -u 1000 zeroclaw
   USER zeroclaw
   ```

### Performance

1. **Dùng volume cho data**
   ```yaml
   volumes:
     - redis-data:/data
   ```

2. **Set resource limits**
   ```yaml
   deploy:
     resources:
       limits:
         cpus: '1'
         memory: 512M
   ```

3. **Use Alpine images**
   ```yaml
   image: redis:7-alpine  # Good
   # image: redis:7  # Larger
   ```

### Backup

```bash
#!/bin/bash
# backup.sh

DATE=$(date +%Y%m%d_%H%M%S)
BACKUP_DIR=~/zeroclaw/backups

mkdir -p $BACKUP_DIR

# Backup Redis
docker run --rm \
  -v zeroclaw_redis-data:/data \
  -v $BACKUP_DIR:/backup \
  alpine tar czf /backup/redis-$DATE.tar.gz /data

# Backup config
tar czf $BACKUP_DIR/config-$DATE.tar.gz ~/zeroclaw/config

# Keep only last 7 backups
find $BACKUP_DIR -name "*.tar.gz" -mtime +7 -delete

echo "Backup completed: $DATE"
```

---

## Quick Commands Reference

```bash
# Start system
cd ~/zeroclaw && docker compose up -d

# Stop system
docker compose down

# View logs
docker compose logs -f

# Restart service
docker compose restart <service>

# Access Redis
docker exec -it zeroclaw-redis redis-cli

# Check status
docker exec zeroclaw-agent zeroclaw status

# View stats
docker stats

# Backup
./backup.sh

# Update
docker compose pull && docker compose up -d
```

---

*Hướng dẫn này được cập nhật cho ZeroClaw v0.1.9*
