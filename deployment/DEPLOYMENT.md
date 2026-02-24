# GeoConsole V3 - VPS Deployment Guide

This guide covers deploying GeoConsole V3 to a Linux VPS with Cloudflare as the CDN/proxy.

## Architecture

```
Cloudflare (CDN + TLS)
    ↓
VPS (Linux) :80/:443
    ├── Reverse Proxy (nginx/caddy/traefik)
    │   └── Forwards to 127.0.0.1:3003
    │
    └── Rust Backend (native binary, systemd)
        ├── Bound to 127.0.0.1:3003 (localhost only)
        ├── Serves API endpoints (/api/*)
        ├── Serves static frontend (/)
        └── DuckDB embedded database
```

**Security**: Backend binds to `127.0.0.1` by default, preventing direct external access. Use a reverse proxy (nginx/Caddy) or set `HOST=0.0.0.0` if Cloudflare connects directly via Cloudflare Tunnel.

## Prerequisites

### On VPS
- **OS**: Ubuntu 20.04+ / Debian 11+ / RHEL 8+
- **RAM**: 2GB minimum (4GB+ recommended for large datasets)
- **Disk**: 10GB+ (depends on dataset size)
- **Docker**: For building frontend only
- **Reverse Proxy**: nginx, Caddy, or Traefik (recommended)
- **Ports**: 80/443 for reverse proxy (backend uses 127.0.0.1:3003)

### On Development Machine
- **Rust**: 1.70+ (for cross-compilation)
- **Node.js**: 20+
- **Docker**: For frontend builds

### Cloudflare Setup
- Domain pointed to VPS IP
- Proxy enabled (orange cloud)
- SSL/TLS mode: **Full** or **Full (strict)**

## Build Process

### Option 1: Build on VPS (Recommended)

```bash
# On VPS
git clone <your-repo> /tmp/geoconsole-build
cd /tmp/geoconsole-build

# Install Rust (if not installed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env

# Build backend (release mode)
cargo build --release

# Build frontend (requires Docker)
chmod +x deployment/build-frontend.sh
./deployment/build-frontend.sh

# Deploy
sudo ./deployment/deploy.sh
```

### Option 2: Cross-compile from Windows

```powershell
# Install cross-compilation target
rustup target add x86_64-unknown-linux-gnu

# Install cross-compilation toolchain (requires WSL or Docker)
# Using cross (recommended):
cargo install cross

# Build for Linux
cross build --release --target x86_64-unknown-linux-gnu

# Build frontend
.\deployment\build-frontend.sh  # or use Docker directly

# Transfer files to VPS
scp target/x86_64-unknown-linux-gnu/release/geoconsole-v3 user@vps:/tmp/
scp -r frontend/dist user@vps:/tmp/
scp -r deployment user@vps:/tmp/
```

### Option 3: CI/CD Pipeline (GitHub Actions example)

```yaml
# .github/workflows/deploy.yml
name: Deploy to VPS

on:
  push:
    branches: [main]

jobs:
  build-and-deploy:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      
      - name: Setup Rust
        uses: actions-rs/toolchain@v1
        with:
          toolchain: stable
      
      - name: Build backend
        run: cargo build --release
      
      - name: Build frontend
        run: |
          chmod +x deployment/build-frontend.sh
          ./deployment/build-frontend.sh
      
      - name: Deploy to VPS
        uses: appleboy/scp-action@master
        with:
          host: ${{ secrets.VPS_HOST }}
          username: ${{ secrets.VPS_USER }}
          key: ${{ secrets.VPS_SSH_KEY }}
          source: "target/release/geoconsole-v3,frontend/dist,deployment"
          target: "/tmp/geoconsole-deploy"
      
      - name: Run deployment script
        uses: appleboy/ssh-action@master
        with:
          host: ${{ secrets.VPS_HOST }}
          username: ${{ secrets.VPS_USER }}
          key: ${{ secrets.VPS_SSH_KEY }}
          script: |
            cd /tmp/geoconsole-deploy
            sudo ./deployment/deploy.sh
```

## Manual Deployment Steps

### 1. Prepare VPS

