# Multi-stage Containerfile for wpdaemon WebRTC audio server daemon

# ─── 1. Build Stage ───
FROM docker.io/library/rust:1.85-bookworm AS builder
WORKDIR /app

# Install build dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    libclang-dev \
    libasound2-dev \
    libudev-dev \
    && rm -rf /var/lib/apt/lists/*

# Copy workspace files
COPY . .

# Build release binary for wpdaemon
RUN cargo build --release -p wpdaemon

# ─── 2. Runtime Stage ───
FROM docker.io/library/debian:bookworm-slim AS runtime
WORKDIR /app

# Install minimal runtime dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    libasound2 \
    libudev1 \
    && rm -rf /var/lib/apt/lists/*

# Use non-root user for execution
USER nobody:nogroup

# Copy release binary from builder stage
COPY --from=builder --chown=nobody:nogroup /app/target/release/wpdaemon /app/wpdaemon

# Expose HTTP/WebRTC signaling port (15000/tcp) and STUN port (3478/udp)
EXPOSE 15000/tcp
EXPOSE 3478/udp

ENTRYPOINT ["/app/wpdaemon"]
CMD ["--port", "15000", "--stun-port", "3478"]
