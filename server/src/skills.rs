use serde_json::Value;
use maple_engine::skill_registry::SkillRegistry;
use maple_engine::skill_registry::Skill;

pub async fn register_builtin_skills(skill_registry: &SkillRegistry) {
    struct EchoSkill;
    impl Skill for EchoSkill {
        fn id(&self) -> &str { "echo" }
        fn description(&self) -> &str { "Echo back the input" }
        fn execute(&self, config: &Value) -> anyhow::Result<Value> {
            Ok(config.clone())
        }
    }

    struct WebSearchSkill;
    impl Skill for WebSearchSkill {
        fn id(&self) -> &str { "web_search" }
        fn description(&self) -> &str { "Search the web for information" }
        fn execute(&self, config: &Value) -> anyhow::Result<Value> {
            let query = config["query"].as_str().unwrap_or("");
            let num_results = config["num_results"].as_u64().unwrap_or(5) as usize;

            if query.is_empty() {
                return Ok(serde_json::json!({"error": "query is required"}));
            }

            let search_api_key = std::env::var("SEARCH_API_KEY").ok();
            let search_engine_id = std::env::var("SEARCH_ENGINE_ID").ok();

            if let (Some(api_key), Some(engine_id)) = (search_api_key, search_engine_id) {
                let rt = tokio::runtime::Handle::current();
                let _guard = rt.enter();

                let url = format!(
                    "https://www.googleapis.com/customsearch/v1?key={}&cx={}&q={}&num={}",
                    api_key, engine_id, urlencoding::encode(query), num_results
                );

                let client = reqwest::Client::new();
                match tokio::task::block_in_place(|| {
                    rt.block_on(async {
                        client.get(&url)
                            .timeout(std::time::Duration::from_secs(10))
                            .send()
                            .await
                    })
                }) {
                    Ok(resp) => {
                        let body = tokio::task::block_in_place(|| {
                            rt.block_on(async { resp.text().await.unwrap_or_default() })
                        });

                        if let Ok(json) = serde_json::from_str::<Value>(&body) {
                            let empty: Vec<Value> = vec![];
                            let items = json["items"].as_array().unwrap_or(&empty);
                            let results: Vec<Value> = items.iter().take(num_results).map(|item| {
                                serde_json::json!({
                                    "title": item["title"].as_str().unwrap_or(""),
                                    "url": item["link"].as_str().unwrap_or(""),
                                    "snippet": item["snippet"].as_str().unwrap_or(""),
                                })
                            }).collect();

                            return Ok(serde_json::json!({
                                "query": query,
                                "results": results,
                                "source": "google_custom_search",
                            }));
                        }
                    }
                    Err(e) => {
                        tracing::warn!("Web search API error: {}", e);
                    }
                }
            }

            let rt = tokio::runtime::Handle::current();
            let _guard = rt.enter();
            let ddg_url = format!("https://lite.duckduckgo.com/lite/?q={}", urlencoding::encode(query));

            let client = reqwest::Client::new();
            let resp = tokio::task::block_in_place(|| {
                rt.block_on(async {
                    client.get(&ddg_url)
                        .header("User-Agent", "Mozilla/5.0 (compatible; MapleOS/1.0)")
                        .timeout(std::time::Duration::from_secs(10))
                        .send()
                        .await
                })
            });

            match resp {
                Ok(response) => {
                    let html = tokio::task::block_in_place(|| {
                        rt.block_on(async { response.text().await.unwrap_or_default() })
                    });
                    let results = parse_ddg_lite(&html, num_results);
                    Ok(serde_json::json!({
                        "query": query,
                        "results": results,
                        "source": "duckduckgo_lite",
                    }))
                }
                Err(e) => {
                    tracing::warn!("DuckDuckGo search error: {}", e);
                    Ok(serde_json::json!({
                        "query": query,
                        "results": [],
                        "source": "none",
                        "message": format!("Search unavailable: {}", e),
                    }))
                }
            }
        }
    }

    fn parse_ddg_lite(html: &str, max: usize) -> Vec<Value> {
        let mut results: Vec<Value> = Vec::new();
        let mut title = String::new();
        let mut url = String::new();
        let mut snippet = String::new();
        let mut in_link = false;

        for line in html.lines() {
            let trimmed = line.trim();
            if trimmed.contains("class=\"result__a\"") {
                in_link = true;
                if let Some(start) = trimmed.find(">")
                    && let Some(end) = trimmed.find("</a>")
                {
                    title = trimmed[start + 1..end].trim().to_string();
                }
                if let Some(href_start) = trimmed.find("href=\"") {
                    let rest = &trimmed[href_start+6..];
                    if let Some(href_end) = rest.find("\"") {
                        url = rest[..href_end].to_string();
                        if url.starts_with("//") {
                            url = format!("https:{}", url);
                        }
                    }
                }
            } else if in_link && trimmed.contains("class=\"result__snippet\"") {
                if let Some(start) = trimmed.find(">") {
                    if let Some(end) = trimmed.find("</td>") {
                        snippet = trimmed[start+1..end].trim().to_string();
                    } else {
                        snippet = trimmed[start+1..].trim().to_string();
                    }
                }
                in_link = false;
                if !title.is_empty() {
                    results.push(serde_json::json!({
                        "title": title,
                        "url": url,
                        "snippet": snippet,
                    }));
                    title.clear(); url.clear(); snippet.clear();
                }
                if results.len() >= max { break; }
            }
        }
        results
    }

    struct CodeExecSkill;
    impl Skill for CodeExecSkill {
        fn id(&self) -> &str { "code_execute" }
        fn description(&self) -> &str { "Execute code in a sandboxed environment" }
        fn execute(&self, config: &Value) -> anyhow::Result<Value> {
            let language = config["language"].as_str().unwrap_or("unknown");
            let code = config["code"].as_str().unwrap_or("");
            let timeout_secs = config["timeout"].as_u64().unwrap_or(10).min(30);

            if code.is_empty() {
                return Ok(serde_json::json!({"error": "code is required"}));
            }

            // Use sandboxed execution: temp dir, env_clear, timeout
            let rt = tokio::runtime::Handle::current();
            let _guard = rt.enter();
            tokio::task::block_in_place(|| {
                rt.block_on(async {
                    let sandbox = crate::sandbox::CodeSandbox::new(language, code, timeout_secs);
                    let result = sandbox.execute().await?;
                    Ok(serde_json::json!({
                        "language": language,
                        "stdout": result.stdout,
                        "stderr": result.stderr,
                        "exit_code": result.exit_code,
                        "timed_out": result.timed_out,
                    }))
                })
            })
        }
    }

    struct FileOpsSkill;
    impl Skill for FileOpsSkill {
        fn id(&self) -> &str { "file_ops" }
        fn description(&self) -> &str { "Read, write, and list files within workspace" }
        fn execute(&self, config: &Value) -> anyhow::Result<Value> {
            let operation = config["operation"].as_str().unwrap_or("list");
            let path = config["path"].as_str().unwrap_or(".");
            let content = config["content"].as_str();
            let max_read_bytes = 65536;

            let workspace_dir = std::env::var("MAPLEOS_WORKSPACE_DIR")
                .unwrap_or_else(|_| std::env::current_dir().unwrap_or_default().to_string_lossy().to_string());
            let requested_path = std::path::Path::new(path);

            // Use canonicalize-based validation to prevent symlink and path traversal attacks
            let safe_path = match crate::sandbox::validate_path(requested_path, &workspace_dir) {
                Ok(p) => p,
                Err(e) => return Ok(serde_json::json!({
                    "error": e.to_string(),
                    "path": path,
                    "workspace": workspace_dir,
                })),
            };

            match operation {
                "read" => {
                    match std::fs::read_to_string(&safe_path) {
                        Ok(data) => {
                            let size = data.len();
                            let truncated = if size > max_read_bytes { data[..max_read_bytes].to_string() + "...[truncated]" } else { data };
                            Ok(serde_json::json!({
                                "operation": "read",
                                "path": path,
                                "content": truncated,
                                "size": size,
                            }))
                        }
                        Err(e) => Ok(serde_json::json!({
                            "operation": "read",
                            "path": path,
                            "error": e.to_string(),
                        })),
                    }
                }
                "write" => {
                    let data = content.unwrap_or("");
                    match std::fs::write(&safe_path, data) {
                        Ok(_) => Ok(serde_json::json!({
                            "operation": "write",
                            "path": path,
                            "bytes_written": data.len(),
                            "status": "success",
                        })),
                        Err(e) => Ok(serde_json::json!({
                            "operation": "write",
                            "path": path,
                            "error": e.to_string(),
                        })),
                    }
                }
                "list" => {
                    match std::fs::read_dir(&safe_path) {
                        Ok(entries) => {
                            let files: Vec<serde_json::Value> = entries
                                .filter_map(|e| e.ok())
                                .map(|e| {
                                    let metadata = e.metadata().ok();
                                    serde_json::json!({
                                        "name": e.file_name().to_string_lossy(),
                                        "path": e.path().to_string_lossy(),
                                        "is_dir": metadata.as_ref().map(|m| m.is_dir()).unwrap_or(false),
                                        "size": metadata.as_ref().map(|m| m.len()).unwrap_or(0),
                                    })
                                })
                                .collect();
                            Ok(serde_json::json!({
                                "operation": "list",
                                "path": path,
                                "entries": files,
                                "count": files.len(),
                            }))
                        }
                        Err(e) => Ok(serde_json::json!({
                            "operation": "list",
                            "path": path,
                            "error": e.to_string(),
                        })),
                    }
                }
                "exists" => {
                    Ok(serde_json::json!({
                        "operation": "exists",
                        "path": path,
                        "exists": std::path::Path::new(path).exists(),
                    }))
                }
                _ => Ok(serde_json::json!({
                    "error": format!("Unknown operation: {}", operation),
                    "supported": ["read", "write", "list", "exists"],
                })),
            }
        }
    }

    struct HttpRequestSkill;
    impl Skill for HttpRequestSkill {
        fn id(&self) -> &str { "http_request" }
        fn description(&self) -> &str { "Make HTTP requests with timeout and size limits" }
        fn execute(&self, config: &Value) -> anyhow::Result<Value> {
            let url = config["url"].as_str().unwrap_or("");
            let method = config["method"].as_str().unwrap_or("GET");
            let headers = config["headers"].as_object();
            let body = config["body"].as_str();
            let max_response_bytes = 32768;

            if url.is_empty() {
                return Ok(serde_json::json!({"error": "url is required"}));
            }

            let rt = tokio::runtime::Handle::current();
            let _guard = rt.enter();

            let client = reqwest::Client::new();
            let mut req = match method.to_uppercase().as_str() {
                "POST" => client.post(url),
                "PUT" => client.put(url),
                "DELETE" => client.delete(url),
                "PATCH" => client.patch(url),
                _ => client.get(url),
            };

            if let Some(hdrs) = headers {
                for (k, v) in hdrs {
                    if let Some(val) = v.as_str() {
                        req = req.header(k.as_str(), val);
                    }
                }
            }

            if let Some(b) = body {
                req = req.body(b.to_string());
            }

            match tokio::task::block_in_place(|| {
                rt.block_on(async {
                    req.timeout(std::time::Duration::from_secs(30))
                        .send()
                        .await
                })
            }) {
                Ok(resp) => {
                    let status = resp.status().as_u16();
                    let body_text = tokio::task::block_in_place(|| {
                        rt.block_on(async { resp.text().await.unwrap_or_default() })
                    });
                    let body_size = body_text.len();
                    let truncated = if body_size > max_response_bytes { body_text[..max_response_bytes].to_string() + "...[truncated]" } else { body_text };
                    Ok(serde_json::json!({
                        "url": url,
                        "method": method,
                        "status": status,
                        "body": truncated,
                        "body_size": body_size,
                    }))
                }
                Err(e) => Ok(serde_json::json!({
                    "url": url,
                    "method": method,
                    "error": e.to_string(),
                })),
            }
        }
    }

    skill_registry.register(Box::new(EchoSkill)).await;
    skill_registry.register(Box::new(WebSearchSkill)).await;
    skill_registry.register(Box::new(CodeExecSkill)).await;
    skill_registry.register(Box::new(FileOpsSkill)).await;
    skill_registry.register(Box::new(HttpRequestSkill)).await;

    tracing::info!("Built-in skills registered: echo, web_search, code_execute, file_ops, http_request");
}
