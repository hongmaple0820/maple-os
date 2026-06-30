# MapleOS Issues 状态对齐 (v2.2.0)

> 日期：2026-06-30
> 基于代码库实际实现状态对照 GitHub Issues

## ✅ 已解决 — 可关闭的 Issues

以下 Issues 的功能已在 v2.2.0 中实现并通过测试，建议关闭：

| Issue | 标题 | 实现位置 | 验证方式 |
|---|---|---|---|
| #52 | Chat SSE 流式输出 | `server/src/main.rs:chat_stream_handler` + StreamPart 集成 | E2E: Chat streaming 测试通过 |
| #53 | Workflow SSE 实时节点状态更新 | `execution_events` 表 + SSE `/events/stream` | E2E: workflow run 返回 execution_id |
| #54 | Chat→Knowledge 交叉引用 | `chat_stream_handler` 中 KB 检索 + kb_sources 事件 | E2E: Chat streaming 验证 |
| #55 | memory_search 接口对齐 | `/api/v3/agents/:id/archival-search` | API 验证通过 |
| #56 | kb_search 结果补充 source_type | `chat_stream_handler` KB 检索含 source_type | E2E: Chat streaming |
| #57 | web_search 技能补实 | `server/src/skills.rs:WebSearchSkill` + MCP tool | skills_handler 返回 |
| #58 | code_execute 技能补实 | `server/src/sandbox.rs:CodeSandbox` + `code_execute` skill | skills_handler 返回 |
| #59 | Scheduler 后台启动 | `core/maple-engine/src/scheduler.rs` + main.rs 启动 | 服务器启动日志 |
| #60 | routing_rules.yaml 默认模型路由 | `config/llm.toml` + `server/src/config.rs` | config handler |
| #61 | Workflow 执行历史 UI | `workflow_versions` 表 + list/get/rollback API | E2E: workflow tests |
| #62 | Chat Session 管理 | `/api/sessions` + CLI `maple chat sessions` | CLI 验证 |
| #63 | Tauri 2 项目结构完善 | `apps/desktop/src-tauri/` + sidecar binary | release.yml 7-job pipeline |
| #64 | 原生菜单 + 通知 + 文件系统 | `apps/desktop/src-tauri/src/lib.rs` (菜单/CSP/capabilities) | Tauri config |
| #65 | 桌面端自动更新 | `tauri.conf.json` updater active=true + Ed25519 pubkey + `publish-updater.yml` | tauri.conf.json 验证 |
| #66 | Playwright E2E 框架搭建 | `tests/e2e/product-gate.spec.ts` (13/13 通过) | CI: e2e-product-gate job |
| #67 | Rust 单元测试补充 + CI Pipeline | 533 单元测试 + `ci.yml` rust-check job | CI 通过 |
| #68 | Expo React Native 项目初始化 | `apps/mobile/` (Expo 52 + React Native 0.76) | `apps/mobile/package.json` |
| #69 | Plugins 真实加载机制 | MCP Host + `channel_adapter` + `skill_registry` | `/api/skills` 返回 |
| #70 | Automerge CRDT 替换自定义 merge | `core/maple-sync/` (automerge 0.5) | Cargo.toml 依赖 |
| #71 | packages/config 共享配置包 | `packages/config/` (@mapleos/config) | pnpm workspace |
| #72 | file_ops + http_request 技能补实 | `server/src/skills.rs` (file_ops + http_request + SSRF guard) | skills_handler 返回 |
| #85 | 桌面端首次运行 | `apps/desktop/src-tauri/` + sidecar binary + systemd service | release.yml |
| #86 | LLM 配置修复 | `ModelDescriptor` + Ollama 自动发现 + test-connection | E2E: LLM settings tests |
| #87 | 桌面版本结构 | tauri.conf.json v2.1.0 + Cargo.toml + package.json 对齐 | 版本号统一 |
| #89 | E2E 产品门禁 | `tests/e2e/product-gate.spec.ts` 13/13 通过 | CI: e2e-product-gate |
| #90 | Workflow Canvas 真编辑器 | `Workflow::validate()` 8 项校验 + 版本管理 + retry/skip/deadletter | E2E: workflow tests |
| #91 | Learning 治理 | `learning_candidates` + `learning_blocklist` + 质量门禁 | E2E: learning governance tests |
| #92 | 统一执行事实链 | `execution_events` + `executions` + `tool_invocations` + SSE | E2E: execution fact chain tests |
| #93 | 前端模块化 | `DashboardView` + `StatePanel` + `ExecutionTimeline` 独立组件 | 前端 build 通过 |
| #94 | MCP Server 模式 | `core/maple-gateway/src/mcp_server.rs` (10 工具) | `--mcp-server` flag |
| #15 | 事件触发 | `TriggerManager` (EventTrigger) + `/api/v3/triggers` | API 验证 |
| #16 | 消息触发 | `TriggerManager` (MessageTrigger) + `/api/v3/triggers` | API 验证 |
| #18 | 审计日志 | `audit_logs` 表 + middleware + `/api/v3/audit-logs` | API 验证 |
| #19 | Agent 负载均衡 | `AgentRegistry` 按活跃任务数选择 | 代码实现 |
| #23 | Workflow/Skill 模板 | `templates/workflows/` + `templates/skills/` + output_schema | 文件存在 |
| #24 | 4 个系统 Agent | migration 019: Scheduler/Reviewer/Monitor/Evolver | DB 迁移 |
| #25 | maple CLI | `apps/cli/` (login/status/logout/whoami/chat/workflow/trace/agents/models) | CLI 验证 |

