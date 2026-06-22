# MapleOS Dockerfile with cargo-chef for dependency caching
# Uses 3-stage build: planner → cacher → builder → runtime
# This dramatically reduces rebuild time when only source code changes
# (dependencies are cached and only rebuilt when Cargo.toml changes)

FROM rust:slim AS chef
RUN apt-get update && apt-get install -y pkg-config libsqlite3-dev && rm -rf /var/lib/apt/lists/*
RUN cargo install cargo-chef --locked
WORKDIR /app

FROM chef AS planner
COPY Cargo.toml ./
COPY core/maple-engine/Cargo.toml core/maple-engine/Cargo.toml
COPY core/maple-llm/Cargo.toml core/maple-llm/Cargo.toml
COPY core/maple-agent/Cargo.toml core/maple-agent/Cargo.toml
COPY core/maple-kb/Cargo.toml core/maple-kb/Cargo.toml
COPY core/maple-sync/Cargo.toml core/maple-sync/Cargo.toml
COPY core/maple-gateway/Cargo.toml core/maple-gateway/Cargo.toml
COPY core/maple-collab/Cargo.toml core/maple-collab/Cargo.toml
COPY core/maple-rpc/Cargo.toml core/maple-rpc/Cargo.toml
COPY core/maple-macro/Cargo.toml core/maple-macro/Cargo.toml
COPY apps/cli/Cargo.toml apps/cli/Cargo.toml
COPY server/Cargo.toml server/Cargo.toml
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
# Build dependencies (cached layer — only rebuilds when Cargo.toml changes)
RUN cargo chef cook --release --recipe-path recipe.json
# Now copy source and build the actual binary
COPY . .
RUN cargo build --release -p mapleos-server

# Runtime image — minimal
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates libsqlite3-0 && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/mapleos-server /usr/local/bin/mapleos-server
COPY migrations/ /app/migrations/
COPY config/ /app/config/
EXPOSE 7788
ENV RUST_LOG=info
CMD ["mapleos-server"]
