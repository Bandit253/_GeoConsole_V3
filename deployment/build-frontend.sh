#!/bin/bash
set -e

echo "Building frontend with Docker..."

# Change to project root (parent of deployment/)
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
cd "$PROJECT_ROOT"

echo "Project root: $PROJECT_ROOT"

# Build the Docker image
docker build -t geoconsole-frontend-builder ./frontend

# Create a temporary container
CONTAINER_ID=$(docker create geoconsole-frontend-builder)

# Copy the built files out
docker cp "$CONTAINER_ID:/app/dist" ./frontend/dist

# Clean up
docker rm "$CONTAINER_ID"

echo "Frontend built successfully in ./frontend/dist/"
