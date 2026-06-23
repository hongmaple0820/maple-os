# MapleOS 产品闭环修复方案

## 目标

让用户能完整走通核心路径：**注册 -> 登录 -> 创建 Agent -> 上传知识 -> 对话 -> 管理工作流**

## 当前状态

核心闭环 **未走通**，5 个关键断点阻塞。

---

## Phase 1: 认证体系（P0，阻塞第一步）

### 问题
- 无注册端点，仅硬编码 admin 账号
- 前端无登录页面
- JWT Token 未持久化存储
- `require_auth` 默认 false，中间件形同虚设
- 无 users 表

### 修复项

| # | 文件 | 改动 |
|---|------|------|
| 1.1 | `migrations/002_users.sql` | 新建 users 表 (id, username, password_hash, email, created_at) + api_tokens 表 |
| 1.2 | `server/src/main.rs` | 新增 `POST /api/auth/register`（注册，bcrypt 密码） |
| 1.3 | `server/src/main.rs` | 修改 `POST /api/auth/login`：查 users 表验证，返回 access_token + refresh_token |
| 1.4 | `server/src/main.rs` | 新增 `POST /api/auth/refresh`：刷新 token |
| 1.5 | `server/src/main.rs` | auth middleware：从 Authorization header 提取 Bearer token 验证 |
| 1.6 | `apps/web/src/components/login-page.tsx` | 新建登录页：用户名+密码表单，token 存 localStorage |
| 1.7 | `apps/web/src/components/register-page.tsx` | 新建注册页 |
| 1.8 | `apps/web/src/lib/api.ts` | mapleApi 自动附加 Authorization header，401 时跳转登录页 |
| 1.9 | `apps/mobile/src/lib/api.ts` | mobileRpcCall/mobileRestCall 自动附加 token，401 时提示 |
| 1.10 | `server/src/main.rs` | 默认 `require_auth=true`（开发环境可通过 env 关闭） |

### 验收标准
- 用户可注册新账号
- 登录后 token 持久化，刷新页面不丢失
- 未登录访问受保护 API 返回 401
- 前端 401 自动跳转登录页

---

## Phase 2: API 对齐（P0，阻塞前后端通信）

### 问题
- REST `/api/chat` 用 `message`/`reply`，RPC `agent.chat` 用 `prompt`/`response`
- Mobile `agent.chat` 传 `message` 但后端 RPC 期望 `prompt`
- Mobile `task.create` 传嵌套 `payload` 但后端 RPC 期望扁平参数
- REST 和 RPC 两套接口命名不一致

### 修复项

| # | 文件 | 改动 |
|---|------|------|
| 2.1 | `server/src/main.rs` | RPC `agent.chat`：参数名 `prompt` -> `message`，返回字段 `response` -> `reply`，兼容旧名 |
| 2.2 | `server/src/main.rs` | RPC `task.create`：兼容扁平参数和嵌套 payload 两种格式 |
| 2.3 | `apps/mobile/app/(tabs)/chat.tsx` | 已用 `message`/`agent_id`，与 2.1 对齐后自动修复 |
| 2.4 | `apps/mobile/app/(tabs)/agents.tsx` | `task.create` 参数改为扁平格式：`{ task_type, agent_id, prompt, priority }` |

### 验收标准
- Mobile chat 能正常发送消息并收到回复
- Mobile agents 能正常下发任务
- Web chat 通过 REST 和 RPC 都能正常工作

---

## Phase 3: 真流式对话（P0，核心体验）

### 问题
- `chat_stream_handler` 先同步等 LLM 完整返回，再按 8 字符切片逐块推送
- 用户等数秒后才看到第一个字，然后快速刷出全部内容，不是真正的流式体验

### 修复项

| # | 文件 | 改动 |
|---|------|------|
| 3.1 | `core/maple-llm/src/adapters/openai_compat.rs` | 实现 `stream()` 方法，返回真正的 SSE token 流 |
| 3.2 | `core/maple-llm/src/adapters/anthropic.rs` | 实现 `stream()` 方法 |
| 3.3 | `core/maple-llm/src/router.rs` | LlmRouter 新增 `route_stream()` 方法，返回 stream adapter |
| 3.4 | `server/src/main.rs` | `chat_stream_handler` 改为：调用 `adapter.stream()`，逐 token yield SSE event |
| 3.5 | `apps/web/src/components/chat-panel.tsx` | SSE 解析已支持 `token` event，无需改动 |

### 验收标准
- 发消息后立即看到第一个 token 逐字出现
- 中间可展示 "正在生成..." 指示器
- LLM 错误实时展示（不等完整响应）

---

## Phase 4: 工作流 JSON/YAML 兼容（P1，阻塞工作流执行）

### 问题
- 前端用 `JSON.stringify()` 生成内容存入 `yaml_content` 字段
- 后端 `Workflow::parse_yaml()` 用 `serde_yaml::from_str()` 解析 JSON，必然失败
- main.rs 调度器用 `serde_json::from_str` 解析同一字段，两处不一致

### 修复项

| # | 文件 | 改动 |
|---|------|------|
| 4.1 | `core/maple-engine/src/workflow.rs` | 新增 `parse_json()` 方法：`serde_json::from_str()` |
| 4.2 | `server/src/main.rs` | `workflow.create` 和执行逻辑：先尝试 `parse_json`，失败再 `parse_yaml`，兼容两种格式 |
| 4.3 | `server/src/main.rs` | 调度器统一用同样的兼容解析逻辑 |
| 4.4 | `apps/web/src/components/workflow-manager.tsx` | 字段名对齐：`type` -> `node_type`，补充 `name`/`version` 等必要字段 |
| 4.5 | `server/src/main.rs` | RPC `workflow.create` 参数重命名：`yaml_content` -> `definition`（兼容旧名） |

