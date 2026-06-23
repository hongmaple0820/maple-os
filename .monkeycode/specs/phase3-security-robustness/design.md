# Phase 3: 安全与健壮性 — 技术设计规格说明书

> 日期: 2026-05-26
> 优先级: P1

---

## 1. code_execute 安全沙箱

### 1.1 方案选型

| 方案 | 优点 | 缺点 | 推荐 |
|------|------|------|------|
| A. Docker 容器 | 完全隔离，成熟方案 | 需要 Docker runtime，部署复杂 | 备选(有 Docker 环境) |
| B. WASM wasmtime | 纯 Rust，无外部依赖，local-first | WASM 生态限制(Python WASM 支持有限) | **推荐(无 Docker)** |
| C. nsjail | Linux 专用，轻量级 | 仅 Linux，需 root | 不推荐 |
| D. Firecracker microVM | 极强隔离 | 需 KVM，重 | 不推荐 |

### 1.2 推荐方案: Docker 优先 + WASM fallback

策略: 检测 Docker runtime 可用时用 Docker，否则用 WASM。

```rust
// server/src/main.rs skill_registry code_execute 改造

fn detect_docker() -> bool {
    std::process::Command::new("docker")
        .arg("info")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}
```

### 1.3 Docker 容器执行方案

```rust
struct DockerCodeExecutor {
    docker_available: bool,
    workspace_dir: String,
}

impl DockerCodeExecutor {
    async fn execute(&self, code: &str, language: &str, timeout_secs: u64) -> Result<CodeResult> {
        let image = match language {
            "javascript" | "node" => "node:20-slim",
            "python" => "python:3.12-slim",
            _ => return Err("unsupported language"),
        };

        // 写入临时文件到 workspace/.sandbox/
        let sandbox_dir = format!("{}/.sandbox/{}", self.workspace_dir, uuid());
        std::fs::create_dir_all(&sandbox_dir)?;
        let filename = match language {
            "javascript" | "node" => "code.js",
            "python" => "code.py",
            _ => "code.txt",
        };
        std::fs::write(format!("{}/{}", sandbox_dir, filename), code)?;

        // Docker run with constraints
        let output = tokio::process::Command::new("docker")
            .args([
                "run",
                "--rm",                          // 执行后删除容器
                "--network=none",                // 无网络访问
                "--memory=256m",                 // 内存限制
                "--cpus=1",                      // CPU 限制
                &format!("--stop-timeout={}", timeout_secs),
                "-v", &format!("{}:/workspace:ro", sandbox_dir),  // 只读挂载
                image,
                match language {
                    "javascript" | "node" => "node",
                    "python" => "python3",
                    _ => "cat",
                },
                &format!("/workspace/{}", filename),
            ])
            .output()
            .await?;

        // 清理 sandbox 目录
        std::fs::remove_dir_all(&sandbox_dir)?;

        // 截断输出
        let stdout = truncate(&String::from_utf8_lossy(&output.stdout), 8192);
        let stderr = truncate(&String::from_utf8_lossy(&output.stderr), 4096);

        Ok(CodeResult {
            exit_code: output.status.code().unwrap_or(-1),
            stdout,
            stderr,
            timed_out: output.status.code().is_none(), // 被强制终止
        })
    }
}
```

### 1.4 WASM wastime 执行方案 (无 Docker fallback)

```rust
// Cargo.toml 新增依赖
// wasmtime = "25"
// wasm-component-layer = "0.1"  (可选, 用于 Component Model)

struct WasmCodeExecutor {
    engine: wasmtime::Engine,
}

impl WasmCodeExecutor {
    fn new() -> Self {
        let engine = wasmtime::Engine::new(&wasmtime::Config::new()
            .consume_fuel(true)            // Fuel 限制执行指令数
            .max_wasm_stack(1024 * 64)     // 栈深度限制
        ).unwrap();
        Self { engine }
    }

    async fn execute(&self, wasm_bytes: &[u8], timeout: Duration) -> Result<CodeResult> {
        let mut store = wasmtime::Store::new(&self.engine, ());
        store.set_fuel(1_000_000)?;       // 100万条指令上限

        let module = wasmtime::Module::new(&self.engine, wasm_bytes)?;
        let instance = wasmtime::Linker::new(&self.engine)
            .allow_wasi()?                  // 最小 WASI 支持 (仅 stdout)
            .instantiate_async(&mut store, &module)?;

        // WASI stdout 捕获
        let stdout = Vec::<u8>::new();
        // ... (设置 WASI stdout 到 Vec<u8> buffer)

        // 调用 _start 函数
        let start = instance.get_typed_func::<(), ()>(&mut store, "_start")?;
        start.call_async(&mut store, ())?;

        Ok(CodeResult {
            exit_code: 0,
            stdout: String::from_utf8_lossy(&stdout).to_string(),
            stderr: "",
            timed_out: false,
        })
    }
}
```

