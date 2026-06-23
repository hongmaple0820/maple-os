# hermes-agent 深度分析 — Context Compressor + Memory + Curator + IM + Error Classifier

---

## 1. Context Compressor（上下文压缩器）

### Token 预算计算

```
threshold_tokens = max(context_length * 0.50, MINIMUM_CONTEXT_LENGTH)
tail_token_budget = threshold_tokens * summary_target_ratio  // 默认 0.20
max_summary_tokens = min(context_length * 0.05, 12_000)
min_summary_tokens = 2_000
```

### 头尾保护

- **头部**：保护前 3 条消息 + 系统提示（隐式）
- **尾部**：从末尾向前按 token 预算累积，非固定消息数
- 硬下限 3 条尾部消息；软上限 `1.5x tail_token_budget` 避免切割超大消息
- `_ensure_last_user_message_in_tail()` 保证最近用户消息永不被压缩

### 工具输出裁剪（3 遍预处理，无需 LLM）

1. **去重**：MD5 哈希，保留最新完整副本
2. **摘要**：旧工具结果压缩为一行，如 `[terminal] ran 'npm test' -> exit 0, 47 lines output`
3. **截断**：大 JSON 工具参数智能缩小（保持有效 JSON）

### 结构化摘要模板

```markdown
## Active Task       — 原始用户请求（最关键字段）
## Goal
## Constraints & Preferences
## Completed Actions — 编号列表，含工具名+结果
## Active State      — 工作目录、分支、测试状态
## In Progress
## Blocked
## Key Decisions
## Resolved Questions
## Pending User Asks
## Relevant Files
## Remaining Work
## Critical Context
```

### 迭代摘要更新

- 存储 `_previous_summary` 在压缩之间
- 再压缩时：prompt 包含 `PREVIOUS SUMMARY` + `NEW TURNS TO INCORPORATE`
- 指令："保留所有仍相关的现有信息。将新完成的操作添加到编号列表中。将 'In Progress' 中的已完成项移到 'Completed Actions'。"

### 辅助模型选择

- `summary_model_override` 配置或回退到主模型
- 失败时：`_fallback_to_main_for_compression()` 立即在主模型重试
- 冷却期：无 provider 600s，JSON 解码/流式错误 30s，瞬态错误 60s

### 关键常量

```python
_MIN_SUMMARY_TOKENS = 2_000
_SUMMARY_RATIO = 0.20
_SUMMARY_TOKENS_CEILING = 12_000
_CHARS_PER_TOKEN = 4
_IMAGE_TOKEN_ESTIMATE = 1_600
_SUMMARY_FAILURE_COOLDOWN_SECONDS = 600
```

### Rust 移植

```rust
struct ContextCompressor {
    context_length: usize,
    threshold_tokens: usize,      // max(context_length * 0.50, MINIMUM)
    tail_token_budget: usize,     // threshold * 0.20
    max_summary_tokens: usize,    // min(context_length * 0.05, 12_000)
    previous_summary: Option<String>,
    ineffective_count: u8,
}
```

**工作量：2-3 周**

---

## 2. Memory System（记忆系统）

### MemoryProvider 生命周期钩子

```
is_available()         → 配置检查，无网络调用
initialize(session_id) → 连接，创建资源
system_prompt_block()  → 静态文本注入系统提示
prefetch(query)        → 每次 API 调用前的后台召回（必须快速）
queue_prefetch(query)  → 为下一回合排队后台召回
sync_turn(user, asst)  → 持久化已完成回合
get_tool_schemas()     → OpenAI function-calling 格式
handle_tool_call()     → 分发工具调用
shutdown()             → 刷新队列，关闭连接
```

### 可选钩子

- `on_turn_start(turn_number, message)` — 含 `remaining_tokens`, `model`, `platform`, `tool_count`
- `on_session_end(messages)` — 仅在实际会话边界触发
- `on_session_switch(new_id, parent_id)` — 中途会话 ID 轮换
- `on_pre_compress(messages) -> str` — 压缩前提取洞察注入摘要 prompt
- `on_memory_write(action, target, content)` — 镜像内置记忆写入
- `on_delegation(task, result, child_id)` — 父侧观察子 agent 工作

### 关键设计

- 最多一个外部插件 provider（防止工具 schema 膨胀）
- 记忆注入为 `<memory-context>` 围栏块在**消息中**（非系统提示，保留前缀缓存）

### StreamingContextScrubber（流式上下文清洗器）

