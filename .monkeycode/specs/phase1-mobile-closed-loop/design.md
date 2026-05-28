# Phase 1: 移动端闭环 — 技术设计规格说明书

> 日期: 2026-05-26
> 优先级: P0
> 依赖: 后端 `/api/auth/*` + `/api/chat/stream` + `/api/kb/*` 已实现

---

## 1. 技术架构总览

```
Mobile App (Expo 52 + Expo Router 4)
├── app/
│   ├── auth/
│   │   ├── login.tsx       — 登录页
│   │   └── register.tsx    — 注册页
│   ├── (tabs)/
│   │   ├── dashboard.tsx   — 已有
│   │   ├── chat.tsx        — 改造: SSE 流式
│   │   ├── knowledge.tsx   — 改造: 索引功能
│   │   └── agents.tsx      — 已有
│   └── _layout.tsx         — 改造: Auth guard
├── src/lib/
│   ├── api.ts              — 改造: SSE 支持
│   ├── auth-context.tsx    — 新增: Auth 状态管理
│   └── sse-client.ts       — 新增: SSE 通用客户端
└── package.json            — 改造: 补依赖
```

---

## 2. Auth 模块设计

### 2.1 架构方案

采用 React Context + AsyncStorage 持久化:

```typescript
// src/lib/auth-context.tsx
interface AuthState {
  accessToken: string | null;
  refreshToken: string | null;
  user: { id: string; username: string; role: string } | null;
  isAuthenticated: boolean;
  isLoading: boolean;          // 启动时 token 验证中
}

interface AuthContextValue extends AuthState {
  login: (username: string, password: string) => Promise<void>;
  register: (username: string, password: string, email?: string) => Promise<void>;
  logout: () => Promise<void>;
  refreshAuth: () => Promise<void>;  // token 刷新
}
```

### 2.2 Token 管理

| 操作 | 流程 |
|------|------|
| 登录 | POST `/api/auth/login` → 存储 access+refresh 到 AsyncStorage → 设置 Context state |
| 注册 | POST `/api/auth/register` → 自动调用 login 流程 |
| Token 刷新 | POST `/api/auth/refresh` → rotation refresh token → 更新 AsyncStorage |
| 启动恢复 | AsyncStorage.read → 验证 access_token(可选调 `/api/auth/refresh`) → 设置 state |
| 401 处理 | API 调用返回 401 → refreshAuth() → 仍失败则 logout() → 跳转登录页 |

### 2.3 Auth Guard (Layout)

```typescript
// app/_layout.tsx RootLayout
function RootLayout() {
  const { isAuthenticated, isLoading } = useAuth();

  if (isLoading) return <SplashScreen />;
  if (!isAuthenticated) return <Redirect to="/auth/login" />;

  return <TabsLayout />;
}
```

Expo Router 文件路由方式: `app/auth/login.tsx` 和 `app/auth/register.tsx` 作为独立路由，不受 Tab 导航约束。

### 2.4 登录页 UI

| 元素 | 规格 |
|------|------|
| 页面背景 | #0f0f23 (暗色主题) |
| Logo | MapleOS 文字 Logo，居中顶部 |
| 用户名输入 | 左侧图标，placeholder "请输入用户名" |
| 密码输入 | 左侧图标，placeholder "请输入密码"，右侧可切换显示/隐藏 |
| 登录按钮 | #6366f1 主色，full width，圆角 12px |
| 注册链接 | 底部文字 "没有账号？注册" |
| 错误提示 | #EF4444 红色 Toast，3s 自动消失 |

### 2.5 注册页 UI

| 元素 | 规格 |
|------|------|
| 用户名输入 | 同登录 |
| 密码输入 | 含 8+ 字符提示 |
| 邮箱输入 | 选填，placeholder "邮箱(选填)" |
| 注册按钮 | 同登录按钮样式 |
| 返回登录 | 底部文字 "已有账号？登录" |

---

## 3. Chat SSE 流式设计

### 3.1 问题分析

Expo React Native 环境**无原生 EventSource API**。需要选择替代方案:

