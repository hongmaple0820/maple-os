# Phase 5: 生产部署 — 技术设计规格说明书

> 日期: 2026-05-26
> 优先级: P2

---

## 1. Docker 前端打包与 Nginx 反代

### 1.1 多阶段 Dockerfile 改造

当前 Dockerfile 仅构建 Rust 后端二进制。改造为三阶段构建，包含前端和 Nginx:

```dockerfile
# infra/docker/Dockerfile

# ===== Stage 1: 构建前端 =====
FROM node:22-slim AS frontend-builder
WORKDIR /app
RUN corepack enable && corepack prepare pnpm@latest --activate
COPY pnpm-lock.yaml pnpm-workspace.yaml turbo.json package.json ./
COPY apps/web/package.json apps/web/package.json
COPY packages/ui/package.json packages/ui/package.json
COPY packages/sdk/package.json packages/sdk/package.json
COPY packages/config/package.json packages/config/package.json
RUN pnpm install --frozen-lockfile
COPY apps/web/ apps/web/
COPY packages/ packages/
RUN pnpm run build:shared && pnpm run build:web

# ===== Stage 2: 构建 Rust 后端 =====
FROM rust:latest-slim AS backend-builder
RUN apt-get update && apt-get install -y pkg-config libsqlite3-dev && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY core/ core/
COPY server/ server/
COPY migrations/ migrations/
# 空壳编译缓存依赖
RUN mkdir src && echo "fn main(){}" > src/main.rs && cargo build --release -p mapleos-server && rm -rf src
# 全量编译
COPY server/src/ server/src/
RUN touch server/src/main.rs && cargo build --release -p mapleos-server

# ===== Stage 3: 运行镜像 =====
FROM debian:bookworm-slim AS runtime
RUN apt-get update && apt-get install -y nginx libsqlite3-0 curl nodejs && rm -rf /var/lib/apt/lists/*

# 复制后端二进制
COPY --from=backend-builder /app/target/release/mapleos-server /usr/local/bin/mapleos-server

# 复制前端静态文件
COPY --from=frontend-builder /app/apps/web/out /var/www/mapleos

# 复制 migrations
COPY migrations/ /app/migrations/

# 复制 Nginx 配置
COPY infra/docker/nginx.conf /etc/nginx/sites-available/mapleos
RUN ln -s /etc/nginx/sites-available/mapleos /etc/nginx/sites-enabled/mapleos && rm -f /etc/nginx/sites-enabled/default

# 复制 SCALE bridge
COPY core/scale-engine/bridge-http.mjs /app/bridge-http.mjs

# 复制启动脚本
COPY infra/docker/entrypoint.sh /app/entrypoint.sh
RUN chmod +x /app/entrypoint.sh

# 数据目录
VOLUME /app/data

EXPOSE 80

ENTRYPOINT ["/app/entrypoint.sh"]
```

### 1.2 Nginx 配置

```nginx
# infra/docker/nginx.conf

upstream backend {
    server 127.0.0.1:7788;
}

upstream scale {
    server 127.0.0.1:7790;
}

server {
    listen 80;
    server_name _;

    # 前端静态文件
    root /var/www/mapleos;
    index index.html;

    # SPA 路由: 所有非 /api 路径返回 index.html
    location / {
        try_files $uri $uri/ /index.html;
    }

    # 后端 REST API 反代
    location /api/ {
        proxy_pass http://backend;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
    }

    # JSON-RPC 端点
    location /rpc {
        proxy_pass http://backend;
        proxy_set_header Host $host;
    }

    # SCALE Engine API 反代
    location /api/scale/ {
        rewrite ^/api/scale/(.*) /$1 break;
        proxy_pass http://scale;
        proxy_set_header Host $host;
    }

    # SSE 事件流 (禁用 buffering)
    location /api/events {
        proxy_pass http://backend;
        proxy_set_header Connection '';
        proxy_http_version 1.1;
        chunked_transfer_encoding off;
        proxy_buffering off;
        proxy_cache off;
        proxy_read_timeout 86400s;
    }

    # Chat SSE 流式 (禁用 buffering)
    location /api/chat/stream {
        proxy_pass http://backend;
        proxy_set_header Connection '';
        proxy_http_version 1.1;
        chunked_transfer_encoding off;
        proxy_buffering off;
        proxy_cache off;
    }

    # WebSocket Agent 连接
    location /ws/agents {
        proxy_pass http://backend;
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
        proxy_set_header Host $host;
        proxy_read_timeout 86400s;
    }

    # 健康检查
    location /health {
        proxy_pass http://backend;
    }
}
```

### 1.3 启动脚本

```bash
# infra/docker/entrypoint.sh

#!/bin/bash
set -e

# 启动 Rust 后端
mapleos-server &
BACKEND_PID=$!

# 启动 SCALE bridge
node /app/bridge-http.mjs &
SCALE_PID=$!

# 启动 Nginx
nginx -g 'daemon off;' &
NGINX_PID=$!

# 等待后端就绪
until curl -s http://127.0.0.1:7788/health > /dev/null 2>&1; do
  echo "Waiting for backend..."
  sleep 2
done

echo "MapleOS is ready on port 80"

# 容器停止时清理
trap "kill $BACKEND_PID $SCALE_PID $NGINX_PID" EXIT

# 保持运行
wait
```

