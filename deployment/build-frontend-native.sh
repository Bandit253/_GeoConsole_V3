#!/bin/bash
set -e

echo "Building frontend natively (without Docker)..."

# Change to project root
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
cd "$PROJECT_ROOT"

echo "Project root: $PROJECT_ROOT"

# Check if frontend directory exists
if [ ! -d "frontend" ]; then
    echo "ERROR: frontend directory not found at $PROJECT_ROOT/frontend"
    echo "Current directory contents:"
    ls -la
    exit 1
fi

# Install Node.js if not present
if ! command -v node &> /dev/null; then
    echo "Node.js not found. Installing..."
    curl -fsSL https://deb.nodesource.com/setup_20.x | sudo -E bash -
    sudo apt-get install -y nodejs
fi

echo "Node version: $(node --version)"
echo "NPM version: $(npm --version)"

# Build frontend
cd frontend
echo "Installing dependencies..."
npm ci

echo "Building production bundle..."
npm run build

echo ""
echo "Frontend built successfully!"
echo "Output: $PROJECT_ROOT/frontend/dist/"
ls -lh dist/
