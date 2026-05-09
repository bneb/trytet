FROM rustlang/rust:nightly-slim AS builder

# Install build dependencies
RUN apt-get update && apt-get install -y pkg-config libssl-dev protobuf-compiler curl clang libclang-dev cmake && rm -rf /var/lib/apt/lists/*
RUN rustup component add rustfmt

WORKDIR /app
COPY . .

ENV CC=clang
ENV CXX=clang++

# Build the final backend monolith binary
# We only need the backend (not the web workspace target)
RUN cargo build --release --bin tet-core

FROM debian:sid-slim
RUN apt-get update && apt-get install -y ca-certificates libssl-dev nodejs && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=builder /app/target/release/tet-core /app/trytet-api

ENV RUST_LOG="info"
ENV REGISTRY_PATH="/data/registry"
ENV BASE_TET_PATH="/data/base_tets"
ENV DATABASE_URL="sqlite:///data/trytet.db"

CMD ["/app/trytet-api"]