---

## 2. docker-compose 完整服务栈

```yaml
# infra/docker/docker-compose.yml (改造版)

version: "3.8"

services:
  mapleos:
    build:
      context: ../..
      dockerfile: infra/docker/Dockerfile
    ports:
      - "${MAPLEOS_PORT:-80}:80"
    volumes:
      - mapleos-data:/app/data
    environment:
      - RUST_LOG=${RUST_LOG:-info}
      - DATABASE_URL=sqlite:/app/data/mapleos.db?mode=rwc
      - JWT_SECRET=${JWT_SECRET:-change-me-in-production}
      - QDRANT_URL=http://qdrant:6333
      - SEARCH_API_KEY=${SEARCH_API_KEY:-}
      - SEARCH_ENGINE_ID=${SEARCH_ENGINE_ID:-}
    healthcheck:
      test: ["CMD", "curl", "-f", "http://127.0.0.1:7788/health"]
      interval: 30s
      timeout: 10s
      retries: 3
      start_period: 30s
    depends_on:
      qdrant:
        condition: service_healthy
    restart: unless-stopped

  qdrant:
    image: qdrant/qdrant:latest
    ports:
      - "${QDRANT_PORT:-6333}:6333"
    volumes:
      - qdrant-data:/qdrant/storage
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:6333/health"]
      interval: 15s
      timeout: 5s
      retries: 3
    restart: unless-stopped

  # 可选: Ollama 本地推理 (需要 GPU 或 CPU 模式)
  ollama:
    image: ollama/ollama:latest
    ports:
      - "${OLLAMA_PORT:-11434}:11434"
    volumes:
      - ollama-data:/root/.ollama
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:11434/api/tags"]
      interval: 30s
      timeout: 10s
      retries: 3
    restart: unless-stopped
    profiles:
      - ollama

volumes:
  mapleos-data:
  qdrant-data:
  ollama-data:
```

---

## 3. 项目文档

### 3.1 架构文档 (`docs/architecture.md`)

```markdown
# MapleOS 架构设计

## 六层架构

┌─────────────────────────────────────────────────────┐
│  L1 Interface  │ Web / Desktop / Mobile / CLI       │
├─────────────────┼───────────────────────────────────┤
│  L2 Collaboration│ Workspace / Permissions / Sync   │
├─────────────────┼───────────────────────────────────┤
│  L3 Orchestration│ Workflow / Event Bus / Agent编排  │
├─────────────────┼───────────────────────────────────┤
│  L4 Capabilities │ Skills / MCP / Browser / Code    │
├─────────────────┼───────────────────────────────────┤
│  L5 Intelligence │ LLM Router / KB / Memory / 进化  │
├─────────────────┼───────────────────────────────────┤
│  L6 Storage     │ SQLite / Qdrant / CRDT / WebDAV   │
└─────────────────────────────────────────────────────┘

## 核心 Crate 交互图

(Mermaid 图: 8 个 crate 之间的调用关系)

## API 层设计

### REST API (50+ 端点)
(按模块分组的端点列表)

### JSON-RPC 2.0 (19 方法)
(方法列表)

### SSE 事件流
(事件类型列表)

### WebSocket 消息格式
(消息类型列表)
```

### 3.2 API 参考文档 (`docs/api-reference.md`)

按模块分组，每个端点包含:
- 路径和方法
- 请求参数 (含类型和是否必填)
- 响应格式 (JSON schema)
- 错误码和含义
- 示例请求和响应

### 3.3 部署指南 (`docs/deployment-guide.md`)

```markdown
# MapleOS 部署指南

## 快速启动

```bash
# 1. 克隆仓库
git clone https://github.com/hongmaple0820/maple-os.git
cd maple-os

# 2. 配置环境变量
cp infra/docker/.env.example infra/docker/.env
# 编辑 .env 填写 JWT_SECRET 等

# 3. 启动服务
docker-compose up -d

# 4. 访问
# http://localhost
```

## 环境变量清单
(变量列表)

## LLM 配置
(Ollama 部署步骤 / 云端 API 配置)

## 数据备份
(SQLite 备份脚本)

## 常见问题排查
(问题列表和解决方案)
```

---

## 4. Automerge CRDT 替换

### 4.1 当前自定义 CRDT 实现

`core/maple-sync/src/crdt.rs` (169行):
```rust
// 当前: 简单的 JSON merge, 3 种策略
pub enum MergeStrategy {
    LastWriteWins,      // 最近的修改覆盖旧的
    MergeArrays,        // 数组: 合并去重
    MergeObjects,       // 对象: 递归合并,冲突取 last-write
}
```

问题: 无法处理复杂的并发编辑冲突(如同一字段被两人同时修改为不同值)。

### 4.2 Automerge CRDT 替换方案