### 验收标准
- 前端创建的工作流能成功执行
- 旧 YAML 格式的工作流仍能正常加载

---

## Phase 5: 知识库与对话联动（P1，核心功能闭环）

### 问题
- 知识库搜索和对话是割裂的，对话时不会自动引用知识库
- 无文件上传，仅支持文本粘贴

### 修复项

| # | 文件 | 改动 |
|---|------|------|
| 5.1 | `server/src/main.rs` | Chat handler：对话前自动检索知识库，将相关内容注入 system prompt |
| 5.2 | `apps/web/src/components/chat-panel.tsx` | 展示知识库引用来源（已有 UI 骨架，需接数据） |
| 5.3 | `server/src/main.rs` | 新增 `POST /api/kb/upload`：支持文件上传（PDF/TXT/MD），自动分块索引 |
| 5.4 | `apps/web/src/components/knowledge-manager.tsx` | 添加文件上传组件 |
| 5.5 | `apps/mobile/app/(tabs)/knowledge.tsx` | 添加文件上传按钮 |

### 验收标准
- 上传 PDF 后搜索能找到内容
- 对话时 Agent 自动引用知识库内容
- 对话界面展示引用来源

---

## Phase 6: 前端体验补全（P2，可用性提升）

### 问题
- 工作流画布节点配置未绑定 state（修改不生效）
- 无工作流导出/导入
- Agent 管理无删除/更新
- 知识库无文档列表/删除
- Mobile 无登录认证流程

### 修复项

| # | 文件 | 改动 |
|---|------|------|
| 6.1 | `apps/web/src/components/workflow-manager.tsx` | 节点配置 Input 绑定 state，onChange 更新节点数据 |
| 6.2 | `apps/web/src/components/workflow-manager.tsx` | 工作流导出为 JSON/YAML，导入从文件加载 |
| 6.3 | `server/src/main.rs` | 新增 RPC `agent.deregister`、`agent.update` |
| 6.4 | `server/src/main.rs` | 新增 `GET /api/kb/documents`（文档列表）、`DELETE /api/kb/documents/:id` |
| 6.5 | `apps/web/src/components/agent-manager.tsx` | 添加删除/编辑 Agent 按钮 |
| 6.6 | `apps/web/src/components/knowledge-manager.tsx` | 添加文档列表和删除功能 |
| 6.7 | `apps/mobile/app/` | 新增 login 页面，未登录时跳转 |

### 验收标准
- 工作流画布修改节点配置后能保存并执行
- 可导出/导入工作流
- 可删除 Agent 和知识库文档

---

## Phase 7: 数据持久化与健壮性（P2，生产就绪）

### 问题
- Agent 注册仅存内存，重启丢失
- 记忆搜索仅 LIKE 关键词，无向量检索
- 无审计日志
- Docker 镜像不含前端

### 修复项

| # | 文件 | 改动 |
|---|------|------|
| 7.1 | `migrations/003_agent_persist.sql` | agents 表增加 status/capabilities/registered_at 字段 |
| 7.2 | `server/src/main.rs` | `agent.register` 写入 DB，启动时从 DB 恢复 |
| 7.3 | `core/maple-kb/src/memory.rs` | 记忆搜索接入 vector_store，语义检索替代 LIKE |
| 7.4 | `infra/docker/Dockerfile` | 多阶段构建加入前端静态文件，Nginx 反代 |
| 7.5 | `infra/docker/docker-compose.yml` | 加入前端容器 + Ollama 容器 |

### 验收标准
- 服务重启后 Agent 列表不丢失
- 记忆搜索支持语义检索
- `docker-compose up` 一键启动完整服务

---

## 实施优先级

```
Phase 1 (认证) ──> Phase 2 (API对齐) ──> Phase 3 (真流式) ──> Phase 4 (工作流) ──> Phase 5 (知识联动) ──> Phase 6 (体验) ──> Phase 7 (健壮性)
   P0 必须            P0 必须              P0 必须             P1 重要              P1 重要              P2 提升              P2 提升
```

## 预计工作量

| Phase | 涉及文件数 | 预计改动量 | 关键风险 |
|-------|-----------|-----------|----------|
| Phase 1 | ~10 | 中 | bcrypt 依赖引入、token 刷新竞态 |
| Phase 2 | ~4 | 小 | 兼容旧参数需回归测试 |
| Phase 3 | ~5 | 大 | LLM adapter stream() 需处理 SSE 解析、超时、错误中断 |
| Phase 4 | ~4 | 中 | JSON/YAML 双格式兼容需充分测试 |
| Phase 5 | ~5 | 中 | PDF 解析需引入依赖，system prompt 注入需控制长度 |
| Phase 6 | ~6 | 中 | 前端改动多，需逐个组件验证 |
| Phase 7 | ~5 | 大 | Agent 持久化涉及状态机恢复，向量检索需 Qdrant 集成 |

## Phase 完成后的闭环状态

| Phase 完成后 | 注册 | 登录 | 创建Agent | 上传知识 | 对话 | 工作流 |
|-------------|------|------|----------|---------|------|--------|
| Phase 1 | ✅ | ✅ | - | - | - | - |
| Phase 2 | ✅ | ✅ | ✅ | - | ✅(Mobile) | - |
| Phase 3 | ✅ | ✅ | ✅ | - | ✅(真流式) | - |
| Phase 4 | ✅ | ✅ | ✅ | - | ✅ | ✅ |
| Phase 5 | ✅ | ✅ | ✅ | ✅ | ✅(知识联动) | ✅ |
| **完整闭环** | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |

**Phase 5 完成后核心闭环走通。Phase 6-7 为体验优化和生产就绪。**