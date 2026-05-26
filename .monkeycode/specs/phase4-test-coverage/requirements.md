# Phase 4: 测试覆盖 — 需求规格说明书

> 日期: 2026-05-26
> 优先级: P2
> 目标: 补全 Rust 核心模块单元测试、搭建真实后端 E2E 集成测试框架

---

## 1. Rust 核心模块单元测试 (MUST)

### 1.1 executor 模块测试

**EARS 模式**: 当 WorkflowExecutor 执行工作流 DAG 时，系统应按拓扑排序顺序执行各节点，系统应正确处理 LLM/Tool/Condition/Parallel/Loop 等节点类型。

**需求明细**:
- DAG 拓扑排序: 测试有依赖和无依赖节点的执行顺序
- Condition 节点: 测试分支选择(true/false 两条路径)
- Parallel 节点: 测试并发执行所有子节点
- Loop 节点: 测试迭代循环(含 max_iterations 限制)
- 失败处理: 单节点失败时 workflow 标记为 failed
- 当前覆盖: 0% (executor.rs 无 #[cfg(test)])

### 1.2 react_loop 模块测试

**EARS 模式**: 当 Agent 通过 ReactLoop 进行推理时，系统应在最多 10 轮内完成推理，系统应正确处理 ToolUse/ToolResult 交互。

**需求明细**:
- 多轮推理: ToolUse → ToolResult → 继续推理 → 最终答案
- 轮数限制: 超过 max_iterations 时终止并返回中间结果
- Session 消息持久化: 每轮推理消息写入 DB
- 当前覆盖: 0%

### 1.3 mcp_host 模块测试

**EARS 模式**: 当 McpHostManager 管理 MCP Server 进程时，系统应正确启动/停止 Stdio/HTTP/WebSocket 三种 transport 的 MCP Server。

**需求明细**:
- Stdio transport: 启动进程,发送 JSON-RPC 请求,接收响应
- HTTP transport: 发送 HTTP 请求到 MCP server
- 错误处理: 进程崩溃时自动重启(或标记为不可用)
- 当前覆盖: 0%

### 1.4 其他模块测试

| 模块 | 测试重点 | 当前覆盖 |
|------|---------|---------|
| task_queue | enqueue/dequeue/complete/fail/retry/dead_letter | 0% |
| scheduler | cron 解析/next_run_at 计算/job 执行 | 已有(scheduler.rs) |
| workflow | parse_definition/JSON/YAML 双格式 | 0% |
| router | routing rules 匹配/fallback chain | 0% |
| orchestrator | goal 分解/agent 匹配/结果汇聚 | 0% |
| sync_engine | push/pull/冲突解决(3策略) | 0% |

---

## 2. E2E 真实后端集成测试 (MUST)

### 2.1 从 Mock 迁移到真实后端

**EARS 模式**: 当 E2E 测试运行时，系统应启动真实 Rust 后端服务(端口 7788)和 SCALE bridge(端口 7790)，前端页面与真实 API 交互验证功能闭环。

**需求明细**:
- 当前状态: 12 个 E2E 用例全部基于 Mock API (`page.route()`)
- 需求: 核心流程使用真实后端 API 验证
- 测试环境: CI 中需要先启动 Rust 后端 + Web 前端 + SCALE bridge
- Mock 保留: 外部 API(LLM/Ollama)仍 Mock，内部 API 用真实服务

### 2.2 核心闭环 E2E 场景

| # | 场景 | 验证内容 |
|---|------|---------|
| 4.1 | 注册登录闭环 | 注册新用户 → 登录 → Dashboard 显示指标 |
| 4.2 | Chat SSE 流式 | 选择 Agent → 发消息 → 验证 token 逐字出现 → 验证 done 事件 |
| 4.3 | Workflow 创建执行 | 创建工作流 → 执行 → SSE 事件更新节点状态 → 执行历史 |
| 4.4 | Knowledge 搜索 | 索引文本 → 搜索命中 → Chat 中引用 KB |
| 4.5 | Agent 注册派发 | 注册 Agent → 派发任务 → 查看 tasks/stats |

---

## 验收标准

1. Rust 核心模块(executor/react_loop/task_queue/workflow/router)单元测试覆盖 > 80%
2. E2E 真实后端集成测试 5 个核心闭环场景全部通过
3. CI pipeline: cargo test + pnpm build + E2E 真实后端 全绿