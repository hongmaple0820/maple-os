# cc-haha 深度分析 — ToolUseContext + StreamingToolExecutor + Computer Use + Coordinator + Cron

---

## 1. ToolUseContext（依赖注入容器）

### 设计

~30 个字段的结构体，作为所有工具执行的统一上下文：

```rust
struct ToolUseContext {
    // 会话
    session_id: String,
    conversation_id: String,
    message_id: String,

    // 模型
    model: ModelConfig,
    provider: ProviderInfo,

    // 工具
    tool_name: String,
    tool_input: Value,
    permission_level: PermissionLevel,

    // 文件系统
    workspace_root: PathBuf,
    cwd: PathBuf,
    allowed_paths: Vec<PathBuf>,

    // 网络
    http_client: HttpClient,
    api_key: Option<String>,

    // UI 反馈
    progress_sender: Option<ProgressSender>,
    cancellation_token: CancellationToken,

    // 审批
    approval_callback: Option<ApprovalCallback>,
    auto_approve: bool,

    // 配置
    feature_flags: FeatureFlags,
    environment: HashMap<String, String>,

    // ... 更多
}
```

### 关键洞察

- **统一入口**：所有工具通过同一个 context 执行，无需关心依赖来源
- **可测试性**：mock 整个 context 即可测试任意工具
- **权限传播**：permission_level 从 context 向下传播到每个工具
- **取消支持**：cancellation_token 贯穿整个执行链

**工作量：2-3 天**

---

## 2. StreamingToolExecutor（流式工具执行器）

### 并发模型

```typescript
// 两类工具
type ToolConcurrency = 'concurrent-safe' | 'exclusive';

// concurrent-safe: 可并行执行（read_file, search, web_fetch）
// exclusive: 必须独占（write_file, execute_bash, computer_use）
```

### 执行策略

1. 收到 tool_use 列表
2. 分类：concurrent-safe vs exclusive
3. concurrent-safe 工具并行执行（`Promise.all`）
4. exclusive 工具串行执行
5. 结果按原始顺序发射（非完成顺序）
6. 错误级联：一个工具失败 → 取消剩余 → 上报错误

### 有序发射

```typescript
// 结果必须按 tool_use 索引顺序发射
for (const [index, result] of sortedResults) {
    emit({ type: 'tool_result', index, ...result });
}
```

**工作量：2-3 天**

---

## 3. Computer Use（计算机控制）

### 权限层级（每应用）

```
read    — 只能截图查看
click   — 可以点击
full    — 完全控制（键盘、鼠标、文件）
```

### 坐标模式

- **normalized** (0.0-1.0)：跨分辨率兼容
- **pixel** (绝对像素)：精确控制
- 默认 normalized，API 可切换

### 验证机制

1. 执行前：截图获取当前状态
2. 执行动作（点击/输入/滚动）
3. 执行后：再次截图
4. 像素级 diff 验证动作是否生效
5. 失败时自动重试（最多 3 次）

### macOS 后端

- Swift 原生实现
- Accessibility API 用于 UI 元素发现
- Core Graphics 用于截图和鼠标控制
- CGEvent 用于键盘输入

**工作量：3-5 天（不含 macOS 后端）**

---

## 4. Browser Automation（浏览器自动化）

### 架构

- MCP Server 模式：独立进程，通过 MCP 协议通信
- 基于 Playwright 内核
- 支持 Chrome/Firefox/WebKit

### 功能

- 页面导航、截图、DOM 查询
- 表单填写、按钮点击
- 网络请求拦截
- 多标签页管理
- 文件上传

### 开源版本

- 框架代码存在但功能 stub
- 需要自行实现具体 browser 操作

**工作量：1-2 周（完整实现）**

---

## 5. Cron Tasks（定时任务）

### 任务类型

| 类型 | 说明 | 示例 |
|------|------|------|
| `recurring` | 周期执行 | 每天 9:00 发送报告 |
| `durable` | 持久化，跨重启恢复 | 监控任务 |
| `permanent` | 永久任务，不自动删除 | 定期清理 |

### 存储

- 文件后端：`~/.claude/cron/jobs.json`
- 最大任务数：`MAX_JOBS = 50`
- 原子写入：先写临时文件，再 rename

