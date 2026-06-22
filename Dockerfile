# ---- Stage 1: Build ----
FROM rust:1.85-slim-bookworm AS builder

RUN apt-get update && apt-get install -y --no-install-recommends \
    cmake clang protobuf-compiler pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /usr/src/trytet
COPY . .

# Build the tet binary in release mode
RUN cargo build --release --bin tet

# ---- Stage 2: Runtime ----
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates libssl3 \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /usr/src/trytet/target/release/tet /usr/local/bin/tet

EXPOSE 3000
ENTRYPOINT ["tet"]
CMD ["mcp"]
