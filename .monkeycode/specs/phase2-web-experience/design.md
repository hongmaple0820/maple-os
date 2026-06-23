# Phase 2: Web 体验补全 — 技术设计规格说明书

> 日期: 2026-05-26
> 优先级: P1
> 依赖: 后端所有 API 端点已实现

---

## 1. Session 历史消息加载

### 1.1 数据流

```
用户点击 Session 列表项
  ↓
setMessages([]) → 显示 spinner
  ↓
GET /api/sessions/:id/messages
  ↓
返回: [{ role: "user/assistant", content: "...", timestamp: "...", kb_sources?: KnowledgeRef[] }]
  ↓
setMessages(response) → 消息区域渲染
```

### 1.2 chat-panel.tsx 改造

```typescript
// 当前: 切换 session 只清空消息
// 改造: 加载历史消息

const handleSessionChange = async (sessionId: string) => {
  setCurrentSession(sessionId);
  setMessages([]);
  setLoadingHistory(true);

  try {
    const messages = await mapleApi<{ messages: ChatMessage[] }>(
      `/api/sessions/${sessionId}/messages`
    );
    setMessages(messages.messages || []);
  } catch (err) {
    // 历史加载失败不影响新建对话
    setMessages([]);
  } finally {
    setLoadingHistory(false);
  }
};
```

### 1.3 后端已有端点

`GET /api/sessions/:id/messages` 已在 main.rs 中注册 (handler: `get_session_messages_handler`), 返回 `chat_messages` 表中按 `created_at` 排序的消息列表。

### 1.4 分页策略

后端端点支持 `limit` 查询参数 (默认 50):
```typescript
const messages = await mapleApi<{ messages: ChatMessage[] }>(
  `/api/sessions/${sessionId}/messages?limit=50`
);
```

加载更多:
```typescript
const olderMessages = await mapleApi<{ messages: ChatMessage[] }>(
  `/api/sessions/${sessionId}/messages?limit=50&before=${oldestTimestamp}`
);
setMessages(prev => [...olderMessages.messages, ...prev]);
```

---

## 2. Workflow 已有工作流编辑

### 2.1 数据流

```
用户点击左侧列表中的工作流
  ↓
调用 GET /api/workflows/:id (已有端点)
  ↓
返回: { id, name, yaml_content, version, status, ... }
  ↓
解析 definition (JSON 先, YAML fallback)
  ↓
渲染节点到画布 canvasNodes + 渲染连线到 canvasEdges
  ↓
右侧面板显示第一个选中节点的配置
```

### 2.2 Definition 解析逻辑

```typescript
function parseWorkflowDefinition(raw: string): WorkflowDefinition {
  // 1. 先尝试 JSON
  try {
    const json = JSON.parse(raw);
    return normalizeDefinition(json);
  } catch {}

  // 2. 再尝试 YAML (浏览器端用 js-yaml 库)
  try {
    const yaml = jsYaml.load(raw);
    return normalizeDefinition(yaml);
  } catch {}

  throw new Error('工作流定义格式无效');
}

function normalizeDefinition(def: any): WorkflowDefinition {
  return {
    name: def.name || '未命名',
    nodes: (def.nodes || []).map(n => ({
      id: n.id || uuid(),
      type: n.node_type || n.type || 'llm',
      name: n.name || n.id,
      config: n.config || {},
      position: n.position || { x: 100, y: 100 },  // 后端可能无 position, 需默认布局
    })),
    edges: (def.edges || []).map(e => ({
      source: e.source || e.from,
      target: e.target || e.to,
    })),
  };
}
```

### 2.3 画布渲染改造

当前 `workflow-manager.tsx` 使用 `canvasNodes` state 管理节点位置和类型。加载已有工作流时:

```typescript
const handleLoadWorkflow = async (workflowId: string) => {
  setCurrentWorkflowId(workflowId);

  const wf = await mapleApi<WorkflowItem>(`/api/workflows/${workflowId}`);
  const definition = parseWorkflowDefinition(wf.yaml_content);

  // 自动布局: 如果节点无 position，使用 dagre 自动布局
  const positionedNodes = definition.nodes.map((n, i) => ({
    ...n,
    x: n.position?.x || 150 + (i % 3) * 200,
    y: n.position?.y || 100 + Math.floor(i / 3) * 150,
  }));

  setCanvasNodes(positionedNodes);
  setCanvasEdges(definition.edges);
};
```