```bash
# Update system
sudo apt update && sudo apt upgrade -y

# Install Docker (for frontend builds)
curl -fsSL https://get.docker.com | sh
sudo usermod -aG docker $USER

# Create deployment user
sudo useradd -r -s /bin/false -d /opt/geoconsole geoconsole
sudo mkdir -p /opt/geoconsole/{data,frontend/dist}
sudo chown -R geoconsole:geoconsole /opt/geoconsole
```

### 2. Transfer Files

```bash
# From your dev machine
scp target/release/geoconsole-v3 user@vps:/tmp/
scp -r frontend/dist user@vps:/tmp/
scp -r deployment user@vps:/tmp/
```

### 3. Deploy

```bash
# On VPS
cd /tmp
sudo ./deployment/deploy.sh
```

The script will:
- Copy binary to `/opt/geoconsole/geoconsole-v3`
- Copy frontend to `/opt/geoconsole/frontend/dist/`
- Install systemd service
- Start the service

### 4. Verify Deployment

```bash
# Check service status
sudo systemctl status geoconsole

# View logs
sudo journalctl -u geoconsole -f

# Test health endpoint
curl http://localhost:3003/health
```

Expected response:
```json
{
  "status": "healthy",
  "service": "geoconsole-v3",
  "version": "0.1.0"
}
```

## Reverse Proxy Setup

Since the backend binds to `127.0.0.1:3003` (localhost only), you need a reverse proxy to handle external traffic.

### Option 1: Nginx (Recommended)

```bash
# Install nginx
sudo apt install nginx

# Create config
sudo nano /etc/nginx/sites-available/geoconsole
```

```nginx
server {
    listen 80;
    server_name yourdomain.com;

    # Redirect HTTP to HTTPS (Cloudflare handles TLS)
    return 301 https://$server_name$request_uri;
}

server {
    listen 443 ssl http2;
    server_name yourdomain.com;

    # Cloudflare Origin Certificate (optional, for Full (strict) mode)
    # ssl_certificate /etc/ssl/certs/cloudflare-origin.pem;
    # ssl_certificate_key /etc/ssl/private/cloudflare-origin.key;

    # For Cloudflare Full mode (not strict), self-signed is fine
    ssl_certificate /etc/ssl/certs/ssl-cert-snakeoil.pem;
    ssl_certificate_key /etc/ssl/private/ssl-cert-snakeoil.key;

    # Large file uploads
    client_max_body_size 100M;

    # Proxy to backend
    location / {
        proxy_pass http://127.0.0.1:3003;
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection 'upgrade';
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
        proxy_cache_bypass $http_upgrade;
        
        # Timeouts for large dataset uploads
        proxy_connect_timeout 300s;
        proxy_send_timeout 300s;
        proxy_read_timeout 300s;
    }
}
```

```bash
# Enable site
sudo ln -s /etc/nginx/sites-available/geoconsole /etc/nginx/sites-enabled/
sudo nginx -t
sudo systemctl restart nginx
```

### Option 2: Caddy (Easiest)

```bash
# Install Caddy
sudo apt install -y debian-keyring debian-archive-keyring apt-transport-https
curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/gpg.key' | sudo gpg --dearmor -o /usr/share/keyrings/caddy-stable-archive-keyring.gpg
curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/debian.deb.txt' | sudo tee /etc/apt/sources.list.d/caddy-stable.list
sudo apt update
sudo apt install caddy

# Create Caddyfile
sudo nano /etc/caddy/Caddyfile
```

```caddy
yourdomain.com {
    reverse_proxy 127.0.0.1:3003
    
    # Caddy automatically handles HTTPS with Let's Encrypt
    # But since Cloudflare terminates TLS, you may want:
    tls internal  # Use self-signed cert (Cloudflare Full mode)
}
```

```bash
sudo systemctl restart caddy
```

### Option 3: Cloudflare Tunnel (No reverse proxy needed)

Use Cloudflare Tunnel to connect directly without opening ports:

```bash
# Install cloudflared
wget https://github.com/cloudflare/cloudflared/releases/latest/download/cloudflared-linux-amd64.deb
sudo dpkg -i cloudflared-linux-amd64.deb

# Authenticate
cloudflared tunnel login

# Create tunnel
cloudflared tunnel create geoconsole

# Configure tunnel
nano ~/.cloudflared/config.yml
```

```yaml
tunnel: <tunnel-id>
credentials-file: /home/user/.cloudflared/<tunnel-id>.json

ingress:
  - hostname: yourdomain.com
    service: http://127.0.0.1:3003
  - service: http_status:404
```