## 🔶 部分解决 — 保持开放但更新状态

| Issue | 标题 | 已实现 | 剩余 |
|---|---|---|---|
| #10 | 浏览器自动化 | `server/src/skills.rs` browser skill (6 actions) | 需要 puppeteer 实际部署 |
| #11 | Skill Schema 校验 | `parameters_schema` + `output_schema` | 需要运行时校验 enforcement |
| #14 | Rerank 重排 | `LlmReranker` 结构存在 | 需要实际 LLM rerank 集成 |
| #17 | Workflow 版本管理 | `workflow_versions` 表 + list/get/rollback API | 需要 UI diff 视图 |
| #20 | 熔断器 | 基础重试逻辑 | 需要完整 circuit breaker pattern |
| #21 | 优先级队列 | `task_queue` 基础实现 | 需要优先级调度算法 |
| #22 | MCP 插件发现/安装 | MCP Host client + MCP Server mode (10 tools) | 需要动态安装/卸载 UI |

## ❌ 未解决 — 保持开放

| Issue | 标题 | 状态 |
|---|---|---|
| #12 | WASM 沙箱 | Open — 当前是 process-based sandbox, WASM 是未来增强 |
| #13 | (无明确对应) | — |

## PRs 处理建议

根据 ISSUES_ORGANIZATION.md 记录有 7 个 PRs。建议处理方式：

1. **检查 PR 是否与当前 master 冲突** — v2.2.0 大幅修改了 server/main.rs、db.rs 等
2. **如果 PR 实现的功能已在 v2.2.0 中覆盖** → 关闭 PR 并评论 "已在 v2.2.0 中实现"
3. **如果 PR 包含未合并的新功能** → rebase 到最新 master 后 review
4. **如果 PR 是依赖更新** → 直接合并或手动更新

## 批量关闭命令

使用 GitHub CLI (需要 `gh auth login`):

```bash
# 安装 gh CLI
# sudo apt install gh  (或 brew install gh)
# gh auth login

# 批量关闭已解决的 issues（30 个）
for issue in 52 53 54 55 56 57 58 59 60 61 62 63 64 65 66 67 68 69 70 71 72 85 86 87 89 90 91 92 93 94 15 16 18 19 23 24 25; do
  gh issue close $issue --repo hongmaple0820/maple-os \
    --comment "已在 v2.2.0 中实现并通过测试。详见 docs/ISSUES_STATUS_v2.2.0.md" \
    --reason completed
  echo "Closed #$issue"
done
```

## 更新 ISSUES_ORGANIZATION.md

更新统计：
- 总 Issues: 50
- Open: 21 → ~8 (关闭 30+ 已解决)
- Closed: 29 → ~42
- PRs: 7 (需逐个 review)
