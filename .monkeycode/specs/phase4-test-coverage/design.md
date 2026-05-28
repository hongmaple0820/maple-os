# Phase 4: 测试覆盖 — 技术设计规格说明书

> 日期: 2026-05-26
> 优先级: P2

---

## 1. Rust 核心模块单元测试

### 1.1 executor.rs 测试设计

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_workflow() -> Workflow {
        // 创建简单 DAG: Start -> LLM -> End
        Workflow {
            name: "test_workflow",
            nodes: vec![
                WorkflowNode { id: "start", node_type: NodeType::Delay, config: serde_json::json!({"seconds": 0}), .. },
                WorkflowNode { id: "llm", node_type: NodeType::Llm, config: serde_json::json!({"prompt": "test"}), .. },
            ],
            edges: vec![WorkflowEdge { source: "start", target: "llm" }],
        }
    }

    #[tokio::test]
    async fn test_dag_topological_order() {
        // 验证节点按依赖顺序执行,无依赖的并行
    }

    #[tokio::test]
    async fn test_condition_node_branch() {
        // condition=true 路径 → node_a
        // condition=false 路径 → node_b
    }

    #[tokio::test]
    async fn test_parallel_node_concurrent() {
        // 3 个无依赖子节点并发执行
        // 验证总耗时 < 3 * single_node_duration
    }

    #[tokio::test]
    async fn test_loop_node_max_iterations() {
        // loop 节点设置 max_iterations=3
        // 验证循环 3 次后终止
    }

    #[tokio::test]
    async fn test_node_failure_propagation() {
        // 单节点执行失败 → workflow status = Failed
        // 后续依赖节点不执行
    }

    #[tokio::test]
    async fn test_checkpoint_recovery() {
        // 执行到第 3 个节点后中断
        // 从 checkpoint 恢复 → 继续执行后续节点
    }
}
```

### 1.2 react_loop.rs 测试设计

```rust
#[cfg(test)]
mod tests {
    use super::*;

    struct MockLlmAdapter;
    impl LlmAdapter for MockLlmAdapter {
        fn complete(&self, request: LlmRequest) -> Result<LlmResponse> {
            // 第 1 轮: 返回 ToolUse
            // 第 2 轮: 返回最终答案
            if request.messages.len() <= 1 {
                Ok(LlmResponse { content: "Let me search for that", tool_calls: vec![ToolCall { name: "web_search", arguments: json!({"query": "test"}) }] })
            } else {
                Ok(LlmResponse { content: "The answer is 42", tool_calls: vec![] })
            }
        }
    }

    #[tokio::test]
    async fn test_react_loop_multi_turn() {
        // 验证: ToolUse → ToolResult → 继续推理 → 最终答案
    }

    #[tokio::test]
    async fn test_react_loop_max_iterations() {
        // 设置 max_iterations=2
        // 验证: 2轮后终止,返回中间结果
    }

    #[tokio::test]
    async fn test_react_loop_no_tool_call() {
        // LLM 不调用工具,直接返回答案
        // 验证: 1轮完成
    }
}
```

### 1.3 task_queue.rs 测试设计

```rust
#[cfg(test)]
mod tests {
    use super::*;

    async fn setup_test_db() -> SqlitePool {
        // 创建内存 SQLite + 初始化 task_queue 表
    }

    #[tokio::test]
    async fn test_enqueue_dequeue() {
        let db = setup_test_db();
        let queue = TaskQueueService::new(db);
        queue.enqueue("test_task", "llm", json!({"prompt": "hello"}), 1, None).await?;
        let task = queue.dequeue().await?;
        assert_eq!(task.task_type, "llm");
    }

    #[tokio::test]
    async fn test_complete_and_stats() {
        // enqueue → dequeue → complete → stats 应反映 completed 数量
    }

    #[tokio::test]
    async fn test_retry_with_backoff() {
        // fail → retry_count 递增 → retry_at 按 2^n 秒退避
    }

    #[tokio::test]
    async fn test_dead_letter_queue() {
        // fail 超过 max_retries → 转入 dead_letter → dead_letter 列表可查看
    }

