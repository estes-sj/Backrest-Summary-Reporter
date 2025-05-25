# Builder stage – use a Rust image and install native tools
FROM rust:1.82 AS builder

# Install CMake, C/C++ toolchain, pkg-config, and SSL headers so prost-build can compile Protobuf
RUN apt-get update \
 && apt-get install -y --no-install-recommends \
      cmake \
      build-essential \
      pkg-config \
      libssl-dev \
      tzdata \
 && rm -rf /var/lib/apt/lists/*

WORKDIR /app/rust-server

# Copy only Cargo.toml first to cache dependencies
COPY rust-server/Cargo.toml .

# Fetch and build dependencies (this speeds up rebuilds)
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo build --release
RUN rm -rf src

# Now copy source code
COPY rust-server/src ./src
COPY rust-server/html ./html

# Finally build binary
RUN cargo build --release

# Runtime stage – slim down  
FROM debian:bookworm-slim

# Labels
LABEL org.opencontainers.image.title="Backrest Summary Reporter" \
      org.opencontainers.image.description="A companion service for Backrest Restic that tracks snapshot activity and storage usage. It aggregates backup metadata into a database and sends scheduled email reports summarizing snapshot events and storage statistics." \
      org.opencontainers.image.url="https://github.com/estes-sj/Backrest-Summary-Reporter" \
      org.opencontainers.image.source="https://github.com/estes-sj/Backrest-Summary-Reporter" \
      org.opencontainers.image.licenses="GPL-3.0-or-later" \
      org.opencontainers.image.authors="Samuel Estes <samuel.estes2000@gmail.com>"

RUN apt-get update \
 && apt-get install -y --no-install-recommends \
      ca-certificates \
 && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy only the compiled binary and static assets  
COPY --from=builder /app/rust-server/target/release/rust-server .  
COPY --from=builder /app/rust-server/html ./html

CMD ["./rust-server"]