```bash
# Run tunnel
cloudflared tunnel route dns geoconsole yourdomain.com
cloudflared tunnel run geoconsole
```

With Cloudflare Tunnel, you don't need nginx/Caddy and can keep UFW completely closed.

### UFW Firewall Setup

```bash
# Allow SSH
sudo ufw allow 22/tcp

# If using nginx/Caddy (allow HTTP/HTTPS)
sudo ufw allow 80/tcp
sudo ufw allow 443/tcp

# If using Cloudflare Tunnel (no ports needed)
# Just allow SSH

# Enable firewall
sudo ufw enable

# Verify backend is NOT accessible externally
curl http://your-vps-ip:3003/health  # Should timeout/refuse

# Verify it works locally
ssh user@vps
curl http://127.0.0.1:3003/health  # Should return {"status":"healthy",...}
```

## Cloudflare Configuration

### 1. DNS Setup
- **Type**: A
- **Name**: @ (or subdomain like `maps`)
- **Content**: Your VPS IP
- **Proxy status**: Proxied (orange cloud)

### 2. SSL/TLS Settings
- **SSL/TLS encryption mode**: Full or Full (strict)
- **Always Use HTTPS**: On
- **Minimum TLS Version**: 1.2

### 3. Critical: COOP/COEP Headers

DuckDB-WASM requires `SharedArrayBuffer`, which needs these headers:
- `Cross-Origin-Opener-Policy: same-origin`
- `Cross-Origin-Embedder-Policy: require-corp`

The Rust backend sets these headers, but Cloudflare must **pass them through**.

**Option A: Transform Rules (Recommended)**

Go to **Rules → Transform Rules → Modify Response Header**:

1. **Rule name**: Pass COOP/COEP headers
2. **When incoming requests match**: `Hostname equals yourdomain.com`
3. **Then**:
   - Set static `Cross-Origin-Opener-Policy` = `same-origin`
   - Set static `Cross-Origin-Embedder-Policy` = `require-corp`

**Option B: Workers (Advanced)**

```javascript
export default {
  async fetch(request, env) {
    const response = await fetch(request);
    const newHeaders = new Headers(response.headers);
    
    newHeaders.set('Cross-Origin-Opener-Policy', 'same-origin');
    newHeaders.set('Cross-Origin-Embedder-Policy', 'require-corp');
    
    return new Response(response.body, {
      status: response.status,
      statusText: response.statusText,
      headers: newHeaders
    });
  }
};
```

### 4. Performance Settings
- **Caching Level**: Standard
- **Browser Cache TTL**: 4 hours (for static assets)
- **Auto Minify**: Enable CSS, JS, HTML
- **Brotli**: On

### 5. Firewall Rules (Optional)
Restrict API endpoints to prevent abuse:
- Rate limit `/api/datasets` (POST) to 10 req/min per IP
- Challenge on `/api/*` if threat score > 10

## Environment Variables

Edit `/etc/systemd/system/geoconsole.service`:

```ini
[Service]
Environment="RUST_LOG=info,geoconsole_v3=debug"
Environment="HOST=0.0.0.0"
Environment="PORT=3003"
Environment="STATIC_DIR=/opt/geoconsole/frontend/dist"
```

After changes:
```bash
sudo systemctl daemon-reload
sudo systemctl restart geoconsole
```

## Service Management

```bash
# Start
sudo systemctl start geoconsole

# Stop
sudo systemctl stop geoconsole

# Restart
sudo systemctl restart geoconsole

# Enable on boot
sudo systemctl enable geoconsole

# Disable
sudo systemctl disable geoconsole

# View logs
sudo journalctl -u geoconsole -f

# View last 100 lines
sudo journalctl -u geoconsole -n 100
```

## Updating the Application

```bash
# Build new version
cargo build --release
./deployment/build-frontend.sh

# Deploy
sudo ./deployment/deploy.sh

# Service will automatically restart
```

## Backup Strategy

### Database Backup

```bash
# Backup DuckDB file
sudo cp /opt/geoconsole/data/geoconsole.duckdb \
       /opt/geoconsole/data/geoconsole.duckdb.backup-$(date +%Y%m%d)

# Or use rsync for remote backup
rsync -avz /opt/geoconsole/data/ user@backup-server:/backups/geoconsole/
```