    #[tokio::test]
    async fn test_requeue_from_dead_letter() {
        // 从 dead_letter requeue → 重新变为 pending
    }
}
```

### 1.4 workflow.rs 测试设计

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_json_definition() {
        let json = r#"{"name": "test", "nodes": [...], "edges": [...]}"#;
        let wf = Workflow::parse_definition(json)?;
        assert_eq!(wf.name, "test");
    }

    #[test]
    fn test_parse_yaml_definition() {
        let yaml = "name: test\nnodes:\n  ...";
        let wf = Workflow::parse_definition(yaml)?;
        assert_eq!(wf.name, "test");
    }

    #[test]
    fn test_parse_json_fallback_to_yaml() {
        // 先尝试 JSON,失败后 fallback YAML
        let invalid_json_but_valid_yaml = "...";
        let wf = Workflow::parse_definition(invalid_json_but_valid_yaml)?;
        // 应成功解析为 YAML
    }
}
```

### 1.5 router.rs 测试设计

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_route_by_task_type() {
        // task_type=code → route to deepseek-coder
    }

    #[test]
    fn test_route_by_privacy_level() {
        // privacy_level=sensitive → route to local model (no fallback to cloud)
    }

    #[test]
    fn test_fallback_chain() {
        // primary model unavailable → fallback chain → 最终使用 gpt-4o-mini
    }

    #[test]
    fn test_budget_limit() {
        // budget exhausted → return error
    }
}
```

---

## 2. E2E 真实后端集成测试

### 2.1 测试基础设施

```typescript
// tests/e2e/fixtures.ts 改造

import { test as base } from '@playwright/test';
import { ChildProcess, spawn } from 'child_process';

type E2eFixture = {
  backendProcess: ChildProcess | null;
  scaleProcess: ChildProcess | null;
};

export const test = base.extend<E2eFixture>({
  backendProcess: [async ({}, use) => {
    // 启动 Rust 后端
    const backend = spawn('cargo', ['run', '--release', '-p', 'mapleos-server'], {
      cwd: process.cwd(),
      env: { ...process.env, RUST_LOG: 'info', DATABASE_URL: 'sqlite:test_e2e.db?mode=rwc' },
    });
    // 等待端口 7788 可用
    await waitForPort(7788, 30000);
    await use(backend);
    backend.kill();
  }, { scope: 'worker' }],

  scaleProcess: [async ({}, use) => {
    // 启动 SCALE bridge
    const scale = spawn('node', ['core/scale-engine/bridge-http.mjs'], {
      cwd: process.cwd(),
      env: { ...process.env, SCALE_PORT: '7790' },
    });
    await waitForPort(7790, 15000);
    await use(scale);
    scale.kill();
  }, { scope: 'worker' }],
});
```

### 2.2 Mock 策略

**内部 API (真实)**: /api/auth/*, /api/chat/stream, /api/kb/*, /api/workflows/*, /api/agents/*, /api/tasks/*, /api/config/*, /rpc

**外部 API (Mock)**: LLM 调用(Ollama/OpenAI/Anthropic) — 不可在 CI 环境依赖真实 LLM

Mock LLM 方案:
```typescript
// tests/e2e/mock-llm.ts
// 通过环境变量 RUST_LLM_ADAPTER=mock 让后端使用 mock adapter
// mock adapter: 固定返回 "This is a mock LLM response" + 2秒延迟模拟流式
```

后端改造: 新增 `MockLlmAdapter`:
```rust
pub struct MockLlmAdapter;

impl LlmAdapter for MockLlmAdapter {
    fn complete(&self, _req: LlmRequest) -> Result<LlmResponse> {
        Ok(LlmResponse {
            content: "This is a mock LLM response for testing.",
            tool_calls: vec![],
            usage: Usage { prompt_tokens: 10, completion_tokens: 20, total_tokens: 30 },
        })
    }