### 2.4 保存工作流修改

```typescript
const handleSaveWorkflow = async () => {
  const definition = {
    name: currentWorkflowName,
    nodes: canvasNodes.map(n => ({
      id: n.id,
      node_type: n.type,
      name: n.name,
      config: n.config,
      position: { x: n.x, y: n.y },
    })),
    edges: canvasEdges.map(e => ({ source: e.source, target: e.target })),
  };

  await mapleApi(`/api/workflows/${currentWorkflowId}`, {
    method: 'PUT',
    body: { yaml_content: JSON.stringify(definition) },
  });

  showToast('已保存');
};
```

后端已有端点: `PUT /api/workflows/:id` (update_workflow_handler)

### 2.5 节点配置 State 绑定

当前问题: `workflow-manager.tsx` 中右侧配置面板的 Input onChange 不更新 canvasNodes state。

改造:
```typescript
// 配置面板 Input
<input
  value={selectedNode.config?.prompt || ''}
  onChange={(e) => {
    setCanvasNodes(prev => prev.map(n =>
      n.id === selectedNode.id
        ? { ...n, config: { ...n.config, prompt: e.target.value } }
        : n
    ));
  }}
/>
```

---

## 3. Workflow 导出/导入

### 3.1 导出

```typescript
const handleExport = () => {
  const definition = buildDefinitionFromCanvas();
  const blob = new Blob([JSON.stringify(definition, null, 2)], { type: 'application/json' });
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = `${currentWorkflowName || 'workflow'}.json`;
  a.click();
  URL.revokeObjectURL(url);
};
```

### 3.2 导入

```typescript
const handleImport = async () => {
  const file = await pickFile({ accept: '.json,.yaml,.yml' });
  const content = await file.text();

  const definition = parseWorkflowDefinition(content);

  // 创建新工作流
  await rpcCall('workflow.create', {
    name: definition.name,
    definition: JSON.stringify(definition),
  });

  // 刷新列表
  await loadWorkflows();
};
```

文件选择使用浏览器原生 `<input type="file">` (无需额外库)。

---

## 4. SDK 完整封装

### 4.1 新增模块结构

```
packages/sdk/src/
├── rpc-client.ts            — 已有 (通用 RPC)
├── agent-client.ts          — 已有 (WebSocket)
├── workflow-builder.ts      — 已有 (构建器)
├── maple-client.ts          — 新增 (业务 RPC + REST 统一封装)
├── event-subscription.ts    — 新增 (SSE 通用订阅)
├── auth-manager.ts          — 新增 (认证管理)
├── types.ts                 — 改造 (补全所有业务类型)
├── index.ts                 — 改造 (导出所有模块)
```

### 4.2 MapleClient (业务统一封装)