```rust
// Cargo.toml 已有: automerge = "0.5"
// 当前未使用, 需要 import 并替换

use automerge::{AutoCommitDoc, ObjId, Value, Transaction};

pub struct AutomergeCrdtManager {
    doc: AutoCommitDoc,
}

impl AutomergeCrdtManager {
    pub fn new() -> Self {
        Self { doc: AutoCommitDoc::new() }
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        let doc = AutoCommitDoc::load(data)?;
        Ok(Self { doc })
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        self.doc.save()
    }

    // 写入数据
    pub fn put(&mut self, path: &str, value: impl Into<Value>) -> Result<()> {
        let mut tx = self.doc.transaction();
        // 解析 path, 逐层导航到目标对象
        // tx.put(obj, key, value)
        tx.commit();
        Ok(())
    }

    // 读取数据
    pub fn get(&self, path: &str) -> Result<Option<Value>> {
        let obj = self.navigate_path(path)?;
        self.doc.get(obj, path.split('/').last().unwrap())?
    }

    // 合并远程修改
    pub fn merge(&mut self, remote: &[u8]) -> Result<()> {
        let remote_doc = AutoCommitDoc::load(remote)?;
        self.doc.merge(&remote_doc)?;
        Ok(())
    }
}
```

### 4.3 SyncEngine 改造

```rust
// core/maple-sync/src/sync_engine.rs 改造

pub struct SyncEngine {
    webdav: WebDavClient,
    crdt: AutomergeCrdtManager,  // 替换 CrdtManager
}

impl SyncEngine {
    pub async fn full_sync(&mut self) -> Result<()> {
        // 1. 从 WebDAV 拉取远程 CRDT state
        let remote_data = self.webdav.get("mapleos-state.bin")?;

        // 2. Automerge merge (自动合并冲突)
        self.crdt.merge(&remote_data)?;

        // 3. 推送合并后的 state 到 WebDAV
        let local_data = self.crdt.to_bytes();
        self.webdav.put("mapleos-state.bin", &local_data)?;

        Ok(())
    }

    pub async fn push(&mut self) -> Result<()> {
        // 推送本地 CRDT state 到 WebDAV
        let data = self.crdt.to_bytes();
        self.webdav.put("mapleos-state.bin", &data)?;
        Ok(())
    }

    pub async fn pull(&mut self) -> Result<()> {
        // 拉取远程 CRDT state 并合并
        let remote_data = self.webdav.get("mapleos-state.bin")?;
        self.crdt.merge(&remote_data)?;
        Ok(())
    }
}
```

### 4.4 数据类型映射

| MapleOS 数据 | Automerge 存储 | 说明 |
|-------------|---------------|------|
| Agent 配置 | Automerge map | id, name, capabilities, triggers |
| Workflow 定义 | Automerge map | nodes 数组, edges 数组 |
| Workspace 设置 | Automerge map | name, rules, members |
| kv_store 配置 | Automerge map | config.* 键值 |
| Memories | Automerge list | 工作记忆/情景记忆/语义记忆 |

---

## 5. 文件变更清单

| # | 文件 | 操作 | 说明 |
|---|------|------|------|
| 5.1 | `infra/docker/Dockerfile` | 改造 | 三阶段构建(前端+Nginx+后端) |
| 5.2 | `infra/docker/nginx.conf` | 新增 | Nginx 反代配置 |
| 5.3 | `infra/docker/entrypoint.sh` | 新增 | 启动脚本(后端+SCALE+Nginx) |
| 5.4 | `infra/docker/docker-compose.yml` | 改造 | 完整服务栈+健康检查 |
| 5.5 | `infra/docker/.env.example` | 新增 | 环境变量示例 |
| 5.6 | `docs/architecture.md` | 新增 | 架构设计文档 |
| 5.7 | `docs/api-reference.md` | 新增 | API 参考文档 |
| 5.8 | `docs/deployment-guide.md` | 新增 | 部署指南 |
| 5.9 | `core/maple-sync/src/crdt.rs` | 改造 | 替换为 Automerge CrdtManager |
| 5.10 | `core/maple-sync/src/sync_engine.rs` | 改造 | 使用 Automerge merge 替换自定义 merge |

---

## 6. 风险与应对

| 风险 | 影响 | 应对策略 |
|------|------|---------|
| 前端静态文件 Next.js SSR 模式不支持 `out` 导出 | Docker 需要运行 Next.js server 而非 Nginx 静态 | Next.js 15 支持 `output: 'export'` 静态导出,已在 next.config.js 中配置 |
| Automerge Rust crate 与项目 Rust edition 2024 兼容 | 编译失败 | automerge 0.5 支持 Rust 1.70+,edition 2024 需要 1.85+,需验证兼容性 |
| Nginx SSE buffering 配置错误 | Chat 流式输出被 Nginx 缓冲,延迟推送 | `proxy_buffering off` + `proxy_cache off` + `chunked_transfer_encoding off` |
| Automerge merge 性能 | 大量数据时合并慢 | Automerge 基于 RGA 算法,O(n) 复杂度,控制在单次 merge < 1s |
| Ollama GPU 不可用 | 本地 LLM 推理速度慢 | docker-compose 中 Ollama 为可选服务(profiles: ollama),CPU 模式可用但慢 |