# ---- Build Stage ----
FROM rust:1.70-bullseye as builder

WORKDIR /app
COPY . .

# Ensure dependencies are installed
RUN apt-get update && apt-get install -y pkg-config libssl-dev

# Build the Rust project
RUN cargo build --release

# ---- Runtime Stage ----
FROM debian:bullseye-slim

# Install only necessary runtime dependencies
RUN apt-get update && apt-get install -y libssl1.1 ca-certificates

WORKDIR /app
COPY --from=builder /app/target/release/my-rust-app .

CMD ["./my-rust-app"]
