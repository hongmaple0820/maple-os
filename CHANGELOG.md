# Changelog

All notable changes to MapleOS will be documented in this file.

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
