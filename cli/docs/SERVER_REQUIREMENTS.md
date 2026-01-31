# Breakpoint CLI - Server Requirements and Setup Guide

This guide covers the Interlint API server requirements and setup instructions for running with the Breakpoint CLI.

## Overview

Breakpoint CLI requires the Interlint API server to be running to perform breaking change analysis. The API server:

- Analyzes differences between two package versions
- Detects breaking changes with severity classification
- Returns detailed change reports with fix suggestions
- Communicates using binary MessagePack format for efficiency

---

## Server Requirements

### Minimum Requirements

| Component | Requirement | Notes |
|-----------|-------------|-------|
| **Rust** | 1.70 or later | Required for building from source |
| **CPU** | Single core (2+ recommended) | Analysis is CPU-intensive for large packages |
| **Memory** | 256 MB minimum (1 GB recommended) | Stores package interfaces in memory |
| **Disk** | 100 MB free space | For temporary analysis data |
| **Network** | IPv4 connectivity | For CLI-to-API communication |

### Supported Operating Systems

- **Linux** (Ubuntu 18.04+, CentOS 7+, Debian 10+)
- **macOS** (10.13+)
- **Windows** (Windows Server 2019+, Windows 10+)

### Network Configuration

- **Port:** Default 3000 (configurable via `PORT` environment variable)
- **Host binding:** Default 0.0.0.0 (all interfaces) or use `HOST` environment variable
- **Protocol:** HTTP/1.1 only (not HTTP/2)
- **CORS:** Enabled by default

---

## Building from Source

### Prerequisites

1. **Install Rust:**
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   source $HOME/.cargo/env
   ```

2. **Verify Rust installation:**
   ```bash
   rustc --version  # Should be 1.70+
   cargo --version
   ```

### Building the Server

```bash
# Navigate to API directory
cd interlint/api

# Build in debug mode (faster compilation, slower execution)
cargo build

# Build in release mode (slower compilation, faster execution)
cargo build --release

# Binary locations
# Debug: ./target/debug/interlint-api
# Release: ./target/release/interlint-api
```

### Build Time

- **Debug build:** 30-60 seconds (faster iteration)
- **Release build:** 2-5 minutes (optimized performance)

---

## Running the Server

### Development

```bash
# Run with default settings
cargo run

# Run with specific port
PORT=8080 cargo run

# Run with custom host
HOST=0.0.0.0 PORT=8080 cargo run

# Run with debug output
RUST_LOG=debug cargo run
```

### Production

```bash
# Use release binary for better performance
PORT=3000 HOST=0.0.0.0 ./target/release/interlint-api

# Run in background (Unix/Linux)
nohup PORT=3000 HOST=0.0.0.0 ./target/release/interlint-api > api.log 2>&1 &

# Run in background (macOS with launchd)
# See systemd setup below

# Run in Docker
docker build -t interlint-api .
docker run -p 3000:3000 interlint-api
```

### Systemd Setup (Linux)

Create `/etc/systemd/system/interlint-api.service`:

```ini
[Unit]
Description=Interlint API Server
After=network.target

[Service]
Type=simple
User=interlint
WorkingDirectory=/opt/interlint/api
ExecStart=/opt/interlint/api/target/release/interlint-api
Environment="PORT=3000"
Environment="HOST=0.0.0.0"
Restart=on-failure
RestartSec=10

[Install]
WantedBy=multi-user.target
```

Enable and start:
```bash
sudo systemctl enable interlint-api
sudo systemctl start interlint-api
sudo systemctl status interlint-api
```

### Launchd Setup (macOS)

Create `~/Library/LaunchAgents/com.interlint.api.plist`:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
 "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.interlint.api</string>
    <key>ProgramArguments</key>
    <array>
        <string>/usr/local/bin/interlint-api</string>
    </array>
    <key>EnvironmentVariables</key>
    <dict>
        <key>PORT</key>
        <string>3000</string>
    </dict>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
</dict>
</plist>
```

Load the service:
```bash
launchctl load ~/Library/LaunchAgents/com.interlint.api.plist
launchctl start com.interlint.api.plist
```

---

## Configuration

All configuration is via environment variables:

| Variable | Description | Default | Example |
|----------|-------------|---------|---------|
| `PORT` | Server port | `3000` | `PORT=8080` |
| `HOST` | Host address to bind | `0.0.0.0` | `HOST=127.0.0.1` |
| `RUST_LOG` | Logging level | `info` | `RUST_LOG=debug` |

### Examples

```bash
# Development server on localhost only
HOST=127.0.0.1 PORT=3000 cargo run

# Production server on all interfaces, custom port
HOST=0.0.0.0 PORT=8080 ./target/release/interlint-api

# With verbose logging
RUST_LOG=debug HOST=0.0.0.0 PORT=3000 ./target/release/interlint-api
```

---

## Health Checks

### Basic Health Check

```bash
curl http://localhost:3000/health
```

**Response:**
```json
{
  "status": "healthy",
  "version": "0.1.0"
}
```

### Automated Health Monitoring

```bash
# Check periodically
while true; do
  curl -s http://localhost:3000/health || echo "Server is down"
  sleep 30
done

# Using systemd-timer (Linux)
# Check interlint-api.service status regularly
systemctl status interlint-api
```

---

## Verification

### Verify Server is Running

```bash
# Check process is running
ps aux | grep interlint-api

# Check port is listening
lsof -i :3000

# Verify response to requests
curl -v http://localhost:3000/health
```

### Test with CLI

