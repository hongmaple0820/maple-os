FROM rust:1.95-slim AS builder

WORKDIR /app

COPY Cargo.toml Cargo.lock ./
COPY core/maple-engine/Cargo.toml core/maple-engine/Cargo.toml
COPY core/maple-llm/Cargo.toml core/maple-llm/Cargo.toml
COPY core/maple-agent/Cargo.toml core/maple-agent/Cargo.toml
COPY core/maple-kb/Cargo.toml core/maple-kb/Cargo.toml
COPY core/maple-sync/Cargo.toml core/maple-sync/Cargo.toml
COPY core/maple-gateway/Cargo.toml core/maple-gateway/Cargo.toml
COPY core/maple-collab/Cargo.toml core/maple-collab/Cargo.toml
COPY core/maple-rpc/Cargo.toml core/maple-rpc/Cargo.toml
COPY server/Cargo.toml server/Cargo.toml

RUN mkdir -p core/maple-engine/src && echo "" > core/maple-engine/src/lib.rs && \
    mkdir -p core/maple-llm/src && echo "" > core/maple-llm/src/lib.rs && \
    mkdir -p core/maple-agent/src && echo "" > core/maple-agent/src/lib.rs && \
    mkdir -p core/maple-kb/src && echo "" > core/maple-kb/src/lib.rs && \
    mkdir -p core/maple-sync/src && echo "" > core/maple-sync/src/lib.rs && \
    mkdir -p core/maple-gateway/src && echo "" > core/maple-gateway/src/lib.rs && \
    mkdir -p core/maple-collab/src && echo "" > core/maple-collab/src/lib.rs && \
    mkdir -p core/maple-rpc/src && echo "" > core/maple-rpc/src/lib.rs && \
    mkdir -p server/src && echo "fn main(){}" > server/src/main.rs

RUN cargo build --release -p mapleos-server 2>/dev/null || true

COPY . .

RUN cargo build --release -p mapleos-server

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/mapleos-server /usr/local/bin/mapleos-server
COPY migrations/ /app/migrations/

EXPOSE 7788

CMD ["mapleos-server"]