    fn stream(&self, _req: LlmRequest) -> Result<Box<dyn LlmStream>> {
        Ok(Box::new(MockLlmStream {
            tokens: vec!["This ", "is ", "a ", "mock ", "response"],
            index: 0,
        }))
    }
}
```

### 2.3 核心闭环 E2E 场景

```typescript
// tests/e2e/auth-flow.spec.ts
test('注册登录闭环', async ({ page }) => {
  // 1. 访问页面，应重定向到登录页
  await page.goto('/');
  await expect(page.locator('[data-testid="login-form"]')).toBeVisible();

  // 2. 注册新用户
  await page.click('[data-testid="goto-register"]');
  await page.fill('[data-testid="register-username"]', `e2e_user_${Date.now()}`);
  await page.fill('[data-testid="register-password"]', 'test1234');
  await page.click('[data-testid="register-submit"]');

  // 3. 注册成功后应自动登录并跳转 Dashboard
  await expect(page.locator('[data-testid="dashboard"]')).toBeVisible();

  // 4. Dashboard 指标卡应有值 (非 0/undefined)
  await expect(page.locator('[data-testid="agents-count"]')).not.toHaveText('0');
});

// tests/e2e/chat-flow.spec.ts
test('Chat SSE 流式闭环', async ({ page }) => {
  // 1. 已登录状态，进入 Chat 页面
  await page.click('[data-testid="nav-chat"]');

  // 2. 选择 Agent
  await page.click('[data-testid="agent-selector"]');
  await page.click('[data-testid="agent-option-0"]');

  // 3. 发送消息
  await page.fill('[data-testid="chat-input"]', '你好');
  await page.click('[data-testid="chat-send"]');

  // 4. 验证 token 逐字出现 (1s 内看到第一个字)
  const firstToken = page.locator('[data-testid="assistant-message"]').first();
  await expect(firstToken).toBeVisible({ timeout: 2000 });

  // 5. 验证 done 后完整消息
  await expect(page.locator('[data-testid="assistant-message"]')).toContainText('mock response');
});

// tests/e2e/workflow-flow.spec.ts
test('Workflow 创建执行闭环', async ({ page }) => {
  // 1. 进入 Workflow 页面
  await page.click('[data-testid="nav-workflows"]');

  // 2. 创建新工作流
  await page.click('[data-testid="new-workflow"]');

  // 3. 添加节点
  await page.click('[data-testid="add-llm-node"]');

  // 4. 执行工作流
  await page.click('[data-testid="execute-workflow"]');

  // 5. 验证 SSE 事件更新节点状态
  await expect(page.locator('[data-testid="node-status-running"]')).toBeVisible({ timeout: 3000 });
  await expect(page.locator('[data-testid="node-status-completed"]')).toBeVisible({ timeout: 10000 });
});

// tests/e2e/knowledge-flow.spec.ts
test('Knowledge 搜索引用闭环', async ({ page }) => {
  // 1. 索引文本
  await page.click('[data-testid="nav-knowledge"]');
  await page.fill('[data-testid="kb-title"]', '测试文档');
  await page.fill('[data-testid="kb-content"]', 'MapleOS 是一个 AI Native 操作系统');
  await page.click('[data-testid="kb-index"]');

  // 2. 搜索
  await page.fill('[data-testid="kb-search-input"]', 'MapleOS');
  await page.click('[data-testid="kb-search"]');

  // 3. 验证搜索结果
  await expect(page.locator('[data-testid="kb-result"]')).toContainText('AI Native');

  // 4. Chat 中自动引用
  await page.click('[data-testid="nav-chat"]');
  await page.fill('[data-testid="chat-input"]', 'MapleOS 是什么');
  await page.click('[data-testid="chat-send"]');

  // 5. 验证 KB 引用卡片出现
  await expect(page.locator('[data-testid="kb-ref-card"]')).toBeVisible();
});