### Automated Backups (Cron)

```bash
# Edit crontab
sudo crontab -e

# Add daily backup at 2 AM
0 2 * * * cp /opt/geoconsole/data/geoconsole.duckdb \
             /opt/geoconsole/data/backups/geoconsole-$(date +\%Y\%m\%d).duckdb
```

## Troubleshooting

### Service won't start

```bash
# Check logs
sudo journalctl -u geoconsole -n 50

# Common issues:
# 1. Port already in use
sudo lsof -i :3003

# 2. Permission denied on data directory
sudo chown -R geoconsole:geoconsole /opt/geoconsole/data

# 3. Binary not executable
sudo chmod +x /opt/geoconsole/geoconsole-v3
```

### Frontend not loading

```bash
# Check if static files exist
ls -la /opt/geoconsole/frontend/dist/

# Rebuild frontend
./deployment/build-frontend.sh
sudo cp -r frontend/dist/* /opt/geoconsole/frontend/dist/
sudo systemctl restart geoconsole
```

### DuckDB-WASM not working

Check browser console for `SharedArrayBuffer` errors:
1. Verify COOP/COEP headers in browser DevTools → Network
2. Check Cloudflare Transform Rules are active
3. Ensure site is accessed via HTTPS (not HTTP)

### High memory usage

DuckDB loads data into memory for processing:
```bash
# Check memory usage
free -h
sudo systemctl status geoconsole

# Adjust if needed (add to service file)
Environment="DUCKDB_MEMORY_LIMIT=2GB"
```

## Security Hardening

### 1. Firewall (UFW)

```bash
sudo ufw allow 22/tcp    # SSH
sudo ufw allow 3003/tcp  # GeoConsole (or 80/443 if using those)
sudo ufw enable
```

### 2. Restrict CORS (Production)

Edit `src/main.rs`:
```rust
.layer(CorsLayer::new()
    .allow_origin("https://yourdomain.com".parse::<HeaderValue>().unwrap())
    .allow_methods([Method::GET, Method::POST, Method::DELETE])
    .allow_headers(Any))
```

### 3. File Upload Limits

Already set to 100MB in `main.rs`. Adjust if needed:
```rust
.layer(DefaultBodyLimit::max(50 * 1024 * 1024)) // 50MB
```

### 4. Rate Limiting

Use Cloudflare rate limiting rules or add middleware:
```bash
# Install fail2ban for SSH protection
sudo apt install fail2ban
```

## Monitoring

### Basic Health Check

```bash
# Add to cron for uptime monitoring
*/5 * * * * curl -f http://localhost:3003/health || echo "Service down" | mail -s "GeoConsole Alert" admin@example.com
```

### Prometheus Metrics (Optional)

Add `axum-prometheus` to `Cargo.toml` for metrics endpoint.

## Performance Tuning

### 1. Increase File Descriptors

```bash
# Edit /etc/security/limits.conf
geoconsole soft nofile 65536
geoconsole hard nofile 65536
```

### 2. DuckDB Configuration

Set in environment or code:
- `threads`: Number of CPU cores
- `memory_limit`: Max RAM for queries
- `temp_directory`: SSD path for spills

### 3. Cloudflare Caching

Cache static assets aggressively:
- **Page Rule**: `yourdomain.com/assets/*` → Cache Level: Cache Everything, Edge TTL: 1 month

## Cost Estimation

### VPS (Monthly)
- **Basic** (2GB RAM, 1 CPU): $5-10/mo (DigitalOcean, Linode, Vultr)
- **Standard** (4GB RAM, 2 CPU): $12-20/mo
- **High-performance** (8GB RAM, 4 CPU): $40-60/mo

### Cloudflare
- **Free tier**: Sufficient for most use cases
- **Pro** ($20/mo): Advanced caching, image optimization
- **Business** ($200/mo): Custom SSL, advanced DDoS

### Storage
- DuckDB file grows with datasets
- 1M features ≈ 100-500MB (depends on geometry complexity)
- Budget 1GB per 2-5M features

## Support

- **Logs**: `sudo journalctl -u geoconsole -f`
- **Health**: `curl http://localhost:3003/health`
- **DuckDB**: Check `/opt/geoconsole/data/geoconsole.duckdb`