```typescript
export class MapleClient {
  private rpc: RpcClient;
  private baseUrl: string;
  private authManager: AuthManager;

  constructor(config: { rpcUrl: string; restUrl: string }) {
    this.rpc = new RpcClient(config.rpcUrl);
    this.baseUrl = config.restUrl;
    this.authManager = new AuthManager(this.baseUrl);
  }

  // Workflow
  workflowList()              { return this.rpc.request<WorkflowItem[]>('workflow.list'); }
  workflowCreate(name: string, definition: string) { return this.rpc.request('workflow.create', { name, definition }); }
  workflowExecute(id: string) { return this.rpc.request('workflow.execute', { workflow_id: id }); }

  // Agent
  agentList()                 { return this.rpc.request<AgentListItem[]>('agent.list'); }
  agentRegister(name: string) { return this.rpc.request('agent.register', { name }); }
  agentDeregister(id: string) { return this.rpc.request('agent.deregister', { id }); }
  agentChat(agentId: string, message: string) { return this.rpc.request('agent.chat', { agent_id: agentId, message }); }

  // LLM
  llmModels()                 { return this.rpc.request<ModelInfo[]>('llm.models'); }

  // Skill
  skillList()                 { return this.rpc.request<SkillInfo[]>('skill.list'); }
  skillInstall(skillId: string) { return this.rpc.request('skill.install', { skill_id: skillId }); }
  skillUninstall(skillId: string) { return this.rpc.request('skill.uninstall', { skill_id: skillId }); }

  // Task
  taskCreate(params: TaskCreateParams) { return this.rpc.request('task.create', params); }

  // Config
  configGet()                 { return this.rpc.request<AppConfig>('config.get'); }
  configUpdate(config: Partial<AppConfig>) { return this.rpc.request('config.update', config); }

  // Scale
  scaleTools()                { return this.rpc.request('scale.tools'); }
  scaleCall(toolName: string, args: Record<string, any>) { return this.rpc.request('scale.call', { tool_name: toolName, arguments: args }); }

  // REST API
  kbSearch(query: string, topK: number = 5) { return this.restGet('/api/kb/search', { query, top_k: topK }); }
  kbIndex(title: string, content: string, sourceType: string) { return this.restPost('/api/kb/index', { title, content, source_type: sourceType }); }
  kbDocuments()               { return this.restGet('/api/kb/documents'); }
  kbUpload(file: File)        { /* FormData upload */ }
  sessionsList()              { return this.restGet('/api/sessions'); }
  sessionMessages(id: string) { return this.restGet(`/api/sessions/${id}/messages`); }
  memoriesSearch(keyword: string, memoryType?: string) { return this.restPost('/api/memories/search', { keyword, memory_type: memoryType }); }
  tasksStats()                { return this.restGet('/api/tasks/stats');
  }
}
```

### 4.3 EventSubscription (SSE 通用订阅)

```typescript
export class EventSubscription {
  private eventSource: EventSource | null = null;
  private listeners: Map<string, Set<(data: any) => void>> = new Map();
  private reconnectTimer: number | null = null;

  constructor(url: string, eventTypes: string[]) {
    this.connect(url, eventTypes);
  }

  private connect(url: string, eventTypes: string[]) {
    this.eventSource = new EventSource(url);

    for (const type of eventTypes) {
      this.eventSource.addEventListener(type, (e) => {
        const data = JSON.parse(e.data);
        this.listeners.get(type)?.forEach(cb => cb(data));
      });
    }

    this.eventSource.onerror = () => {
      this.eventSource?.close();
      this.reconnectTimer = setTimeout(() => this.connect(url, eventTypes), 5000);
    };
  }

  on(eventType: string, callback: (data: any) => void) {
    if (!this.listeners.has(eventType)) this.listeners.set(eventType, new Set());
    this.listeners.get(eventType)!.add(callback);
    return () => this.listeners.get(eventType)?.delete(callback);
  }

  close() {
    this.eventSource?.close();
    if (this.reconnectTimer) clearTimeout(this.reconnectTimer);
  }
}

// 使用示例
const subscription = new EventSubscription('/api/maple/api/events', [
  'node.started', 'node.completed', 'node.failed',
  'workflow.completed', 'workflow.failed'
]);
subscription.on('node.completed', (data) => { updateNodeStatus(data); });
```

---

## 5. Zustand 状态管理

### 5.1 Store 设计

```typescript
// stores/auth-store.ts
export const useAuthStore = create<AuthState>()(
  persist(
    (set) => ({
      token: null,
      user: null,
      isAuthenticated: false,
      setAuth: (token, user) => set({ token, user, isAuthenticated: true }),
      clearAuth: () => set({ token: null, user: null, isAuthenticated: false }),
    }),
    { name: 'mapleos-auth', storage: createJSONStorage(() => localStorage) }
  )
);

// stores/chat-store.ts
export const useChatStore = create<ChatState>((set, get) => ({
  sessions: [],
  currentSessionId: null,
  messages: [],
  selectedAgent: null,
  sending: false,
  setSessions: (sessions) => set({ sessions }),
  setCurrentSession: (id) => set({ currentSessionId: id }),
  setMessages: (messages) => set({ messages }),
  addMessage: (msg) => set({ messages: [...get().messages, msg] }),
  updateLastAssistant: (content) => set({
    messages: get().messages.map((m, i) =>
      i === get().messages.length - 1 && m.role === 'assistant'
        ? { ...m, content }
        : m
    ),
  }),
}));

// stores/workflow-store.ts
export const useWorkflowStore = create<WorkflowState>((set) => ({
  workflows: [],
  currentWorkflowId: null,
  canvasNodes: [],
  canvasEdges: [],
  // ...
}));
```

