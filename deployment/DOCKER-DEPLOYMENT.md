# GeoConsole V3 - Docker Deployment Guide

Complete Docker-based deployment for VPS with Cloudflare Tunnel.

## Architecture

```
Cloudflare Tunnel
    ↓
VPS (Linux)
    ↓
Docker Container (geoconsole-backend)
    ├── Rust Backend :3003
    ├── Serves /api/* (API endpoints)
    ├── Serves /* (static frontend)
    └── DuckDB (volume mounted)
```

## Prerequisites

- **VPS**: Ubuntu 20.04+ / Debian 11+ / RHEL 8+
- **RAM**: 2GB minimum (4GB+ recommended)
- **Docker**: 20.10+
- **Docker Compose**: 2.0+
- **Cloudflare Tunnel**: Configured and running

## Quick Start

### 1. Install Docker

```bash
# Install Docker
curl -fsSL https://get.docker.com | sh

# Add user to docker group (optional, to run without sudo)
sudo usermod -aG docker $USER
newgrp docker
```

### 2. Clone Repository

```bash
git clone <your-repo> /opt/geoconsole
cd /opt/geoconsole
```

### 3. Deploy

```bash
chmod +x deployment/docker-deploy.sh
./deployment/docker-deploy.sh
```

This will:
1. Build frontend (static files)
2. Build backend Docker image
3. Start the backend container
4. Verify health

### 4. Verify

```bash
# Check status
docker compose ps

# Test health endpoint
curl http://localhost:3003/health

# View logs
docker compose logs -f backend
```

Expected response:
```json
{"status":"healthy","service":"geoconsole-v3","version":"0.1.0"}
```

## Cloudflare Tunnel Configuration

Update your Cloudflare Tunnel route:

```
map.oratagroup.net → http://127.0.0.1:3003
```

**Important:** Set COOP/COEP headers in Cloudflare Transform Rules:
- `Cross-Origin-Opener-Policy: same-origin`
- `Cross-Origin-Embedder-Policy: require-corp`

See main DEPLOYMENT.md for Cloudflare setup details.

## Manual Deployment Steps

### Build Frontend

```bash
docker compose --profile build run --rm frontend-builder
```

Output: `./frontend/dist/`

### Build Backend

```bash
docker compose build backend
```

### Start Services

```bash
# Start in background
docker compose up -d backend

# Start with logs
docker compose up backend
```

### Stop Services

```bash
docker compose down
```

## Service Management

### View Logs

```bash
# Follow logs
docker compose logs -f backend

# Last 100 lines
docker compose logs backend --tail 100
```

### Restart Service

```bash
docker compose restart backend
```

### Update Application

```bash
# Pull latest code
git pull

# Rebuild and restart
./deployment/docker-deploy.sh
```

### Shell Access

```bash
# Access container shell
docker compose exec backend sh

# Or as root
docker compose exec -u root backend sh
```

## Data Persistence

Data is stored in Docker volumes and host directories:

```
./data/                    → /app/data (DuckDB database)
./frontend/dist/           → /app/frontend/dist (static files)
```

### Backup Database

```bash
# Backup DuckDB file
cp ./data/geoconsole.duckdb ./data/geoconsole.duckdb.backup-$(date +%Y%m%d)

# Or use Docker volume backup
docker run --rm -v geoconsole_data:/data -v $(pwd):/backup \
  alpine tar czf /backup/data-backup-$(date +%Y%m%d).tar.gz /data
```

## Environment Variables

Edit `docker-compose.yml` to change environment variables:

```yaml
environment:
  - RUST_LOG=info,geoconsole_v3=debug  # Log level
  - HOST=0.0.0.0                        # Bind address
  - PORT=3003                           # Port
  - STATIC_DIR=/app/frontend/dist       # Frontend path
```

After changes:
```bash
docker compose up -d backend
```

## Port Configuration

Default: Backend binds to `127.0.0.1:3003` on host (localhost only).

To expose on all interfaces:
```yaml
ports:
  - "3003:3003"  # Instead of "127.0.0.1:3003:3003"
```

**Security:** Keep `127.0.0.1:3003:3003` unless you need direct external access.

## Troubleshooting

### Container won't start

```bash
# Check logs
docker compose logs backend

# Check if port is in use
sudo ss -tlnp | grep 3003

# Rebuild from scratch
docker compose down
docker compose build --no-cache backend
docker compose up -d backend
```

### Frontend not loading

```bash
# Verify dist directory exists
ls -la frontend/dist/

# Rebuild frontend
docker compose --profile build run --rm frontend-builder

# Restart backend
docker compose restart backend
```

### Database errors

```bash
# Check data directory permissions
ls -la data/

# Reset database (WARNING: deletes all data)
rm -rf data/geoconsole.duckdb
docker compose restart backend
```

### Memory issues

```bash
# Check container memory usage
docker stats geoconsole-backend

# Limit memory in docker-compose.yml
services:
  backend:
    deploy:
      resources:
        limits:
          memory: 2G
```

## Updating

### Update Code

```bash
cd /opt/geoconsole
git pull
./deployment/docker-deploy.sh
```

### Update Docker Images

```bash
# Pull base images
docker compose pull

# Rebuild
docker compose build --no-cache backend
docker compose up -d backend
```

## Monitoring

### Health Check

```bash
# Manual check
curl http://localhost:3003/health

# Docker health status
docker compose ps
```

### Resource Usage

```bash
# Real-time stats
docker stats geoconsole-backend

# Disk usage
docker system df
```

### Logs

```bash
# Follow logs
docker compose logs -f backend

# Export logs
docker compose logs backend > logs-$(date +%Y%m%d).txt
```

## Security

### Container Security

The container runs as non-root user `geoconsole`:
- No privileged access
- Read-only filesystem (except /app/data)
- Minimal base image (Debian slim)

### Network Security

- Backend binds to `127.0.0.1:3003` (localhost only)
- Access only via Cloudflare Tunnel
- No direct external exposure

### Firewall (UFW)

```bash
# Only allow SSH
sudo ufw allow 22/tcp
sudo ufw enable

# Port 3003 is not exposed externally
```

## Performance Tuning

### Build Optimization

```bash
# Use BuildKit for faster builds
DOCKER_BUILDKIT=1 docker compose build backend
```

### Resource Limits

```yaml
services:
  backend:
    deploy:
      resources:
        limits:
          cpus: '2'
          memory: 4G
        reservations:
          memory: 1G
```

## Uninstall

```bash
# Stop and remove containers
docker compose down

# Remove volumes (WARNING: deletes data)
docker compose down -v

# Remove images
docker rmi geoconsole-backend geoconsole-frontend-builder

# Remove project directory
sudo rm -rf /opt/geoconsole
```

## Support

**Logs:** `docker compose logs -f backend`  
**Health:** `curl http://localhost:3003/health`  
**Status:** `docker compose ps`
