# Frontend build-only Dockerfile
# This container builds the Svelte frontend and exits
# The built files are copied out to the host for the Rust backend to serve

FROM node:20-alpine AS builder

WORKDIR /app

# Copy package files
COPY package*.json ./

# Install dependencies
RUN npm ci

# Copy source files
COPY . .

# Build for production
RUN npm run build

# Output is in /app/dist/
# Use docker cp to extract: docker cp <container>:/app/dist ./dist