| 方案 | 优点 | 缺点 | 推荐 |
|------|------|------|------|
| A. `eventsource-polyfill` | 标准 SSE 协议，与 Web 一致 | RN 环境可能有 fetch 兼容问题 | 备选 |
| B. `react-native-sse` | RN 专用 SSE 库 | 第三方包，社区活跃度低 | 不推荐 |
| C. ReadableStream + TextDecoder | 与 Web Chat 一致，无额外依赖 | 需手动解析 SSE 格式 | **推荐** |
| D. 长轮询 REST `/api/chat` | 最简单实现 | 无流式体验，违背核心需求 | 不推荐 |

### 3.2 推荐方案: ReadableStream (方案 C)

与 Web 端 `chat-panel.tsx` 实现方式一致:

```typescript
// src/lib/sse-client.ts
interface SseOptions {
  url: string;
  body: Record<string, string>;
  headers?: Record<string, string>;
  onToken: (token: string) => void;
  onError: (error: string) => void;
  onDone: (meta: { session_id?: string; model?: string }) => void;
  onKbSources?: (sources: KnowledgeRef[]) => void;
}

async function streamChat(options: SseOptions): Promise<void> {
  const response = await fetch(options.url, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      'Authorization': `Bearer ${accessToken}`,
      ...options.headers,
    },
    body: JSON.stringify(options.body),
  });

  if (!response.ok) {
    const err = await response.json().catch(() => ({ message: '请求失败' }));
    options.onError(err.message || `HTTP ${response.status}`);
    return;
  }

  const reader = response.body?.getReader();
  if (!reader) { options.onError('无响应流'); return; }

  const decoder = new TextDecoder();
  let buffer = '';
  let currentEvent = '';

  while (true) {
    const { done, value } = await reader.read();
    if (done) break;
    buffer += decoder.decode(value, { stream: true });

    const lines = buffer.split('\n');
    buffer = lines.pop() || '';

    for (const line of lines) {
      if (line.startsWith('event:')) {
        currentEvent = line.slice(6).trim();
      } else if (line.startsWith('data:')) {
        const data = line.slice(5).trim();
        if (currentEvent === 'token') {
          options.onToken(data);
        } else if (currentEvent === 'error') {
          options.onError(JSON.parse(data).message || data);
        } else if (currentEvent === 'done') {
          options.onDone(JSON.parse(data));
        } else if (currentEvent === 'kb_sources') {
          options.onKbSources?.(JSON.parse(data));
        }
      }
    }
  }
}
```

### 3.3 Chat 页面改造

```typescript
// app/(tabs)/chat.tsx 改造要点

// 1. 消息发送改为 SSE 流式
const handleSend = async (text: string) => {
  setSending(true);
  const userMsg = { role: 'user', content: text };
  setMessages(prev => [...prev, userMsg]);

  // 添加空的 assistant 消息占位
  const assistantMsg = { role: 'assistant', content: '' };
  setMessages(prev => [...prev, assistantMsg]);
  const msgIndex = messages.length + 1; // assistant 消息索引

  await streamChat({
    url: `${API_BASE_URL}/api/chat/stream`,
    body: { message: text, agent_id: selectedAgent },
    onToken: (token) => {
      setMessages(prev => {
        const updated = [...prev];
        updated[msgIndex] = { ...updated[msgIndex], content: updated[msgIndex].content + token };
        return updated;
      });
    },
    onError: (error) => {
      setMessages(prev => {
        const updated = [...prev];
        updated[msgIndex] = { ...updated[msgIndex], content: `[错误] ${error}` };
        return updated;
      });
    },
    onDone: (meta) => {
      setSending(false);
      // 可更新 session_id 等
    },
    onKbSources: (sources) => {
      setMessages(prev => {
        const updated = [...prev];
        updated[msgIndex] = { ...updated[msgIndex], kb_sources: sources };
        return updated;
      });
    },
  });
};

// 2. 显示 "正在思考..." 指示器
// 当 assistantMsg.content === '' 且 sending === true 时显示

// 3. KB 引用卡片渲染
// KnowledgeRefCard: 显示 source_type Badge + score 进度条 + snippet
```

### 3.4 API 调用变更