**WASM 方案的语言支持策略**:
- JavaScript: 编译为 WASM 使用 `wasmer-js` 或预编译 QuickJS WASM
- Python: 编译为 WASM 使用 `pyodide` WASM 或 `rustpython-wasm`
- 预编译 WASM 二进制随项目分发(存放在 `core/wasm-runtime/prebuilt/`)

### 1.5 SkillRegistry 改造

```rust
// 当前 code_execute skill 改造

pub struct CodeExecuteSkill {
    executor: CodeExecutorStrategy,
}

enum CodeExecutorStrategy {
    Docker(DockerCodeExecutor),
    Wasm(WasmCodeExecutor),
}

impl CodeExecuteSkill {
    fn new(workspace_dir: &str) -> Self {
        let executor = if detect_docker() {
            CodeExecutorStrategy::Docker(DockerCodeExecutor { workspace_dir: workspace_dir.to_string() })
        } else {
            CodeExecutorStrategy::Wasm(WasmCodeExecutor::new())
        };
        Self { executor }
    }
}

impl Skill for CodeExecuteSkill {
    fn execute(&self, params: &serde_json::Value) -> Result<serde_json::Value> {
        let code = params["code"].as_str()?;
        let language = params["language"].as_str().unwrap_or("javascript");
        let timeout = params["timeout"].as_u64().unwrap_or(10).min(30);

        match &self.executor {
            CodeExecutorStrategy::Docker(d) => d.execute(code, language, timeout),
            CodeExecutorStrategy::Wasm(w) => w.execute(precompiled_wasm(code, language), Duration::from_secs(timeout)),
        }
    }
}
```

---

## 2. file_ops 路径安全校验加固

### 2.1 当前实现问题

```rust
// 当前 (不安全):
fn is_path_safe(path: &str, workspace_root: &str) -> bool {
    path.starts_with(workspace_root)
}
```

攻击方式:
- 符号链接: `/workspace/safe_dir/link -> /etc/passwd`, `starts_with` 检查通过
- 路径拼接: `/workspace/../../etc/passwd` (canonicalize 后变为 `/etc/passwd`)

### 2.2 改造方案

```rust
use std::path::PathBuf;

fn canonicalize_and_check(path: &str, workspace_root: &str) -> Result<PathBuf> {
    let requested = PathBuf::from(path);
    let root = std::fs::canonicalize(workspace_root)
        .map_err(|e| format!("workspace root canonicalize failed: {}", e))?;

    // 如果路径不存在(新文件写入), 先 canonicalize 父目录
    let canonical_path = if requested.exists() {
        std::fs::canonicalize(&requested)
            .map_err(|e| format!("path canonicalize failed: {}", e))?
    } else {
        // 新文件: canonicalize 父目录, 然后拼接文件名
        let parent = requested.parent()
            .ok_or("path has no parent directory")?;
        let canonical_parent = std::fs::canonicalize(parent)
            .map_err(|e| format!("parent canonicalize failed: {}", e))?;
        canonical_parent.join(requested.file_name().ok_or("path has no filename")?)
    };

    // 校验: canonical_path 必须在 root 目录内
    if !canonical_path.starts_with(&root) {
        return Err(format!("path escapes workspace: {} not under {}", canonical_path, root));
    }

    // 额外黑名单校验 (防御性)
    let forbidden_prefixes = ["/etc", "/home", "/root", "/var", "/sys", "/proc", "/dev"];
    for prefix in forbidden_prefixes {
        if canonical_path.starts_with(prefix) {
            return Err(format!("path in forbidden zone: {}", canonical_path));
        }
    }

    Ok(canonical_path)
}
```

### 2.3 file_ops skill 改造