**状态机**，从流式输出中逐块剥离 `<memory-context>` 围栏：
- 状态：`in_span` (bool), `_buf` (暂存部分标签), `_at_block_boundary`
- `feed(text) -> visible_text`（暂存部分开/关标签）
- `flush()` 在流结束时丢弃未终止的 span
- 仅匹配块边界标签（前面是换行或流开头）

### Rust 移植

```rust
#[async_trait]
trait MemoryProvider: Send + Sync {
    fn name(&self) -> &str;
    fn is_available(&self) -> bool;
    async fn initialize(&mut self, session_id: &str, ctx: &InitContext) -> Result<()>;
    fn system_prompt_block(&self) -> Option<&str>;
    async fn prefetch(&self, query: &str, session_id: &str) -> Result<Option<String>>;
    async fn sync_turn(&self, user: &str, assistant: &str, session_id: &str) -> Result<()>;
    fn tool_schemas(&self) -> Vec<ToolSchema>;
    async fn handle_tool_call(&self, name: &str, args: &JsonValue) -> Result<String>;
    async fn shutdown(&mut self) -> Result<()>;
}

struct StreamingContextScrubber {
    in_span: bool,
    buf: String,
    at_block_boundary: bool,
}
```

**工作量：2-3 周**

---

## 3. Curator / Evolver（技能维护器）

### 技能生命周期

```
active → stale (30 天不活跃) → archived (90 天)
```

- 固定技能绕过所有自动转换
- 仅操作 agent 创建的技能

### 调度机制（非 cron）

```python
def should_run_now():
    return (enabled
            and not paused
            and last_run_at older than interval_hours  # 默认 7 天
            and idle_for >= min_idle_hours)             # 默认 2 小时
```

### 后台审查 Agent

- 生成 forked AIAgent（`max_iterations=9999`, `quiet_mode=True`, `skip_memory=True`）
- 使用辅助客户端（不碰主会话的 prompt cache）
- 在守护线程中运行

### 操作

- **Pin**: 技能绕过转换
- **Archive**: 移到 `~/.hermes/skills/.archive/`（可恢复）
- **Consolidate**: 合并窄技能为伞形技能
- **Patch**: 向伞形技能添加内容

### 状态持久化

- `.curator_state` JSON 文件：`last_run_at`, `run_count`, `paused`, `last_run_summary`
- 原子写入：`tempfile.mkstemp` + `os.replace`
- 首次运行：推迟一个完整间隔

**工作量：1-2 周**

---

## 4. IM Messaging Gateway（IM 消息网关）

### BasePlatformAdapter 接口

```python
class BasePlatformAdapter:
    _active_sessions: Dict[str, asyncio.Event]      # 每会话中断支持
    _pending_messages: Dict[str, MessageEvent]        # 中断时排队
    _busy_text_mode: "queue" | "interrupt"            # agent 忙时策略
    _auto_tts_enabled_chats / _auto_tts_disabled_chats

    async def connect() / disconnect()
    async def send(chat_id, content, metadata) -> SendResult
    async def send_draft(chat_id, draft_id, content)  # 流式草稿预览
    async def edit_message(chat_id, message_id, content)
    def truncate_message(text, limit)  # 平台感知（Telegram 用 UTF-16）
```

### GatewayRunner

```python
_AGENT_CACHE_MAX_SIZE = 128          # LRU 缓存 AIAgent 实例
_AGENT_CACHE_IDLE_TTL_SECS = 3600.0  # 1 小时空闲驱逐
_running_agents: Dict[str, AIAgent]  # 活跃 agent
_queued_events: Dict[str, List]      # FIFO 队列
```

### 38 个平台适配器

Telegram, Slack, Discord, WhatsApp, WeChat, DingTalk, Feishu, QQBot, Signal, SMS, Email, Matrix, HomeAssistant, BlueBubbles, Microsoft Graph, Webhook, Yuanbao, WeCom...

### Rust 移植

```rust
#[async_trait]
trait PlatformAdapter: Send + Sync {
    async fn connect(&mut self) -> Result<()>;
    async fn disconnect(&mut self) -> Result<()>;
    async fn send(&self, chat_id: &str, content: &str, meta: SendMeta) -> Result<SendResult>;
    async fn edit(&self, chat_id: &str, msg_id: &str, content: &str) -> Result<()>;
}

struct GatewayRunner {
    adapters: HashMap<Platform, Box<dyn PlatformAdapter>>,
    agent_cache: LruCache<String, AIAgent>,  // session_key -> agent
}
```