| 原调用 | 新调用 | 变更 |
|--------|--------|------|
| `mobileRpcCall("agent.chat", { message, agent_id })` | `streamChat({ url: "/api/chat/stream", ... })` | 从同步 RPC 改为 SSE 流式 |
| `mobileRpcCall("agent.list")` | 保持不变 | Agent 列表仍用 RPC |

---

## 4. Knowledge 索引设计

### 4.1 文本索引

```typescript
// 知识库索引表单组件
const handleIndexText = async () => {
  const result = await mobileRestCall('/api/kb/index', {
    method: 'POST',
    body: { title: inputTitle, content: inputContent, source_type: selectedSourceType },
  });
  // 成功后刷新文档列表
  await loadDocuments();
};
```

UI: 模态弹窗(Modal)，包含:
- 标题输入 (TextInput)
- 内容输入 (TextInput, multiline, minHeight 120px)
- source_type 下拉 (document/faq/log)
- "添加" 按钮 (#6366f1 主色)

### 4.2 文件上传索引

```typescript
// 文件选择 + 上传
import * as DocumentPicker from 'expo-document-picker';

const handleUploadFile = async () => {
  const result = await DocumentPicker.getDocumentAsync({
    type: ['application/pdf', 'text/plain', 'text/markdown'],
    multiple: false,
  });

  if (result.canceled) return;

  const formData = new FormData();
  formData.append('file', {
    uri: result.assets[0].uri,
    name: result.assets[0].name,
    type: result.assets[0].mimeType,
  } as any);

  const uploadResult = await fetch(`${API_BASE_URL}/api/kb/upload`, {
    method: 'POST',
    headers: { 'Authorization': `Bearer ${accessToken}` },
    body: formData,
  });

  // 成功后刷新文档列表
  await loadDocuments();
};
```

新增依赖: `expo-document-picker` (Expo 官方包)

---

## 5. AsyncStorage 依赖修复

### 5.1 package.json 补声明

```json
{
  "dependencies": {
    "@react-native-async-storage/async-storage": "1.23.1",
    ...已有依赖
  }
}
```

版本选择: 1.23.1 是与 Expo 52 兼容的最新稳定版。

---

## 6. 文件变更清单

| # | 文件 | 操作 | 说明 |
|---|------|------|------|
| 1.1 | `apps/mobile/app/auth/login.tsx` | 新增 | 登录页面 |
| 1.2 | `apps/mobile/app/auth/register.tsx` | 新增 | 注册页面 |
| 1.3 | `apps/mobile/src/lib/auth-context.tsx` | 新增 | Auth Context + AsyncStorage 持久化 |
| 1.4 | `apps/mobile/src/lib/sse-client.ts` | 新增 | SSE ReadableStream 通用客户端 |
| 1.5 | `apps/mobile/app/_layout.tsx` | 改造 | 加入 Auth guard (未登录跳转) |
| 1.6 | `apps/mobile/app/(tabs)/chat.tsx` | 改造 | Chat 从同步 RPC 改为 SSE 流式 |
| 1.7 | `apps/mobile/app/(tabs)/knowledge.tsx` | 改造 | 去除 "coming soon"，实现索引+上传 |
| 1.8 | `apps/mobile/src/lib/api.ts` | 改造 | 补 401 自动 refresh + Bearer token |
| 1.9 | `apps/mobile/package.json` | 改造 | 补 AsyncStorage + expo-document-picker |
| 1.10 | `apps/mobile/src/components/knowledge-ref-card.tsx` | 新增 | KB 引用卡片组件 |

---

## 7. 风险与应对

| 风险 | 影响 | 应对策略 |
|------|------|---------|
| RN ReadableStream 兼容性 | 某些 RN 版本不支持 ReadableStream | 降级方案: 使用 `react-native-fetch-api` polyfill 或 `eventsource-polyfill` |
| Expo Document Picker iOS 权限 | iOS 需声明文件访问权限 | 在 app.json 中配置 `ios.infoPlist.UIDocumentTypes` |
| SSE 连接超时/断开 | 流式输出中断 | 已接收 token 保留显示，尾部标记 "(连接中断)"，提供重试按钮 |
| AsyncStorage 加密 | Token 明文存储 | 后续可选引入 `expo-secure-store` 替代 AsyncStorage 存储 token |