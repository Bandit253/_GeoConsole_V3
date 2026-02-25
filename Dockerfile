# Multi-stage build for GeoConsole V3 backend
FROM rust:1.75-slim as builder

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    clang \
    mold \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build

# Copy dependency files first for caching
COPY Cargo.toml Cargo.lock ./
COPY .cargo .cargo

# Create dummy main to build dependencies
RUN mkdir src && \
    echo "fn main() {}" > src/main.rs && \
    cargo build --release && \
    rm -rf src

# Copy actual source code
COPY src ./src

# Build the application
RUN cargo build --release

# Runtime stage
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

# Create app user
RUN useradd -r -s /bin/false -d /app geoconsole

WORKDIR /app

# Copy binary from builder
COPY --from=builder /build/target/release/geoconsole-v3 ./geoconsole-v3

# Create data directory
RUN mkdir -p data && chown -R geoconsole:geoconsole /app

USER geoconsole

# Environment variables
ENV RUST_LOG=info,geoconsole_v3=debug
ENV HOST=0.0.0.0
ENV PORT=3003
ENV STATIC_DIR=/app/frontend/dist

EXPOSE 3003

CMD ["./geoconsole-v3"]
