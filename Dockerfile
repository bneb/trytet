FROM rust:1.80-slim-bookworm as builder

# Install build dependencies
RUN apt-get update && apt-get install -y pkg-config libssl-dev protobuf-compiler curl && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY . .

# Build the final backend monolith binary
# We only need the backend (not the web workspace target)
RUN cargo build --release --bin tet-core

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates libssl-dev && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=builder /app/target/release/tet-core /app/trytet-api

ENV RUST_LOG="info"
ENV REGISTRY_PATH="/data/registry"
ENV BASE_TET_PATH="/data/base_tets"
ENV DATABASE_URL="sqlite:///data/trytet.db"

CMD ["/app/trytet-api"]
