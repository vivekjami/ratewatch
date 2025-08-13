# Build stage - use specific version and Alpine for security
FROM rust:1.82-alpine AS builder

WORKDIR /app

# Install build dependencies for Alpine
RUN apk add --no-cache \
    musl-dev \
    pkgconfig \
    openssl-dev \
    openssl-libs-static

# Copy manifests and source code
COPY Cargo.toml Cargo.lock ./
COPY src ./src

# Build statically linked binary for security and portability
ENV RUSTFLAGS="-C target-feature=+crt-static"
RUN cargo build --release --target x86_64-unknown-linux-musl

# Runtime stage - use distroless for minimal attack surface
FROM gcr.io/distroless/static-debian12:nonroot

WORKDIR /app

# Copy statically linked binary (no dependencies needed)
COPY --from=builder /app/target/x86_64-unknown-linux-musl/release/ratewatch .
COPY static ./static

# Set environment variables
ENV RUST_LOG=info
ENV PORT=8081

EXPOSE 8081

# Distroless runs as nonroot user by default (uid 65532)
# No health check in distroless - handle externally

CMD ["./ratewatch"]
