# Deployment Files

This directory contains all files needed to deploy GeoConsole V3 to a VPS.

## Quick Start

```bash
# 1. Build backend (on VPS or cross-compile)
cargo build --release

# 2. Build frontend (requires Docker)
./deployment/build-frontend.sh

# 3. Deploy to VPS
sudo ./deployment/deploy.sh
```

## Files

- **`DEPLOYMENT.md`**: Complete deployment guide with Cloudflare setup
- **`deploy.sh`**: Automated deployment script (run on VPS as root)
- **`build-frontend.sh`**: Builds frontend using Docker
- **`geoconsole.service`**: systemd service unit file

## Architecture

- **Backend**: Native Rust binary at `/opt/geoconsole/geoconsole-v3`
- **Frontend**: Static files at `/opt/geoconsole/frontend/dist/`
- **Database**: DuckDB at `/opt/geoconsole/data/geoconsole.duckdb`
- **Service**: systemd manages the backend process
- **Proxy**: Cloudflare handles TLS and CDN

## Environment Variables

Set in `/etc/systemd/system/geoconsole.service`:

| Variable | Default | Description |
|----------|---------|-------------|
| `HOST` | `0.0.0.0` | Bind address |
| `PORT` | `3003` | Listen port |
| `STATIC_DIR` | `frontend/dist` | Frontend files path |
| `RUST_LOG` | `info,geoconsole_v3=debug` | Log level |

## Service Commands

```bash
sudo systemctl start geoconsole      # Start
sudo systemctl stop geoconsole       # Stop
sudo systemctl restart geoconsole    # Restart
sudo systemctl status geoconsole     # Status
sudo journalctl -u geoconsole -f     # Logs
```

## Cloudflare Setup

**Critical**: Set these headers in Cloudflare Transform Rules:
- `Cross-Origin-Opener-Policy: same-origin`
- `Cross-Origin-Embedder-Policy: require-corp`

Required for DuckDB-WASM `SharedArrayBuffer` support.

See `DEPLOYMENT.md` for detailed Cloudflare configuration.

## Troubleshooting

**Service won't start:**
```bash
sudo journalctl -u geoconsole -n 50
```

**Frontend not loading:**
```bash
ls /opt/geoconsole/frontend/dist/
./deployment/build-frontend.sh
```

**Database issues:**
```bash
sudo -u geoconsole ls -la /opt/geoconsole/data/
```

## Security Notes

- Service runs as unprivileged user `geoconsole`
- Data directory is read-write only for `geoconsole` user
- systemd security hardening enabled (NoNewPrivileges, PrivateTmp, etc.)
- Adjust CORS in `src/main.rs` for production (restrict origins)
