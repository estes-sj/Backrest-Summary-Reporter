# 1. Builder stage – use a Rust 1.82+ image
FROM rust:1.82 as builder

WORKDIR /app/rust-server
COPY rust-server/Cargo.toml .
COPY rust-server/src ./src

RUN cargo build --release

# 2. Runtime stage
FROM debian:bookworm-slim

RUN apt-get update \
 && apt-get install -y ca-certificates libpq-dev \
 && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=builder /app/rust-server/target/release/rust-server .

# Ensure the log file exists
RUN touch /app/Backrest_Listener.log

# If you have a .env file:
#COPY rust-server/.env .env

CMD ["./rust-server"]
