# Stage 1: Build
FROM rust:1.77 AS builder

# Put ourselves into /app/rust-server, where your Cargo.toml lives
WORKDIR /app/rust-server

# Copy only the Cargo.toml and source into that folder
COPY rust-server/Cargo.toml .
COPY rust-server/src ./src

# Build the release binary
RUN cargo build --release

# Stage 2: Runtime
FROM debian:bookworm-slim

# Install ca-certificates (for any HTTPS calls)
RUN apt-get update \
 && apt-get install -y ca-certificates \
 && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy the statically built binary out of the builder
COPY --from=builder /app/rust-server/target/release/rust-server .

# Create the log file so the volume mount won’t become a folder
RUN touch /app/log.log

CMD ["./rust-server"]
