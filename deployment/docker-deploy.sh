#!/bin/bash
set -e

echo "=== GeoConsole V3 Docker Deployment ==="

# Change to project root
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
cd "$PROJECT_ROOT"

echo "Project root: $PROJECT_ROOT"

# Check if Docker is installed
if ! command -v docker &> /dev/null; then
    echo "ERROR: Docker is not installed"
    echo "Install Docker: curl -fsSL https://get.docker.com | sh"
    exit 1
fi

# Detect docker compose command (v2 uses space, v1 uses hyphen)
if docker compose version &> /dev/null; then
    DOCKER_COMPOSE="docker compose"
elif command -v docker-compose &> /dev/null; then
    DOCKER_COMPOSE="docker-compose"
else
    echo "ERROR: Docker Compose is not installe d"
    exit 1
fi

echo "Using: $DOCKER_COMPOSE"

echo ""
echo "Step 1: Building frontend..."
$DOCKER_COMPOSE --profile build run --rm frontend-builder

echo ""
echo "Step 2: Building backend Docker image..."
$DOCKER_COMPOSE build backend

echo ""
echo "Step 3: Starting backend service..."
$DOCKER_COMPOSE up -d backend

echo ""
echo "Step 4: Waiting for service to be healthy..."
sleep 5

# Check health
if $DOCKER_COMPOSE ps | grep -q "healthy"; then
    echo "✓ Service is healthy"
else
    echo "⚠ Service may not be healthy yet, checking logs..."
    $DOCKER_COMPOSE logs backend --tail 20
fi

echo ""
echo "=== Deployment Complete ==="
echo ""
echo "Service status:"
$DOCKER_COMPOSE ps

echo ""
echo "Test health endpoint:"
echo "  curl http://localhost:3003/health"
echo ""
echo "View logs:"
echo "  $DOCKER_COMPOSE logs -f backend"
echo ""
echo "Stop service:"
echo "  $DOCKER_COMPOSE down"
echo ""
echo "Update Cloudflare Tunnel route to: http://127.0.0.1:3003"