```rust
impl Skill for FileOpsSkill {
    fn execute(&self, params: &serde_json::Value) -> Result<serde_json::Value> {
        let operation = params["operation"].as_str()?;
        let path = params["path"].as_str()?;
        let workspace_root = self.workspace_root.as_str();

        // 先做路径安全校验
        let safe_path = canonicalize_and_check(path, workspace_root)
            .map_err(|e| serde_json::json!({ "error": e }))?;

        match operation {
            "read" => {
                let content = std::fs::read_to_string(&safe_path)?;
                Ok(serde_json::json!({ "content": truncate(&content, 65536) }))
            },
            "write" => {
                let content = params["content"].as_str()?;
                std::fs::write(&safe_path, content)?;
                Ok(serde_json::json!({ "success": true }))
            },
            "list" => {
                let entries = std::fs::read_dir(&safe_path)?
                    .filter_map(|e| e.ok())
                    .map(|e| e.file_name().to_string_lossy().to_string())
                    .collect::<Vec<_>>();
                Ok(serde_json::json!({ "entries": entries }))
            },
            "exists" => {
                Ok(serde_json::json!({ "exists": safe_path.exists() }))
            },
            _ => Err("unknown operation"),
        }
    }
}
```

---

## 3. Skill 持久化

### 3.1 数据库设计

新增 `installed_skills` 表:

```sql
-- migrations/004_installed_skills.sql
CREATE TABLE IF NOT EXISTS installed_skills (
    id TEXT PRIMARY KEY,
    skill_type TEXT NOT NULL,           -- 'builtin' / 'mcp'
    name TEXT NOT NULL,
    description TEXT,
    mcp_command TEXT,                    -- MCP server 启动命令 (mcp 类型)
    mcp_args TEXT,                       -- JSON array of args
    mcp_env TEXT,                        -- JSON object of env vars
    mcp_transport TEXT,                  -- 'stdio' / 'http' / 'websocket'
    installed_at INTEGER NOT NULL,
    status TEXT NOT NULL DEFAULT 'active'  -- 'active' / 'disabled'
);

CREATE INDEX idx_installed_skills_type ON installed_skills(skill_type);
```

### 3.2 安装时写入

```rust
// skill.install RPC 改造 (在 McpHostManager 启动 MCP server 后)
// main.rs skill.install handler

// 1. 启动 MCP server (已有逻辑)
// 2. 新增: INSERT INTO installed_skills
db.execute(
    "INSERT INTO installed_skills (id, skill_type, name, description, mcp_command, mcp_args, mcp_env, mcp_transport, installed_at, status) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, 'active')",
    [skill_id, "mcp", name, description, command, args_json, env_json, transport, now_timestamp],
)?;
```

### 3.3 启动时恢复

```rust
// main.rs 启动时恢复已安装 MCP skills

async fn restore_installed_skills(state: &AppState) -> Result<()> {
    let rows = state.db.query(
        "SELECT id, mcp_command, mcp_args, mcp_env, mcp_transport FROM installed_skills WHERE skill_type='mcp' AND status='active'",
        [],
    )?;

    for row in rows {
        let mcp_host = &state.mcp_host;
        mcp_host.start_server(row.id, row.mcp_command, parse_args(row.mcp_args), parse_env(row.mcp_env), row.mcp_transport)?;
    }

    Ok(())
}

// main.rs 启动流程新增:
// 1. register_builtin_skills()   (已有)
// 2. restore_installed_skills()  (新增)
// 3. scheduler.start_loop()      (已有)
// 4. task_worker spawn           (已有)
```

### 3.4 卸载时删除

```rust
// skill.uninstall handler 改造
// 停止 MCP server (已有)
// 新增: DELETE FROM installed_skills WHERE id = ?
db.execute("DELETE FROM installed_skills WHERE id = ?", [skill_id])?;
```

---

## 4. CI 日常分支触发

### 4.1 ci.yml 改造

