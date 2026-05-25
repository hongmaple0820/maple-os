# 用户指令记忆

本文件记录了用户的指令、偏好和教导，用于在未来的交互中提供参考。

## 格式

### 用户指令条目
用户指令条目应遵循以下格式：

[用户指令摘要]
- Date: [YYYY-MM-DD]
- Context: [提及的场景或时间]
- Instructions:
  - [用户教导或指示的内容，逐行描述]

### 项目知识条目
Agent 在任务执行过程中发现的条目应遵循以下格式：

[项目知识摘要]
- Date: [YYYY-MM-DD]
- Context: Agent 在执行 [具体任务描述] 时发现
- Category: [运维部署|构建方法|测试方法|排错调试|工作流协作|环境配置]
- Instructions:
  - [具体的知识点，逐行描述]

## 去重策略
- 添加新条目前，检查是否存在相似或相同的指令
- 若发现重复，跳过新条目或与已有条目合并
- 合并时，更新上下文或日期信息
- 这有助于避免冗余条目，保持记忆文件整洁

## 条目

[MapleOS 项目构建方法]
- Date: 2026-05-21
- Context: Agent 在执行 MapleOS 项目初始化时发现
- Category: 构建方法
- Instructions:
  - Rust 工具链需要通过 rustup 安装，环境变量在 `$HOME/.cargo/env`
  - 项目使用 Cargo workspace 管理多 crate，根 Cargo.toml 定义 workspace
  - 编译验证命令: `source "$HOME/.cargo/env" && cargo check`
  - 运行测试: `source "$HOME/.cargo/env" && cargo test`
  - 前端使用 pnpm monorepo + Turborepo 管理，需要 pnpm 9.15+
  - 核心服务端口: 7788 (Axum JSON-RPC server)
  - 数据库默认: `sqlite:mapleos.db?mode=rwc`，通过 DATABASE_URL 环境变量配置
  - JWT 密钥通过 JWT_SECRET 环境变量配置，默认 `mapleos-dev-secret-change-me`

[MapleOS 项目结构]
- Date: 2026-05-21
- Context: Agent 在执行项目骨架搭建时发现
- Category: 工作流协作
- Instructions:
  - Rust 核心引擎在 core/ 目录下，8 个 crate: maple-engine, maple-llm, maple-agent, maple-kb, maple-sync, maple-gateway, maple-collab, maple-rpc
  - 云端服务在 server/ 目录，server/main.rs 接入所有核心模块并构建 AppState
  - 前端在 apps/ 目录 (desktop, web, mobile)
  - 共享包在 packages/ 目录 (ui, sdk, config)
  - 数据库 migration 在 migrations/ 目录
  - 部署配置在 infra/docker/ 目录
  - Axum Router 状态类型: 需要 state 的路由通过 `.with_state()` 转换，无状态路由直接 merge

[MapleOS 测试方法]
- Date: 2026-05-21
- Context: Agent 在实现 JWT 认证和 HMAC 签名验证后编写单元测试
- Category: 测试方法
- Instructions:
  - Rust 单元测试通过 `cargo test -p <package>` 运行
  - JWT 过期测试：构造 claims 时使用 `exp: now - 3600` 确保令牌已过期，不能用 ttl=0（leeway 容差会导致不报错）
  - HMAC-SHA256 验证使用 hmac + sha2 crate 组合，不自行实现

[MapleOS 递归 async 和 Send 约束]
- Date: 2026-05-22
- Context: Agent 在完善 maple-engine executor 时发现
- Category: 排错调试
- Instructions:
  - Rust 递归 async fn 需要 Box::pin 包装返回类型，否则编译器报 E0733
  - tokio::spawn 要求 Future 实现 Send，如果 LlmRouter 内部持有非 Send 的 trait object，spawn 内不能使用
  - 解决方案：递归方法用 `Pin<Box<dyn Future<Output = Result<T>> + Send + 'a>>` 返回类型
  - 对需要并发的场景，如果 spawn 不可行，可以用顺序执行 + 上下文快照替代

[项目进度管理方式]
- Date: 2026-05-23
- Context: 用户明确要求项目进度通过 GitHub Issues 管理，方便社区共建
- Category: 工作流协作
- Instructions:
  - 项目进度和未实现功能统一用 GitHub Issues 管理，仓库地址: hongmaple0820/maple-os
  - 新功能规划、缺陷修复、技术改进都应先创建 Issue 再开发
  - Issue 按模块分组打标签: [基础设施]、[能力层]、[智能层]、[协作与运维]、[产品化]、[多端]
  - 目标是让外部贡献者也能通过 Issue 参与共建 agent os

[所有技术栈使用最新稳定版]
- Date: 2026-05-25
- Context: 用户明确指示
- Category: 构建方法
- Instructions:
  - 所有技术栈（Rust、Node.js、pnpm、Docker 基础镜像等）统一使用最新稳定版，不固定小版本号
  - CI 工作流中 Rust toolchain 使用 `stable`，Dockerfile 使用 `rust:latest-slim`

[MapleOS CI/CD 和构建依赖]
- Date: 2026-05-25
- Context: Agent 在配置 GitHub Actions CI/CD 时发现
- Category: 构建方法
- Instructions:
  - Rust edition 2024 需要工具链 1.85+，CI 和 Dockerfile 均使用 `stable` 版本（不固定小版本号），避免锁定依赖要求更新 Rust 时反复升级
  - Dockerfile 基础镜像使用 `rust:latest-slim`，运行镜像 `debian:bookworm-slim`
  - reqwest 已切换为 `rustls-tls` (default-features=false)，无需 OpenSSL 开发库
  - SQLx SQLite 编译需要系统安装 `pkg-config` + `libsqlite3-dev` (Ubuntu) / `sqlite3` (macOS)
  - EXPO_TOKEN secret 需要用户在 GitHub repo settings 中配置，mobile 构建在 secret 不存在时自动跳过
  - 前端构建需要先编译 `@mapleos/ui` 和 `@mapleos/sdk` (共享包)，再编译 `mapleos-web`
  - Release 工作流触发条件: push tag `v*`，自动构建 4 平台二进制 + Web 静态包 + Docker 镜像推送 GHCR
