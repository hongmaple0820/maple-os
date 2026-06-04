# golutra 终端引擎 + 全项目产品设计模式分析

---

## Part 1: golutra 终端引擎

### 1.1 PTY Session 生命周期

```
Connecting (locked) → Online → Working → Online → Offline
                        ↑                         |
                        |_________________________|
                              (进程退出/崩溃)
```

**关键设计：**
- **预创建状态锁定**：`lock_session_status_precreate()` 在 PTY 注册前设置 "Connecting"，消除启动时的 "Online" 闪烁
- **Working 触发**：用户输入 `\n`/`\r` 立即标记；输出到达时如果在 `working_intent_window`(1500ms) 内或 `chat_pending`/`dispatch_inflight` 也触发
- **Working→Online 回退**：静默超时 4500ms + 1000ms 去抖

### 1.2 Dispatch Queue

- 队列限制：每终端 32 条
- 消息 ID 去重窗口：128 条（FIFO deque）
- 去重检查：inflight、queue、recent 三处
- 同发送者连续消息用 `\n\n` 合并

### 1.3 Flow Control（ACK-based）

| 参数 | 值 | 说明 |
|------|-----|------|
| 高水位 | 200KB | 未确认字节，暂停 PTY 读取 |
| 低水位 | 20KB | 恢复读取 |
| 发射间隔 | 16ms | ~60fps UI 输出 |
| 单批最大 | 64KB | |

### 1.4 Semantic Worker Thread

独立线程 + 专用虚拟终端仿真器：

1. 维护**并行终端仿真器**镜像真实 PTY 输出
2. `UserInput` 事件：设置 `chat_block_pending`，生成 `chat_span_id`(ULID)
3. `Output` 事件：应用字节到仿真器，可选 160ms 间隔发射**流增量**（与前内容 diff）
4. `Flush` 事件：快照 → 过滤器管道：
   - `Allow` → 构建语义负载写入聊天
   - `Drop` → 丢弃（噪音）
   - `Defer` → 丢弃但释放 dispatch gate
5. 调用 `dispatch_gate.on_semantic_flush_complete()` 释放下一条排队消息

**关键洞察**：语义 worker 将终端渲染与聊天提取解耦。过滤器只影响聊天回写，不影响终端显示。

### 1.5 Filter Pipeline

5 个配置文件：Generic, Codex, Gemini, Claude, Shell

决策：`Allow`(传到聊天), `Drop`(从聊天抑制), `Defer`(抑制但释放 dispatch gate)

两种模式：`Stream`(实时流更新) 和 `Final`(沉默后最终快照)

### 1.6 Shim Process（~80 行）

1. 生成前立即发射 `OSC 633;A`（就绪信号）
2. 生成目标 CLI（继承 stdin/stdout/stderr）
3. 退出时发射 `OSC 633;D;{exit_code}`（退出信号）
4. 错误处理：`SHIM_LAUNCH_ERROR` 标记前缀；退出码：101(无目标), 102(生成失败), 103(等待失败)
5. Windows：强制 UTF-8 控制台代码页 (65001)

### 1.7 Chat Outbox Worker

| 参数 | 值 |
|------|-----|
| 轮询间隔 | 280ms |
| 每次认领 | 8 任务 |
| 租约时长 | 8 秒 |
| 最大尝试 | 6 次 |
| 退避基础 | 800ms |
| 退避最大 | 30s |
| 退避因子 | 2^(attempts-1) |

### 1.8 Trigger System

- `TriggerBus`: mpsc channel 包装器
- `TriggerScheduler`: BinaryHeap 优先队列（最早到期优先）
- **去重**：`HashMap<TriggerKey, u64>` 存储每个 key 的最新 due_at

**延迟阶段**：`Stable`, `Silence`, `Debounce`, `PostReadyTick`, `ChatPendingForce`

### 1.9 Post-Ready System

4 种步骤：
- `Input { input, require_stable }` — 发送文字
- `ExtractSessionId { keyword, require_stable }` — 解析屏幕输出获取 session ID
- `WaitForPattern { pattern, require_stable }` — 等待模式出现
- `Introduction { prompt_type, require_stable }` — 生成语言感知的入门提示

自动重启：超时 2000ms，最多 3 次重试

---

## Part 2: 跨项目产品设计模式

### 2.1 会话持久化和恢复

| 项目 | 方案 |
|------|------|
| **golutra** | SQLite 会话映射；`resume_command_template` + `ExtractSessionId` 自动检测 |
| **hermes** | 命名会话 `/new` `/resume` `/branch` `/compress`；FTS5 搜索；跨会话召回 |
| **cc-haha** | CRUD API；消息级分支；worktree 隔离；批量管理 |
| **claw-code** | 命名会话；`claw prompt` 一次性；配置文件层级 |
| **rig** | `ConversationMemory` trait + 可插拔后端 |

