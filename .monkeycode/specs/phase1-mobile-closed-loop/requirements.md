# Phase 1: 移动端闭环 — 需求规格说明书

> 日期: 2026-05-26
> 优先级: P0
> 目标: 让 Mobile 端用户能走通注册->登录->对话->知识检索的完整路径

---

## 1. 用户登录与注册 (MUST)

### 1.1 登录页面

**EARS 模式**: 当用户打开 MapleOS 移动端应用时，系统应展示登录页面，用户输入用户名和密码后可登录并获得 JWT token。

**需求明细**:
- 输入字段: 用户名(必填)、密码(必填)
- 登录成功: JWT access_token + refresh_token 存入 AsyncStorage，跳转至 Dashboard Tab
- 登录失败: 显示错误提示(用户名或密码错误)
- Token 过期处理: 401 响应时自动尝试 refresh，失败则跳转登录页
- 已登录状态: 启动时检查 AsyncStorage token 有效性，有效则直接进入 Dashboard

### 1.2 注册页面

**EARS 模式**: 当用户在登录页点击"注册"时，系统应展示注册表单，用户填写信息后可创建新账号。

**需求明细**:
- 输入字段: 用户名(必填,3-32字符)、密码(必填,8+字符)、邮箱(选填)
- 注册成功: 自动登录并跳转 Dashboard
- 注册失败: 用户名已存在时提示"用户名已被占用"
- 注册后自动登录: 无需手动回到登录页重新输入

---

## 2. 移动端 Chat 流式对话 (MUST)

### 2.1 SSE 流式输出

**EARS 模式**: 当用户在移动端 Chat 页面发送消息时，系统应以 SSE 流式方式逐 token 展示 Agent 回复，用户应在发送后 1 秒内看到第一个 token。

**需求明细**:
- 发送消息后立即在消息区域显示 "正在思考..." 指示器
- 逐 token 增量渲染回复文本，不等待完整响应
- 支持 SSE 事件类型: token(增量文本) / error(错误消息) / done(完成标记)
- 流式完成后显示完整回复，含 kb_sources 引用卡片(如有)
- 网络中断时: 已接收的 token 保留显示，尾部标记"(连接中断)"
- 当前状态: Mobile Chat 使用同步 RPC `agent.chat`，需改为 SSE 流式

### 2.2 Agent 选择与切换

**EARS 模式**: 当用户在 Chat 页面点击 Agent 选择器时，系统应列出所有可用 Agent，用户选择后后续对话使用该 Agent。

**需求明细**:
- Agent 列表: 从 `rpcCall("agent.list")` 加载
- 选中 Agent: 显示 Agent 名称和状态(Online/Busy/Offline)
- 切换 Agent: 不清空当前消息历史，新消息路由到新 Agent

---

## 3. 移动端 Knowledge 索引 (SHOULD)

### 3.1 文本索引

**EARS 模式**: 当用户在 Knowledge 页面点击"添加知识"时，系统应提供文本输入表单，用户输入标题和内容后可创建知识条目。

**需求明细**:
- 输入字段: 标题(必填)、内容(必填,支持多行文本)、source_type(下拉: document/faq/log)
- 索引成功: 显示成功提示，更新文档列表
- 索引失败: 显示错误信息

### 3.2 文件上传索引

**EARS 模式**: 当用户在 Knowledge 页面点击"上传文件"时，系统应支持选择本地文件(PDF/TXT/MD)，上传后自动索引并添加到知识库。

**需求明细**:
- 支持文件类型: PDF、TXT、MD
- 文件大小限制: 单文件最大 10MB
- 上传进度: 显示上传百分比
- 上传成功: 文件出现在文档列表，可被搜索命中
- 当前状态: 页面标记 "coming soon"，需去除占位并实现真实功能

---

## 4. 依赖修复 (MUST)

### 4.1 AsyncStorage 依赖声明

**EARS 模式**: 当 Mobile 应用启动时，系统应能正常读取 AsyncStorage 存储的 token 和用户信息，不出现模块缺失错误。

**需求明细**:
- 在 apps/mobile/package.json 中声明 `@react-native-async-storage/async-storage` 依赖
- 确保与 Expo 52 兼容的版本

---

## 非功能需求

| 维度 | 要求 |
|------|------|
| 性能 | SSE 流式首 token 延迟 < 1s |
| 安全 | Token 存储使用 AsyncStorage(加密可选) |
| 可用性 | 登录/注册 3 步内完成 |
| 兼容性 | iOS 14+ / Android 10+ |
| 离线 | 登录状态持久化，重启后自动恢复 |

---

## 验收标准

1. Mobile 端可注册新账号并登录
2. 登录后 Token 持久化，关闭应用重开仍保持登录
3. Mobile Chat 发送消息后 1s 内看到第一个 token 流式输出
4. Mobile Knowledge 可索引文本内容并搜索命中
5. Mobile Knowledge 可上传 PDF/TXT/MD 文件并自动索引
6. Mobile 应用启动不出现 AsyncStorage 模块缺失错误