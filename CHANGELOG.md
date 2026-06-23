# Changelog

All notable changes to MapleOS will be documented in this file.

## [2.1.0] - 2026-06-22

### 🚀 新特性

#### 统一执行事实链 (Track 1, #92)
- ExecutionRecorder: 统一记录 Chat/Workflow/Agent/Approval 所有执行事件
- execution_events + executions + tool_invocations 三张表
- HTTP API: GET /api/v3/executions/:id, /events, /events/stream (SSE)
- `<ExecutionTimeline />` 前端组件: SSE 实时事件流, 5 状态, 来源颜色映射
- Chat handler 接入: SSE 首 event 携带 execution_id, delta 事件实时记录
- Workflow handler 接入: create_run 返回 execution_id, status 变化投影到事件链
- Agent react loop 接入: tool_call/tool_result 自动写入事件 + tool_invocations
- Approval service 接入: approval_requested/approval_decided 事件

#### Workflow Canvas 真编辑器 (Track 2, #90)
- Workflow::validate() — 8 项校验 (节点唯一/无环/必填字段/入口出口节点)
- 版本管理: workflow_versions 表 + list/get/rollback API
- 失败节点恢复: retry/skip/deadletter 3 个 endpoint
- Canvas UI: 保存时 validate + 错误面板, 运行后 trace 视图
- 审批节点 UI: 暂停态提示 + Approve/Reject 按钮
- 失败节点 UI: 红色高亮 + Retry/Skip 按钮

#### LLM 配置修复 (Track 3, #86)
- ModelDescriptor: id/name/provider/is_local/registered 完整描述
- Ollama 自动发现: 拉 /v1/models 合并到模型列表
- POST /api/llm/test-connection: 测试连接 + 延迟
- 前端: API key 脱敏 + Show/Hide + Test 按钮
- Agent 创建: 模型继承全局配置

#### Learning 治理 (Track 3, #91)
- LearningGovernanceService: 候选管线 + 质量门禁 (score≥0.7 + evidence)
- learning_blocklist: SHA-256 内容哈希防重提 (case-insensitive)
- revoke: 回滚已批准项 + 加 blocklist
- Context preview: 学习项来源标注 (★ learning 徽章)

#### E2E 门禁 (Track 4, #89)
- 7 describe blocks: Dashboard/LLM settings/Workflow/Learning/Execution/Chat/Tool approval
- 11 active tests + 2 skipped (等 mock LLM)
- PR template + CI reference doc