### 5.2 迁移策略

分 3 步渐进迁移, 不一次性重构:
1. Auth: `api.ts` 中 `setAuthState/clearAuthState` → `useAuthStore`
2. Chat: `chat-panel.tsx` 中 sessions/messages state → `useChatStore`
3. Workflow: `workflow-manager.tsx` 中 canvas/workflows state → `useWorkflowStore`

---

## 6. Framer Motion 动效

### 6.1 页面切换动画

```typescript
// page.tsx 页面切换包裹
import { AnimatePresence, motion } from 'framer-motion';

<AnimatePresence mode="wait">
  <motion.div
    key={activeNav}
    initial={{ opacity: 0, y: 20 }}
    animate={{ opacity: 1, y: 0 }}
    exit={{ opacity: 0, y: -20 }}
    transition={{ duration: 0.2 }}
  >
    {renderActiveView()}
  </motion.div>
</AnimatePresence>
```

### 6.2 列表动画

```typescript
// LayoutGroup + motion.li
<motion.ul layout>
  {workflows.map(wf => (
    <motion.li key={wf.id} layout initial={{ opacity: 0 }} animate={{ opacity: 1 }}>
      ...
    </motion.li>
  ))}
</motion.ul>
```

---

## 7. 文件变更清单

| # | 文件 | 操作 | 说明 |
|---|------|------|------|
| 2.1 | `apps/web/src/components/chat-panel.tsx` | 改造 | Session 切换加载历史 + zustand 迁移 |
| 2.2 | `apps/web/src/components/workflow-manager.tsx` | 改造 | 加载已有工作流 + 节点配置绑定 + 保存 + 导出/导入 + zustand |
| 2.3 | `apps/web/src/stores/auth-store.ts` | 新增 | Auth zustand store |
| 2.4 | `apps/web/src/stores/chat-store.ts` | 新增 | Chat zustand store |
| 2.5 | `apps/web/src/stores/workflow-store.ts` | 新增 | Workflow zustand store |
| 2.6 | `apps/web/src/app/page.tsx` | 改造 | framer-motion 页面切换动画 |
| 2.7 | `apps/web/src/lib/api.ts` | 改造 | 迁移 Auth 到 zustand |
| 2.8 | `packages/sdk/src/maple-client.ts` | 新增 | 业务 RPC + REST 统一封装 |
| 2.9 | `packages/sdk/src/event-subscription.ts` | 新增 | SSE 通用订阅 |
| 2.10 | `packages/sdk/src/auth-manager.ts` | 新增 | Auth 管理 |
| 2.11 | `packages/sdk/src/types.ts` | 改造 | 补全所有业务类型定义 |
| 2.12 | `packages/sdk/src/index.ts` | 改造 | 导出新模块 |

---

## 8. 风险与应对

| 风险 | 影响 | 应对策略 |
|------|------|---------|
| zustand 迁移影响现有功能 | 状态管理切换可能引入 bug | 渐进迁移，每次迁移一个模块，迁移完跑 E2E 验证 |
| Workflow 定义格式不一致 | 后端可能存 JSON 或 YAML | parseWorkflowDefinition 双格式兼容(先 JSON 后 YAML) |
| 后端 workflow 无 position 字段 | 已存工作流加载到画布时节点位置混乱 | dagre 自动布局算法兜底 |
| SDK 接口变更影响前端 | 前端内联调用 → SDK 方法迁移需改多处 | 保留 api.ts 的 rpcCall/mapleApi 作为过渡，SDK 和 api.ts 并行使用 |
| framer-motion 性能 | 部分低端设备动画卡顿 | 使用 `willChange` 提示 + `transition: { duration: 0.15 }` 缩短动画时间 |