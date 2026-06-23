# #[tool] 派生宏

> MapleOS 提供的 Rust 过程宏，用于声明式定义工具（Tool）。

## 概述

`#[tool]` 宏自动从 Rust 函数生成 `ToolDefinition`（含 JSON Schema）和执行器包装函数，减少手写样板代码。

## 基本用法

```rust
use maple_macro::tool;
use anyhow::Result;
use serde_json::Value;

#[tool(description = "读取文件内容")]
async fn read_file(path: String) -> Result<Value> {
    let content = tokio::fs::read_to_string(&path).await?;
    Ok(serde_json::json!({ "content": content }))
}
```

宏会自动生成两个函数：

```rust
// 工具定义（含 JSON Schema）
pub fn read_file_definition() -> ToolDefinition { ... }

// 执行器包装
pub async fn read_file_execute(args: &Value) -> Result<Value> { ... }
```

## 参数类型映射

| Rust 类型 | JSON Schema |
|-----------|-------------|
| `String`, `&str` | `{ "type": "string" }` |
| `i8` ~ `u64`, `usize` | `{ "type": "integer" }` |
| `f32`, `f64` | `{ "type": "number" }` |
| `bool` | `{ "type": "boolean" }` |
| `Vec<T>` | `{ "type": "array", "items": T_schema }` |
| `Option<T>` | 同 T，但不在 `required` 列表中 |
| `serde_json::Value` | `{}` (任意类型) |

## 可选参数

`Option<T>` 类型的参数会自动标记为可选：

```rust
#[tool(description = "搜索文件")]
async fn search_files(
    query: String,              // required
    limit: Option<usize>,       // optional
    case_sensitive: Option<bool>, // optional
) -> Result<Value> {
    // ...
}
```

## 自定义工具名

默认使用函数名作为工具名。可通过 `name` 参数覆盖：

```rust
#[tool(description = "读取文件", name = "file_read")]
async fn read_file(path: String) -> Result<Value> {
    // ...
}
// 生成: file_read_definition(), file_read_execute()
```

## 与 ToolRegistry 集成

```rust
use maple_agent::ToolRegistry;

let mut registry = ToolRegistry::new();

// 注册工具
registry.register(
    read_file_definition(),
    Box::new(|args| Box::pin(read_file_execute(args))),
);

// 语义搜索选择工具
let tools = registry.search_tools("读取配置文件", 3);
```

## 生成的 JSON Schema 示例

对于以下函数：

```rust
#[tool(description = "创建文件")]
async fn create_file(path: String, content: String, overwrite: Option<bool>) -> Result<Value> {
    // ...
}
```

生成的 Schema：

```json
{
  "type": "object",
  "properties": {
    "path": { "type": "string" },
    "content": { "type": "string" },
    "overwrite": { "type": "boolean" }
  },
  "required": ["path", "content"]
}
```

## 注意事项

1. 函数必须是 `async fn`
2. 返回类型必须是 `Result<Value>`（`anyhow::Result` 或兼容类型）
3. 参数类型必须实现 `serde::Deserialize`
4. 宏不会修改原始函数，只是额外生成两个辅助函数
