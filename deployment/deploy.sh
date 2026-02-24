#!/bin/bash
set -e

# GeoConsole V3 Deployment Script for VPS
# This script deploys the application to /opt/geoconsole

DEPLOY_USER="geoconsole"
DEPLOY_DIR="/opt/geoconsole"
BINARY_NAME="geoconsole-v3"

echo "=== GeoConsole V3 Deployment ==="

# Check if running as root
if [ "$EUID" -ne 0 ]; then 
    echo "Please run as root (sudo)"
    exit 1
fi

# Create user if doesn't exist
if ! id "$DEPLOY_USER" &>/dev/null; then
    echo "Creating user $DEPLOY_USER..."
    useradd -r -s /bin/false -d "$DEPLOY_DIR" "$DEPLOY_USER"
fi

# Create directories
echo "Creating directories..."
mkdir -p "$DEPLOY_DIR"/{data,frontend/dist}
chown -R "$DEPLOY_USER:$DEPLOY_USER" "$DEPLOY_DIR"

# Copy binary
if [ -f "./target/release/$BINARY_NAME" ]; then
    echo "Copying backend binary..."
    cp "./target/release/$BINARY_NAME" "$DEPLOY_DIR/"
    chmod +x "$DEPLOY_DIR/$BINARY_NAME"
    chown "$DEPLOY_USER:$DEPLOY_USER" "$DEPLOY_DIR/$BINARY_NAME"
else
    echo "ERROR: Binary not found at ./target/release/$BINARY_NAME"
    echo "Run 'cargo build --release' first"
    exit 1
fi

# Copy frontend
if [ -d "./frontend/dist" ]; then
    echo "Copying frontend files..."
    cp -r ./frontend/dist/* "$DEPLOY_DIR/frontend/dist/"
    chown -R "$DEPLOY_USER:$DEPLOY_USER" "$DEPLOY_DIR/frontend/dist"
else
    echo "WARNING: Frontend dist not found. Run './deployment/build-frontend.sh' first"
fi

# Install systemd service
echo "Installing systemd service..."
cp ./deployment/geoconsole.service /etc/systemd/system/
systemctl daemon-reload

# Enable and start service
echo "Starting service..."
systemctl enable geoconsole
systemctl restart geoconsole

# Show status
sleep 2
systemctl status geoconsole --no-pager

echo ""
echo "=== Deployment Complete ==="
echo "Service: systemctl status geoconsole"
echo "Logs: journalctl -u geoconsole -f"
echo "Health: curl http://localhost:3003/health"
echo ""
echo "Configure Cloudflare to point to this VPS on port 3003"
echo "Ensure Cloudflare passes through COOP/COEP headers"