### 2.2 工具执行进度可视化

| 项目 | 方案 |
|------|------|
| **golutra** | 终端输出即聊天消息；160ms 语义流增量；过滤器去噪 |
| **hermes** | `/verbose` 循环：off→new→all→verbose；可配置忙碌指示器 |
| **cc-haha** | 聊天中显示工具执行；Computer Use 权限模态框；变更面板显示 diff |
| **claw-code** | REPL 流式显示 |

### 2.3 错误处理和恢复

| 项目 | 方案 |
|------|------|
| **golutra** | `mark_session_broken()`；SHIM_LAUNCH_ERROR 检测；outbox 指数退避；死信处理 |
| **hermes** | `/doctor` 命令；provider 特定错误提示；优雅降级链 |
| **cc-haha** | 激活前 provider 测试；会话错误状态 |
| **claw-code** | `claw doctor` 健康检查；parity harness 回归检测 |

### 2.4 多模型/多 Provider 切换

| 项目 | 方案 |
|------|------|
| **golutra** | 静态成员注册（6 个 CLI）；命令名自动检测 |
| **hermes** | `/model --provider`；200+ 模型通过 OpenRouter；非 agentic 模型警告 |
| **cc-haha** | UI 中 provider CRUD；预设；连接测试；每会话运行时选择 |
| **rig** | 统一 trait 接口 20+ provider；`Client::from_env()` |

### 2.5 安全/权限 UX

| 项目 | 方案 |
|------|------|
| **golutra** | 每 CLI `unlimited_access_flag`；DND 模式 |
| **hermes** | `/yolo` 切换；命令审批 `/approve` `/deny`；容器隔离 |
| **cc-haha** | Computer Use 权限模态框；危险命令审批 |
| **claw-code** | 审批沙箱 |

---

## Part 3: MapleOS 采纳建议

### P1: 核心终端引擎 (3-4 周)

从 golutra 采纳：
1. **Shim 进程** — OSC 就绪/退出信号包装任意 CLI（~80 行，直接可移植）
2. **Semantic Worker** — 并行终端仿真器用于聊天提取
3. **Filter Pipeline** — 基于配置文件的噪音去除
4. **Flow Control** — ACK-based 背压防止内存耗尽

### P2: Dispatch 和编排 (2-3 周)

从 golutra 采纳：
1. **ChatDispatchBatcher** — 语义门控 dispatch 防止上下文污染
2. **Outbox 模式** — 可靠投递 + 指数退避
3. **@mention 路由** — DM vs channel 语义

从 hermes 采纳：
1. **Slash 命令注册表** — CommandDef 数据类
2. **模型切换管道** — 解析→解析→标准化→警告

### P3: 会话管理 (2 周)

从 cc-haha 采纳：
1. **Session Store** — CRUD + 分支 + 批量操作
2. **Tab 系统** — 类型前缀 ID
3. **Provider 管理** — 预设 + 测试

从 golutra 采纳：
1. **Post-ready 系统** — CLI 特定初始化
2. **Session ID 提取** — 跨会话恢复
3. **默认成员注册表** — 已知 CLI

### P4: UI/UX 模式 (3-4 周)

从 cc-haha：工作区面板 + diff 视图 + 权限模态框 + 活跃目标条 + 可调整终端面板
从 hermes：忙碌指示器 + verbose 级别 + 状态栏 + /doctor 诊断
从 rig：typestate builder + derive 宏 + 统一 provider 接口

### P5: 高级功能 (4-6 周)

- golutra 的 Trigger 系统（事件驱动规则评估）
- 多 agent 协调（工作区成员模型 + 并行子 agent + 团队状态栏）

---

## 工作量汇总

| 组件 | 来源 | 工作量 | 优先级 |
|------|------|--------|--------|
| Shim 进程 + OSC 协议 | golutra | 3 天 | P1 |
| Semantic Worker + Filter | golutra | 2 周 | P1 |
| Flow Control | golutra | 3 天 | P1 |
| Dispatch Batcher + Outbox | golutra | 1 周 | P2 |
| Slash 命令注册表 | hermes | 1 周 | P2 |
| 模型切换管道 | hermes | 3 天 | P2 |
| Session Store + Tabs | cc-haha | 1 周 | P3 |
| Provider 管理 UI | cc-haha | 1 周 | P3 |
| Post-ready + Session 恢复 | golutra | 1 周 | P3 |
| 工作区面板 + Diff 视图 | cc-haha | 2 周 | P4 |
| 权限模态框 | cc-haha | 3 天 | P4 |
| Doctor 诊断 | hermes | 3 天 | P4 |
| Trigger 系统 | golutra | 1 周 | P5 |
| Typestate Builder API | rig | 1 周 | P5 |
| Derive 宏 | rig | 1 周 | P5 |
| **总计** | | **~14 周** | |