**工作量：3-4 周**（基础 adapter + gateway + 2-3 平台）

---

## 5. Error Classifier（错误分类器）

### 22 种 FailoverReason

| 分类 | 可重试 | 压缩 | 轮换 | 降级 |
|------|--------|------|------|------|
| `auth` | ✗ | ✗ | ✓ | ✓ |
| `auth_permanent` | ✗ | ✗ | ✗ | ✗ |
| `billing` | ✗ | ✗ | ✓ | ✓ |
| `rate_limit` | ✓ | ✗ | ✓ | ✓ |
| `overloaded` | ✓ | ✗ | ✗ | ✗ |
| `server_error` | ✓ | ✗ | ✗ | ✗ |
| `timeout` | ✓ | ✗ | ✗ | ✗ |
| `context_overflow` | ✓ | ✓ | ✗ | ✗ |
| `payload_too_large` | ✓ | ✓ | ✗ | ✗ |
| `model_not_found` | ✗ | ✗ | ✗ | ✓ |
| `format_error` | ✗ | ✗ | ✗ | ✓ |
| `unknown` | ✓ | ✗ | ✗ | ✗ |

### 分类管道（优先级排序）

1. Provider 特定模式（thinking 签名、tier 门控、xAI 权限）
2. HTTP 状态码 + 消息感知细化
3. 错误码分类（从 body）
4. 消息模式匹配（billing vs rate_limit vs context vs auth）
5. SSL/TLS 瞬态告警 → 重试为 timeout
6. 服务器断开 + 大会话 → context overflow
7. 传输错误启发式
8. 回退：unknown（可重试 + 退避）

### 402 消歧

- "usage limit" + "try again" → `rate_limit`（瞬态）
- 否则 → `billing`（耗尽）

### Rust 移植

```rust
enum FailoverReason {
    Auth, AuthPermanent, Billing, RateLimit, Overloaded, ServerError,
    Timeout, ContextOverflow, PayloadTooLarge, ModelNotFound, Unknown, ...
}

struct ClassifiedError {
    reason: FailoverReason,
    status_code: Option<u16>,
    retryable: bool,
    should_compress: bool,
    should_rotate_credential: bool,
    should_fallback: bool,
}
```

**工作量：1-2 周**

---

## 6. Auxiliary Client（辅助客户端）

### Provider 解析链

1. 用户主 provider + 主模型
2. OpenRouter (`OPENROUTER_API_KEY`)
3. Nous Portal (`~/.hermes/auth.json`)
4. 自定义端点 (`config.yaml model.base_url` + `OPENAI_API_KEY`)
5. 原生 Anthropic
6. 直接 API-key providers (z.ai/GLM, Kimi/Moonshot, MiniMax)
7. None

### 信用耗尽降级

- HTTP 402 → 自动重试链中下一个 provider
- `call_llm()` 透明处理

### 懒加载 OpenAI

- `_OpenAIProxy` 延迟 `from openai import OpenAI` 到首次调用
- 节省 ~240ms 冷启动时间

**工作量：1 周**

---

## 7. Tool System（工具系统）

### 自注册机制

```python
registry.register(
    name="tool_name",
    toolset="toolset_name",
    schema={...},
    handler=handle_tool_call,
    check_fn=check_requirements,  # 可选可用性检查
    requires_env=["ENV_VAR"],
)
```

### AST 发现

- `discover_builtin_tools()` 扫描 `tools/*.py`
- `_module_registers_tools()` 解析 AST 查找顶层 `registry.register(...)` 调用
- 避免导入不注册工具的模块

### Check Function TTL 缓存

```python
_CHECK_FN_TTL_SECONDS = 30.0
_check_fn_cache: Dict[Callable, tuple[float, bool]] = {}
```

### 委派工具

- 每个子 agent 获取新对话、独立 task_id、受限工具集
- `DELEGATE_BLOCKED_TOOLS`: delegate_task, clarify, memory, send_message, execute_code
- `MAX_DEPTH = 1`（可配置到 3）
- 审批回调：auto_deny（安全默认）或 auto-approve（可选）

### MCP 工具

- 传输：stdio, HTTP/StreamableHTTP, SSE
- 重连：指数退避 + 抖动（最多 5 次重试）
- 采样：MCP server 可请求 LLM 完成
- 安全：`_SAFE_ENV_KEYS` 仅传递安全环境变量；`_CREDENTIAL_PATTERN` 从错误消息中剥离凭证

**工作量：2-3 周**
