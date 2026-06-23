# Phase 5: 生产部署 — 需求规格说明书

> 日期: 2026-05-26
> 优先级: P2
> 目标: 完善生产部署配置、补全项目文档、替换 CRDT 实现

---

## 1. Docker 前端打包与 Nginx 反代 (MUST)

### 1.1 一键部署

**EARS 模式**: 当运维人员执行 `docker-compose up` 时，系统应一键启动包含 Rust 后端、Web 前端、SCALE bridge 和可选 Qdrant 的完整服务栈，用户通过浏览器访问即可使用全部功能。

**需求明细**:
- 当前状态: Dockerfile 仅打包 Rust 后端二进制，不含前端静态文件
- 需求: Docker 镜像包含前端静态文件 + Nginx 反代
- 服务栈: Rust 后端(7788) + Nginx(80/443 反代前端静态+后端API) + SCALE bridge(7790) + Qdrant(可选6333)
- 端口映射: 宿主机 80 → Nginx → 前端静态 + /api → 后端 7788 + /api/scale → 7790

### 1.2 Nginx 配置

**EARS 模式**: 当用户通过浏览器访问 MapleOS 时，系统应通过 Nginx 反向代理统一入口，前端静态页面和后端 API 均通过同一域名和端口访问。

**需求明细**:
- 前端静态: `/` → Nginx 直接返回 HTML/JS/CSS
- 后端 API: `/api/*` → Nginx proxy_pass 到 Rust 后端 7788
- RPC 端点: `/rpc` → proxy_pass 到 7788
- SCALE API: `/api/scale/*` → proxy_pass 到 7790
- WebSocket: `/ws/agents` → proxy_pass 到 7788 (ws 协议)
- SSE: `/api/events` → proxy_pass 到 7788 (需禁用 buffering)
- SSL: 可选 HTTPS (证书由运维配置)

---

## 2. docker-compose 完整服务栈 (MUST)

### 2.1 完整编排

**EARS 模式**: 当运维人员通过 docker-compose 部署 MapleOS 时，系统应提供完整的编排文件，包含所有必需和可选服务。

**需求明细**:
- 必需服务: mapleos-app(前端+Nginx+后端+SCALE)、Qdrant(可选)
- 数据持久化: mapleos-data(SQLite 文件)、qdrant-data(向量数据)
- 环境变量: JWT_SECRET、DATABASE_URL、RUST_LOG、QDRANT_URL
- 健康检查: 各服务配置 healthcheck,确保可用性
- Ollama: 可选本地推理服务(需 GPU 或 CPU 模式)

---

## 3. 项目文档 (SHOULD)

### 3.1 架构文档

**EARS 模式**: 当开发者首次接触 MapleOS 项目时，系统应在 docs/ 目录提供架构设计文档，帮助开发者理解项目整体结构和模块职责。

**需求明细**:
- 六层架构图: Interface → Collaboration → Orchestration → Capabilities → Intelligence → Storage
- 8 个核心 crate 的职责和交互关系
- API 层设计: REST + JSON-RPC 双接口
- 数据流: 用户请求 → 前端 → Nginx → 后端 → DB/LLM/KB → 响应
- 当前状态: docs/ 目录空壳(仅含社区二维码图片)

### 3.2 API 参考文档

**EARS 模式**: 当前端开发者需要调用 MapleOS API 时，系统应在 docs/ 目录提供 API 参考文档，列出所有 REST 端点和 JSON-RPC 方法的参数与返回值。

**需求明细**:
- REST API: 50+ 端点的路径、方法、参数、返回值、错误码
- JSON-RPC: 19 个方法的参数、返回值、错误码
- SSE 事件: 事件类型和 payload 格式
- WebSocket 消息格式

### 3.3 部署指南

**EARS 模式**: 当运维人员需要部署 MapleOS 时，系统应在 docs/ 目录提供部署指南，覆盖 Docker 部署、环境变量配置和常见问题排查。

**需求明细**:
- Docker 部署: docker-compose up 步骤
- 环境变量清单: 必需和可选变量
- LLM 配置: Ollama 本地部署 / 云端 API Key 配置
- 数据备份: SQLite 文件备份策略
- 常见问题: 端口冲突、权限问题、LLM 连接失败

---

## 4. Automerge CRDT 替换 (SHOULD)

### 4.1 真正的离线协同

**EARS 模式**: 当多个用户在不同设备上同时编辑 MapleOS 数据时，系统应使用 Automerge CRDT 自动合并修改，冲突自动解决，无需手动干预。

**需求明细**:
- 当前状态: Cargo.toml 有 `automerge` 依赖，但从未 import/使用
- 自定义 CRDT: `maple-sync/src/crdt.rs` 实现了简单的 JSON merge (3策略: last-write-wins/merge-arrays/merge-objects)
- 需求: 替换自定义 CRDT 为 Automerge 真正的 CRDT 实现
- 同步场景: Agent 配置、Workflow 定义、Workspace 数据
- 冲突解决: Automerge 自动合并，不丢失任何一方修改

---

## 验收标准

1. `docker-compose up` 一键启动完整服务栈，浏览器访问可正常使用
2. Nginx 正确反代前端静态 + 后端 API + SCALE + WebSocket + SSE
3. docs/ 目录包含架构文档、API 参考文档、部署指南
4. Automerge CRDT 替换自定义 merge，冲突自动合并测试通过
5. 服务健康检查配置，容器异常时自动重启