// tests/e2e/agent-task-flow.spec.ts
test('Agent 注册派发闭环', async ({ page }) => {
  // 1. 注册 Agent
  await page.click('[data-testid="nav-agents"]');
  await page.fill('[data-testid="agent-name"]', `e2e_agent_${Date.now()}`);
  await page.click('[data-testid="agent-register"]');

  // 2. 验证 Agent 出现
  await expect(page.locator('[data-testid="agent-list"]')).toContainText('e2e_agent');

  // 3. 派发任务
  await page.click('[data-testid="task-dispatch"]');

  // 4. 验证 tasks/stats 变化
  await expect(page.locator('[data-testid="tasks-pending"]')).not.toHaveText('0');
});
```

### 2.4 playwright.config.ts 改造

```typescript
// playwright.config.ts
import { defineConfig } from '@playwright/test';

export default defineConfig({
  testDir: './tests/e2e',
  fullyParallel: false,              // 真实后端测试不可并行(共享 DB)
  retries: 1,                        // 一次重试机会(后端启动可能不稳定)
  timeout: 30000,                    // 30s 超时(等待后端处理)
  use: {
    baseURL: 'http://localhost:3000',
  },
  webServer: {
    command: 'cd apps/web && pnpm dev',
    port: 3000,
    reuseExistingServer: true,
  },
});
```

---

## 3. 文件变更清单

| # | 文件 | 操作 | 说明 |
|---|------|------|------|
| 4.1 | `core/maple-engine/src/executor.rs` | 改造 | 新增 #[cfg(test)] mod tests |
| 4.2 | `core/maple-agent/src/react_loop.rs` | 改造 | 新增 #[cfg(test)] mod tests |
| 4.3 | `core/maple-engine/src/task_queue.rs` | 改造 | 新增 #[cfg(test)] mod tests |
| 4.4 | `core/maple-engine/src/workflow.rs` | 改造 | 新增 #[cfg(test)] mod tests |
| 4.5 | `core/maple-llm/src/router.rs` | 改造 | 新增 #[cfg(test)] mod tests |
| 4.6 | `core/maple-agent/src/orchestrator.rs` | 改造 | 新增 #[cfg(test)] mod tests |
| 4.7 | `core/maple-llm/src/adapters/mock.rs` | 新增 | MockLlmAdapter + MockLlmStream |
| 4.8 | `core/maple-llm/src/lib.rs` | 改造 | 导出 MockLlmAdapter |
| 4.9 | `tests/e2e/fixtures.ts` | 改造 | 真实后端启动 + LLM Mock |
| 4.10 | `tests/e2e/mock-llm.ts` | 新增 | LLM Mock 配置 |
| 4.11 | `tests/e2e/auth-flow.spec.ts` | 新增 | 注册登录闭环 |
| 4.12 | `tests/e2e/chat-flow.spec.ts` | 新增 | Chat SSE 流式闭环 |
| 4.13 | `tests/e2e/workflow-flow.spec.ts` | 新增 | Workflow 创建执行闭环 |
| 4.14 | `tests/e2e/knowledge-flow.spec.ts` | 新增 | Knowledge 搜索引用闭环 |
| 4.15 | `tests/e2e/agent-task-flow.spec.ts` | 新增 | Agent 注册派发闭环 |
| 4.16 | `playwright.config.ts` | 改造 | 真实后端配置 |
| 4.17 | 前端各组件 | 改造 | 添加 data-testid 属性 |

---

## 4. 风险与应对

| 风险 | 影响 | 应对策略 |
|------|------|---------|
| CI 环境无 Rust 后端编译时间过长 | E2E 测试启动慢 | CI 中 cargo build --release 缓存,或使用预编译二进制 |
| 真实后端测试不可并行 | 测试执行时间长 | 分为 serial 和 parallel 两组,仅闭环测试 serial |
| LLM Mock 适配器不够真实 | 测试覆盖不了真实 LLM 行为 | Mock 适配器模拟典型场景(ToolUse/错误/空回复),真实 LLM 测试仅本地手动执行 |
| DB 状态污染 | 串行测试间相互影响 | 每个测试用例前清空 SQLite 或使用独立 DB 文件 |
| 前端组件无 data-testid | E2E 选择器不稳定 | 逐组件添加 data-testid,优先在核心交互元素上 |