```yaml
# .github/workflows/ci.yml
name: CI

on:
  push:
    branches: ['*']         # 任意分支 push 都触发
    tags: ['v*']            # tag push 也触发
  pull_request:
    branches: [main]        # PR 到 main 触发

jobs:
  rust-check:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          submodules: recursive
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: clippy
      - uses: Swatinem/rust-cache@v2
      - run: cargo check
      - run: cargo test --lib
      - run: cargo clippy -- -D warnings

  frontend-check:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: pnpm/action-setup@v4
        with:
          version: 9
      - uses: actions/setup-node@v4
        with:
          node-version: 22
          cache: pnpm
      - run: pnpm install
      - run: pnpm run build:shared
      - run: pnpm run typecheck
      - run: pnpm run build:web

  docker-build:
    runs-on: ubuntu-latest
    needs: [rust-check, frontend-check]
    if: github.event_name == 'push' && startsWith(github.ref, 'refs/tags/v')
    steps:
      - uses: actions/checkout@v4
      - run: docker build -f infra/docker/Dockerfile -t mapleos-server .
      - run: docker run mapleos-server --help
```

变更要点:
- `on.push.branches`: `['*']` 替代原来的 `tags: v*` 限定
- `on.pull_request`: 新增 PR 触发
- `docker-build`: 仅在 tag push 时执行(日常 CI 不做 Docker 构建)

### 4.2 Branch Protection Rule

在 GitHub repo settings 中设置:
- main 分支: Require status checks to pass before merging
- Required checks: `rust-check`, `frontend-check`

---

## 5. memory_search 接口双模式

### 5.1 handler 改造

```rust
// main.rs memory_search_handler 改造

async fn memory_search_handler(req: Request) -> impl IntoResponse {
    // 支持 GET query 参数
    let query_params: HashMap<String, String> = req.uri().query()
        .and_then(|q| serde_urlencoded::from_str(q).ok())
        .unwrap_or_default();

    // 支持 POST body
    let body: Option<MemorySearchRequest> = req.body().json().await.ok();

    // 合并: POST body 优先, GET query 补充
    let keyword = body.and_then(|b| b.keyword)
        .or_else(|| query_params.get("query").cloned())
        .or_else(|| query_params.get("keyword").cloned())
        .ok_or("missing keyword or query")?;

    let limit = body.and_then(|b| b.limit)
        .or_else(|| query_params.get("limit").and_then(|v| v.parse::<u32>().ok()))
        .unwrap_or(10);

    let memory_type = body.and_then(|b| b.memory_type)
        .or_else(|| query_params.get("memory_type").cloned());

    // 查询逻辑 (已有)
    let results = search_memories(keyword, memory_type, limit)?;

    Ok(Json(results))
}
```

路由注册变更:
```rust
// 原来只有 POST
// .route("/api/memories/search", post(memory_search_handler))
// 改为 GET + POST 双注册
.route("/api/memories/search", get(memory_search_handler).post(memory_search_handler))
```

---

## 6. 文件变更清单

| # | 文件 | 操作 | 说明 |
|---|------|------|------|
| 3.1 | `migrations/004_installed_skills.sql` | 新增 | installed_skills 表 |
| 3.2 | `server/src/main.rs` code_execute | 改造 | Docker/WASM 双策略沙箱执行 |
| 3.3 | `server/src/main.rs` file_ops | 改造 | canonicalize 路径校验 |
| 3.4 | `server/src/main.rs` skill.install | 改造 | 安装时写入 DB |
| 3.5 | `server/src/main.rs` skill.uninstall | 改造 | 卸载时删除 DB 记录 |
| 3.6 | `server/src/main.rs` 启动流程 | 改造 | 新增 restore_installed_skills() |
| 3.7 | `server/src/main.rs` memory_search | 改造 | GET query + POST body 双模式 |
| 3.8 | `.github/workflows/ci.yml` | 改造 | 任意分支 push + PR 触发 |
| 3.9 | `Cargo.toml` | 改造 | 新增 wasmtime 依赖(可选) |

---

## 7. 风险与应对

| 风险 | 影响 | 应对策略 |
|------|------|---------|
| Docker runtime 不可用 | 沙箱隔离降级为 WASM | 自动检测 Docker, fallback 到 WASM |
| WASM Python 支持有限 | Python 代码执行受限 | WASM 模式仅支持 JavaScript, Python 仅在 Docker 模式可用; 向用户提示限制 |
| canonicalize 对不存在路径失败 | 新文件写入路径校验异常 | 先 canonicalize 父目录, 再拼接文件名 |
| MCP server 重启恢复失败 | skill 启动时进程启动失败 | 跳过失败的 skill, 标记 status='error', 不影响其他 skill |
| CI 触发范围过广 | 每次 push 都跑完整 CI | rust-check 和 frontend-check 分离, Docker 构建仅在 tag 时 |