### 调度

- 基于 cron 表达式
- 支持时区
- 错过执行的处理策略：skip 或 catch-up

**工作量：3-5 天**

---

## 6. Coordinator Mode（协调器模式）

### 369 行系统提示

```
角色：你是协调器，负责将复杂任务分解并分配给 worker agent
原则：
1. 先分析任务，识别可并行的子任务
2. 为每个子任务创建 worker
3. 监控 worker 执行
4. 汇总结果
5. 处理异常
```

### 4 阶段工作流

```
Analyze → Delegate → Monitor → Synthesize
```

1. **Analyze**：分析用户请求，分解为子任务
2. **Delegate**：创建 worker agent，分配子任务
3. **Monitor**：监控 worker 执行状态
4. **Synthesize**：汇总结果，返回给用户

### Worker 生成

- 限制最多 N 个并行 worker（可配置）
- 每个 worker 独立工具集 + 上下文
- 通过消息队列通信

**工作量：3-5 天**

---

## 7. Swarm Backends（集群后端）

### 3 种后端

| 后端 | 说明 | 场景 |
|------|------|------|
| `tmux` | tmux session 管理 | 终端环境 |
| `iTerm2` | iTerm2 tab/pane | macOS |
| `in-process` | 进程内执行 | 桌面应用 |

### 文件邮箱 IPC

```
~/.claude/teams/{team}/inboxes/{agent}.json
```

- JSON 消息格式
- 文件锁保证原子性
- 支持优先级排序
- 消息过期清理

**工作量：1-2 周**

---

## 8. Desktop UX（桌面体验）

### Session 分支

- 支持从任意消息创建分支
- 分支独立演进
- 可合并回主分支

### Tab 系统

- 类型前缀 ID：`chat:xxx`, `terminal:xxx`, `file:xxx`
- 每个 tab 独立状态
- 支持拖拽排序

### Workspace Panel

- 文件树浏览
- Git diff 视图
- 变更文件高亮
- 一键跳转到文件

### Diff 视图

- 内联 diff 显示
- 语法高亮
- 接受/拒绝单个变更
- 批量操作

**工作量：2-3 周**

---

## 9. Architecture Patterns（架构模式）

### AsyncGenerator 查询循环

```typescript
async function* queryLoop(prompt: string) {
    while (true) {
        const response = await llm.complete(prompt);
        yield response;

        if (response.stop_reason === 'end_turn') break;

        // 处理 tool_use
        const results = await executeTools(response.tool_uses);
        prompt = buildPrompt(results);
    }
}
```

### Feature Flags

```typescript
const FEATURE_FLAGS = {
    ENABLE_COMPUTER_USE: false,
    ENABLE_BROWSER: false,
    ENABLE_SWARM: false,
    ENABLE_CRON: false,
    // ...
};
```

### Stub Pattern

- 功能模块存在但为空实现
- 运行时检查 feature flag
- 启用后注入真实实现
- 便于渐进式开发

### buildTool 默认值

- 每个工具定义包含默认参数
- 用户未提供时使用默认值
- 减少必填参数数量

**工作量：持续改进**

---

## MapleOS 应采纳的关键模式

### 高优先级

| 模式 | 价值 | 工作量 |
|------|------|--------|
| ToolUseContext DI | 统一工具执行上下文 | 2-3 天 |
| StreamingToolExecutor | 并发安全 + 有序发射 | 2-3 天 |
| Cron Tasks | 定时任务能力 | 3-5 天 |
| Coordinator Mode | 复杂任务分解 | 3-5 天 |

### 中优先级

| 模式 | 价值 | 工作量 |
|------|------|--------|
| Computer Use | 桌面自动化 | 3-5 天 |
| Session 分支 | 会话管理增强 | 3-5 天 |
| Tab 系统 | 多会话管理 | 2-3 天 |
| Workspace Panel | 文件管理 UX | 1 周 |

### 低优先级

| 模式 | 价值 | 工作量 |
|------|------|--------|
| Browser Automation | Web 自动化 | 1-2 周 |
| Swarm Backends | 多 agent 集群 | 1-2 周 |
| Diff 视图 | 代码审查 UX | 1 周 |

**总计：5-7 周核心 + 2-3 周桌面 UI**