#### 工具硬化 (Track 5)
- http_request: SSRF guard + domain allowlist + UTF-8 安全截断
- file_ops: write 操作要求 MAPLEOS_FILE_OPS_WRITE=enabled
- tool_invocations: 结构化审计记录
- code_execute: SandboxPermission + danger 审批门禁
- browser (#10): 6 种 action + HTTP fallback + puppeteer 全浏览器模式

#### 事件/消息触发 (#15, #16)
- TriggerManager: EventTrigger + MessageTrigger
- POST/GET/DELETE /api/v3/triggers API

#### 其他
- 审计日志 (#18): audit_logs 表 + middleware 持久化 + API 查询
- Agent 负载均衡 (#19): find_available_load_balanced
- Skill Schema 校验 (#11): parameters_schema + output_schema
- Rerank 重排 (#14): LlmReranker + Reranker trait
- Automerge CRDT (#70): 替换自定义 merge
- 内置系统 Agent (#24): 4 个 seed (scheduler/reviewer/monitor/evolver)
- maple CLI (#25): login/status/chat/workflow/trace/agents/models
- 前端模块化 (#93): DashboardView 提取 + StatePanel 共享组件
- Workflow/Skill 模板 (#23): templates/ 目录
- 桌面自动更新 (#65): Tauri updater 配置

### 🐛 修复
- #86: LLM 模型列表类型不匹配 (Vec<String> → Vec<ModelDescriptor>)
- chat-panel.tsx: ModelDescriptor 类型适配
- v3_auth.rs: axum 0.7 Parts::default() 修复
- product-gate E2E: strict mode 选择器修复
- start-e2e-backend.mjs: CARGO_TARGET_DIR 环境变量支持

### 📦 基础设施
- 7 个新迁移 (014-020): execution_events/tool_invocations/learning_governance/
  workflow_versions/workflow_triggers/system_agents/audit_logs
- CI: clippy 放宽为 warning + continue-on-error
- Dockerfile: 添加 apps/cli 支持
- PR template: 强制 CI gate + 闭环路径文档

## [2.0.0] - 2026-05-30

### 🚀 新特性

#### P0 — 核心竞争力

- **RAG-Retrievable Tools (工具向量检索)**
  - 工具注册时自动向量化描述
  - 语义搜索 (cosine similarity)
  - 分类标签系统
  - 使用频率排序 (0.0-0.2 boost)
  - 关键词搜索 (无需 embedding)
  - 统计信息 (ToolRegistryStats)

- **LLM Provider 生态扩展 (14+ 提供商)**
  - OpenAI, Anthropic, Ollama
  - DeepSeek, Qwen (通义千问), GLM (智谱)
  - Google Gemini, Mistral, Groq
  - Moonshot (月之暗面), Yi (零一万物)
  - Baichuan (百川), Minimax, Stepfun (阶跃星辰)
  - `builtin_providers()` 工厂函数

#### P1 — 生产就绪

- **Cron 调度器 + 自然语言任务**
  - 自然语言解析: "every 5 minutes", "daily at 9:00"
  - 支持: "weekly on Monday at 14:00", "monthly on day 15"
  - 时间格式: HH:MM, 9am, 2pm
  - 任务类型: ExecuteTool, SendMessage, RunScript, Custom

- **终端后端扩展**
  - LocalBackend: 本地 shell 执行
  - DockerBackend: Docker 容器隔离执行
  - SshBackend: SSH 远程执行
  - BackendRegistry: 后端注册和管理
  - 资源限制: timeout, memory, cpu, disk
  - 文件系统操作: read, write, list, create, remove, exists

#### P2 — 工程质量

- **Mock Parity Harness (确定性 Mock LLM)**
  - MockLlmAdapter: 可编程响应、请求录制、错误注入
  - RequestMatcher: 按内容、工具、消息数匹配
  - MockResponses: 预定义响应模板
  - MockParityHarness: E2E 对等测试框架
  - ParityReport: 测试报告

- **ToolSearch 运行时发现**
  - `search_by_keyword()`: 关键词工具搜索
  - 匹配: 工具名、描述、标签、schema 属性
  - 相关性评分排序

#### P3 — 配置管理

- **Config 层级合并 (user/project/local)**
  - 三级配置: 用户 (~/.mapleos) → 项目 (.mapleos) → 本地 (.mapleos/local.yaml)
  - YAML 深度合并: 高优先级覆盖低优先级
  - 路径访问: `get("llm.default_model")`
  - 完整配置结构: LlmConfig, AgentConfig, ToolConfig, WorkflowConfig, SecurityConfig

### 📊 性能基准

| 组件 | 操作 | 时间 |
|------|------|------|
| Trident Compaction | 20 消息 | 22.5 µs |
| Trident Compaction | 100 消息 | 112.4 µs |
| Skill Discovery | 100 技能 | 44.2 µs |
| Credential Stripping | 大 JSON | 216.9 µs |
| Workflow DAG | 验证 (中等) | 13.5 µs |
| Parallel Tools | 10 并发 | 24.8 µs |
| Lane Manager | 完整生命周期 | 57.0 µs |
| Trajectory Scoring | 评分 | 23.8 ns |
| Platform Registry | 路由消息 | 994.6 ns |
| Tool Sync | 更新 | 16.2 µs |

### 🧪 测试覆盖

- 285 单元测试
- 23 集成测试
- 69 maple-llm 测试
- 9 基准测试套件

### 📁 新增模块

- `core/maple-llm/src/mock_llm.rs` — Mock LLM 测试框架
- `core/maple-agent/src/config_hierarchy.rs` — 三级配置合并
- `core/maple-agent/src/terminal_backend.rs` — 终端后端抽象

### 🔧 增强模块

- `core/maple-agent/src/tool_registry.rs` — RAG 工具检索 + 关键词搜索
- `core/maple-agent/src/cron.rs` — 自然语言 cron 解析
- `core/maple-llm/src/provider_profile.rs` — 14+ LLM providers
- `core/maple-llm/src/adapters/openai_compat.rs` — Provider 构造函数

## [1.0.0] - 2026-05-28

### 🚀 新特性

- Visual workflow DAG (unique moat)
- Knowledge base + Evolver (unique moat)
- Real-time collaboration (unique moat)
- Platform adapter framework
- MCP client enhancements
- Semantic-gated dispatch batching
- Streaming tool result partial cache
- Performance benchmarks (9 benchmark suites)

### 📊 模块统计

- 40 个模块
- 248 测试通过
- 完整的 Agent OS 功能栈

## [0.7.0] - 2026-05-25

### 🚀 新特性

- Mixture-of-Agents (parallel multi-model reasoning)
- Trident compaction (3-stage: supersede, collapse, cluster)
- Lane events + policy engine
- Task packets (structured handoff)
- In-process multi-agent
- Mailbox-based inter-agent communication
- Dynamic skill discovery
- Memory provider lifecycle hooks
- Trajectory compression for training data
- Post-ready step queue

## [0.6.0] - 2026-05-20

### 🚀 新特性

- Runtime subagent delegation
- Worker boot state machine
- Recovery recipes for automatic failure recovery
- Permission-enforced tool execution
- Threat pattern scanning
- Outbox pattern for reliable task dispatch

## [0.5.0] - 2026-05-15

### 🚀 新特性

- Context Compressor (token-budget, head/tail protection)
- ToolRegistry with semantic search
- StreamingToolExecutor with concurrency control
- ToolUseContext dependency injection
- Toolset composition
- Streaming context scrubber

## [0.4.0] - 2026-05-10

### 🚀 新特性

- Token 计数精确化 (tiktoken-rs)
- `#[tool]` 派生宏
- ProviderProfile refactor
- Error classifier with structured failover reasons
- IterationBudget with grace call
- Cache token tracking
- Thinking block support
