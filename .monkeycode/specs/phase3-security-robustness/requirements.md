# Phase 3: 安全与健壮性 — 需求规格说明书

> 日期: 2026-05-26
> 优先级: P1
> 目标: 修复安全缺陷、提升系统健壮性、完善 CI 自动化

---

## 1. code_execute 安全沙箱 (MUST)

### 1.1 沙箱隔离执行

**EARS 模式**: 当 Agent 或用户通过 `code_execute` 技能执行代码时，系统应在隔离沙箱环境中运行，确保执行的代码无法访问宿主机的文件系统、网络和进程。

**需求明细**:
- 当前状态: `code_execute` 直接调用 `node -e` 和 `python3` 执行代码，无任何隔离
- 安全风险: 执行代码可读写任意文件、发起网络请求、启动进程
- 需求: 所有代码执行在沙箱内完成，沙箱内进程无法逃逸
- 资源限制: 内存上限 256MB，CPU 时间上限 10s(默认)/30s(最大)
- 网络限制: 沙箱内无网络访问(或仅白名单域名)
- 文件限制: 沙箱内仅可读写 `/workspace` 子目录，不可访问宿主机其他路径

### 1.2 支持的语言

**EARS 模式**: 当用户通过 `code_execute` 提交代码时，系统应支持 JavaScript 和 Python 两种语言，用户通过 `language` 参数指定。

**需求明细**:
- JavaScript: Node.js 运行时
- Python: Python 3 运行时
- 错误处理: 语法错误、运行时错误、超时终止均返回结构化错误信息
- 输出截断: stdout/stderr 合计最大 8KB，超出截断并标记 "[output truncated]"

---

## 2. file_ops 路径安全校验加固 (MUST)

### 2.1 Canonicalize 路径校验

**EARS 模式**: 当用户通过 `file_ops` 技能操作文件时，系统应使用 canonicalize 规范化路径后校验，确保操作路径始终在 workspace 目录内，防止符号链接和路径逃逸攻击。

**需求明细**:
- 当前状态: 使用字符串前缀匹配 `path.starts_with(workspace_root)` 检查
- 安全风险: 符号链接逃逸(`../`或 symlink 指向外部)、路径拼接绕过
- 需求: 使用 `std::fs::canonicalize` 规范化路径后校验
- 校验逻辑: `canonicalize(path).starts_with(canonicalize(workspace_root))`
- 禁止操作: `/etc`, `/home`, `/root`, `/var` 等系统目录
- 白名单路径: 仅 `$CWD/workspace` 及子目录

---

## 3. Skill 持久化 (SHOULD)

### 3.1 MCP Skill 安装持久化

**EARS 模式**: 当用户通过 `skill.install` 安装 MCP 技能后，系统应将安装信息持久化到数据库，服务重启后自动恢复已安装的 MCP 技能。

**需求明细**:
- 当前状态: MCP skill 安装仅存内存(McpHostManager HashMap)，重启后丢失
- 需求: 安装时写入 `installed_skills` 表(或 kv_store)
- 重启恢复: main.rs 启动时从 DB 读取已安装 skill，重新启动 MCP server 进程
- 卸载同步: `skill.uninstall` 同时删除 DB 记录和停止 MCP 进程

---

## 4. CI 日常分支触发 (MUST)

### 4.1 PR/push 自动触发

**EARS 模式**: 当开发者推送代码到任意分支或创建 PR 时，系统应自动触发 CI 流水线执行 lint + test + build 检查。

**需求明细**:
- 当前状态: CI 仅在 `push tags: v*` 时触发，日常开发无自动检查
- 需求: push 到任意分支触发 `ci.yml`
- PR 检查: PR 创建/更新时触发 ci.yml，结果展示在 PR 页面
- CI 内容: cargo check + cargo test + cargo clippy + pnpm build + pnpm typecheck
- 失败阻止: PR CI 失败时不可 merge (branch protection rule)

---

## 5. memory_search 接口双模式 (SHOULD)

### 5.1 GET query 参数支持

**EARS 模式**: 当前端调用 `/api/memories/search` 时，系统应同时支持 GET query 参数和 POST body 两种调用方式，确保不同客户端均可正常使用。

**需求明细**:
- 当前状态: 仅支持 POST body `{ keyword, memory_type, limit }`
- 需求: 同时支持 `GET /api/memories/search?query=X&limit=10`
- 参数映射: GET `query` → POST `keyword`, GET `limit` → POST `limit`

---

## 验收标准

1. code_execute 执行的代码无法访问宿主机文件系统(路径逃逸测试用例)
2. code_execute 执行超时(>10s)的代码被自动终止
3. file_ops 通过符号链接逃逸测试(创建 symlink 指向外部，file_ops 拒绝操作)
4. Skill 安装后重启服务，已安装 skill 自动恢复
5. PR push 触发 CI，cargo check + test + clippy + frontend build 全通过
6. `GET /api/memories/search?query=test&limit=5` 返回正确结果