```bash
# From CLI directory
cd interlint/cli

# Run breakpoint against a test package
./target/release/breakpoint ../examples/v1

# Should connect to API and complete analysis
```

---

## Performance Tuning

### Memory Optimization

For large packages, the server stores interfaces in memory:

```bash
# Monitor memory usage
watch -n 1 'ps aux | grep interlint-api'

# If memory usage is high:
# - Analyze smaller packages
# - Increase available system memory
# - Run in release mode (more optimized)
```

### CPU Optimization

Analysis is CPU-intensive. For slow analysis:

```bash
# Use release build (significant speedup)
cargo build --release

# Run on system with more CPU cores
# The async runtime will utilize available cores
```

### Network Optimization

- API uses HTTP/1.1 (not HTTP/2)
- Binary MessagePack format is 30-50% smaller than JSON
- Typical request size: 10-100 KB (depends on package size)
- Response time: 100-500 ms for typical packages

---

## Reverse Proxy Setup

### Nginx Configuration

```nginx
upstream interlint_api {
    server localhost:3000;
}

server {
    listen 80;
    server_name api.interlint.example.com;

    location / {
        proxy_pass http://interlint_api;
        proxy_http_version 1.1;

        # Required headers
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;

        # Timeouts for large packages
        proxy_connect_timeout 30s;
        proxy_send_timeout 30s;
        proxy_read_timeout 30s;

        # CORS headers (API already sets these)
        add_header 'Access-Control-Allow-Origin' '*';
    }
}
```

### Apache Configuration

```apache
ProxyPreserveHost On
ProxyPass / http://localhost:3000/
ProxyPassReverse / http://localhost:3000/

# Timeouts
ProxyTimeout 30
```

---

## Docker Setup

### Dockerfile

```dockerfile
FROM rust:1.70 as builder

WORKDIR /app
COPY . .

# Build API server
RUN cd api && cargo build --release

FROM debian:bookworm-slim

RUN apt-get update && \
    apt-get install -y ca-certificates && \
    rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/api/target/release/interlint-api /usr/local/bin/

EXPOSE 3000
ENV PORT=3000 HOST=0.0.0.0

CMD ["interlint-api"]
```

### Docker Compose

```yaml
version: '3.8'

services:
  interlint-api:
    build:
      context: .
      dockerfile: Dockerfile
    ports:
      - "3000:3000"
    environment:
      PORT: 3000
      HOST: 0.0.0.0
      RUST_LOG: info
    restart: unless-stopped
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:3000/health"]
      interval: 10s
      timeout: 5s
      retries: 3
```

Run with:
```bash
docker-compose up
```

---

## Logging

### Log Levels

Control logging verbosity with `RUST_LOG`:

```bash
# Default (info level)
RUST_LOG=info cargo run

# Debug level (verbose)
RUST_LOG=debug cargo run

# Specific module logging
RUST_LOG=interlint_api::handlers=debug cargo run

# Minimal logging
RUST_LOG=warn cargo run
```

### Log Output

Logs are printed to stdout:

```bash
# Save logs to file
cargo run > api.log 2>&1

# Follow logs in real-time
tail -f api.log

# Filter logs
grep "ERROR" api.log
```

---

## Troubleshooting

### Server won't start

**Error:** `Address already in use`

```bash
# Kill process using port 3000
lsof -i :3000 | grep LISTEN | awk '{print $2}' | xargs kill -9

# Or use different port
PORT=3001 cargo run
```

### Server crashes

```bash
# Run with debug logging to see what's wrong
RUST_LOG=debug cargo run

# Check system resources
free -h  # Memory
df -h    # Disk space
top      # CPU usage
```

### High memory usage

- This is normal for large package analysis
- Memory is freed after each analysis completes
- If persistent, restart the server

### Connection timeouts

- Increase request timeout in CLI via API timeout settings
- Check network connectivity between CLI and API
- Verify API is not overloaded (check logs)

---

## Performance Benchmarks

Expected performance (typical system):

| Operation | Time |
|-----------|------|
| Health check | 5-10 ms |
| Compare small package (<10 exports) | 50-100 ms |
| Compare medium package (10-100 exports) | 100-300 ms |
| Compare large package (100-1000 exports) | 300-800 ms |

*Times may vary based on system specifications and package complexity.*

---

## Monitoring

### Key Metrics

- **Response time:** Should be <1s for typical packages
- **Memory usage:** Grows with package size, freed after analysis
- **CPU usage:** Peaks during analysis, returns to baseline
- **Uptime:** Should restart gracefully on failure

### Example Monitoring Script

```bash
#!/bin/bash
while true; do
  status=$(curl -s http://localhost:3000/health)
  if [ $? -eq 0 ]; then
    echo "$(date): Server healthy"
  else
    echo "$(date): Server down! Restarting..."
    # Add restart logic here
  fi
  sleep 60
done
```

---

## Deployment Checklist

- [ ] Rust 1.70+ installed
- [ ] Server built in release mode
- [ ] Environment variables configured
- [ ] Port 3000 (or custom) is accessible
- [ ] Health check endpoint responds
- [ ] CLI can connect and run analysis
- [ ] Logs are being captured
- [ ] Monitoring/alerting is set up
- [ ] Firewall rules allow traffic
- [ ] Reverse proxy configured (if needed)

---

## Support and Issues

For server-related issues:

1. Check the troubleshooting section above
2. Review logs with appropriate `RUST_LOG` level
3. Verify health endpoint: `curl http://localhost:3000/health`
4. Run CLI with verbose output
5. Check network connectivity and firewall rules

See main README for more information.
