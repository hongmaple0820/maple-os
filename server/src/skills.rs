use maple_engine::skill_registry::Skill;
use maple_engine::skill_registry::SkillRegistry;
use serde_json::Value;

/// #10: Strip HTML tags to extract text content from a page.
fn strip_html_tags(html: &str) -> String {
    let mut result = String::with_capacity(html.len() / 2);
    let mut in_tag = false;
    let mut in_script = false;

    let lower = html.to_lowercase();
    let chars: Vec<char> = html.chars().collect();
    let lower_chars: Vec<char> = lower.chars().collect();

    let mut i = 0;
    while i < chars.len() {
        if !in_tag && !in_script {
            // Check for <script or <style
            if lower_chars[i] == '<' {
                let remaining: String = lower_chars[i..].iter().take(8).collect();
                if remaining.starts_with("<script") || remaining.starts_with("<style") {
                    in_script = true;
                    i += 1;
                    continue;
                }
            }
        }

        if in_script {
            if lower_chars[i] == '<' {
                let remaining: String = lower_chars[i..].iter().take(9).collect();
                if remaining.starts_with("</script>") || remaining.starts_with("</style>") {
                    in_script = false;
                    // Skip the closing tag
                    while i < chars.len() && chars[i] != '>' {
                        i += 1;
                    }
                    i += 1;
                    continue;
                }
            }
            i += 1;
            continue;
        }

        if chars[i] == '<' {
            in_tag = true;
        } else if chars[i] == '>' {
            in_tag = false;
            // Add a space after tags to prevent word merging
            if !result.is_empty() && !result.ends_with(' ') {
                result.push(' ');
            }
        } else if !in_tag {
            // Decode common entities
            if chars[i] == '&' {
                let entity: String = chars[i..].iter().take(6).collect();
                if entity.starts_with("&amp;") {
                    result.push('&');
                    i += 5;
                    continue;
                } else if entity.starts_with("&lt;") {
                    result.push('<');
                    i += 4;
                    continue;
                } else if entity.starts_with("&gt;") {
                    result.push('>');
                    i += 4;
                    continue;
                } else if entity.starts_with("&quot;") {
                    result.push('"');
                    i += 6;
                    continue;
                } else if entity.starts_with("&#39;") || entity.starts_with("&apos;") {
                    result.push('\'');
                    i += if entity.starts_with("&#39;") { 5 } else { 6 };
                    continue;
                }
            }
            result.push(chars[i]);
        }
        i += 1;
    }

    // Collapse whitespace
    result.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// T5-4: Check whether a host should be blocked to prevent SSRF.
/// Returns Some(reason) if blocked, None if allowed.
///
/// Blocks:
/// - "localhost", "*.localhost"
/// - IPv6 ::1, fc00::/7, fe80::/10
///
/// Does NOT block public DNS names — those go through the allowlist
/// check separately.
fn check_private_host(host: &str) -> Option<String> {
    let lower = host.to_lowercase();
    if lower == "localhost" || lower.ends_with(".localhost") {
        return Some(format!("localhost is blocked (host={host})"));
    }

    // Try to parse as IPv4 / IPv6 — if it's an IP literal, apply range checks
    if let Ok(ip) = lower.parse::<std::net::IpAddr>() {
        match ip {
            std::net::IpAddr::V4(v4) => {
                if v4.is_loopback() {
                    return Some(format!("127.0.0.0/8 loopback blocked (host={host})"));
                }
                if v4.is_private() {
                    return Some(format!("RFC1918 private range blocked (host={host})"));
                }
                if v4.is_link_local() {
                    return Some(format!("169.254.0.0/16 link-local blocked (host={host})"));
                }
                if v4.is_unspecified() {
                    return Some(format!("0.0.0.0 unspecified blocked (host={host})"));
                }
            }
            std::net::IpAddr::V6(v6) => {
                if v6.is_loopback() {
                    return Some(format!("::1 loopback blocked (host={host})"));
                }
                // No stable is_private for IPv6 in std, but we can check
                // unique local addresses (fc00::/7) and link-local (fe80::/10)
                let segs = v6.segments();
                if (segs[0] & 0xfe00) == 0xfc00 {
                    return Some(format!("fc00::/7 ULA blocked (host={host})"));
                }
                if (segs[0] & 0xffc0) == 0xfe80 {
                    return Some(format!("fe80::/10 link-local blocked (host={host})"));
                }
            }
        }
    }

    None
}

struct ExpressionParser<'a> {
    chars: Vec<char>,
    pos: usize,
    source: &'a str,
}

impl<'a> ExpressionParser<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            chars: source.chars().collect(),
            pos: 0,
            source,
        }
    }

    fn parse(mut self) -> anyhow::Result<f64> {
        let value = self.parse_expression()?;
        self.skip_whitespace();

        if self.pos != self.chars.len() {
            anyhow::bail!(
                "unexpected character '{}' at position {}",
                self.chars[self.pos],
                self.pos
            );
        }

        if !value.is_finite() {
            anyhow::bail!("expression result is not finite");
        }

        Ok(value)
    }

    fn parse_expression(&mut self) -> anyhow::Result<f64> {
        let mut value = self.parse_term()?;

        loop {
            self.skip_whitespace();
            match self.peek() {
                Some('+') => {
                    self.advance();
                    value += self.parse_term()?;
                }
                Some('-') => {
                    self.advance();
                    value -= self.parse_term()?;
                }
                _ => return Ok(value),
            }
        }
    }

    fn parse_term(&mut self) -> anyhow::Result<f64> {
        let mut value = self.parse_factor()?;

        loop {
            self.skip_whitespace();
            match self.peek() {
                Some('*') => {
                    self.advance();
                    value *= self.parse_factor()?;
                }
                Some('/') => {
                    self.advance();
                    let divisor = self.parse_factor()?;
                    if divisor == 0.0 {
                        anyhow::bail!("division by zero");
                    }
                    value /= divisor;
                }
                _ => return Ok(value),
            }
        }
    }

    fn parse_factor(&mut self) -> anyhow::Result<f64> {
        self.skip_whitespace();

        match self.peek() {
            Some('+') => {
                self.advance();
                self.parse_factor()
            }
            Some('-') => {
                self.advance();
                Ok(-self.parse_factor()?)
            }
            Some('(') => {
                self.advance();
                let value = self.parse_expression()?;
                self.skip_whitespace();

                if self.peek() != Some(')') {
                    anyhow::bail!("missing closing ')' at position {}", self.pos);
                }

                self.advance();
                Ok(value)
            }
            Some(c) if c.is_ascii_digit() || c == '.' => self.parse_number(),
            Some(c) => anyhow::bail!("unexpected character '{}' at position {}", c, self.pos),
            None => anyhow::bail!("unexpected end of expression"),
        }
    }

    fn parse_number(&mut self) -> anyhow::Result<f64> {
        let start = self.pos;
        let mut seen_dot = false;
        let mut seen_digit = false;

        while let Some(c) = self.peek() {
            if c.is_ascii_digit() {
                seen_digit = true;
                self.advance();
            } else if c == '.' && !seen_dot {
                seen_dot = true;
                self.advance();
            } else {
                break;
            }
        }

        if !seen_digit {
            anyhow::bail!("expected number at position {}", start);
        }

        let number: String = self.chars[start..self.pos].iter().collect();
        number
            .parse::<f64>()
            .map_err(|e| anyhow::anyhow!("invalid number '{}' in '{}': {}", number, self.source, e))
    }

    fn skip_whitespace(&mut self) {
        while matches!(self.peek(), Some(c) if c.is_whitespace()) {
            self.advance();
        }
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn advance(&mut self) {
        self.pos += 1;
    }
}

fn evaluate_arithmetic_expression(expression: &str) -> anyhow::Result<f64> {
    if expression.trim().is_empty() {
        anyhow::bail!("expression is required");
    }

    ExpressionParser::new(expression).parse()
}

pub async fn register_builtin_skills(skill_registry: &SkillRegistry) {
    struct EchoSkill;
    impl Skill for EchoSkill {
        fn id(&self) -> &str {
            "echo"
        }
        fn description(&self) -> &str {
            "Echo back the input"
        }
        fn execute(&self, config: &Value) -> anyhow::Result<Value> {
            Ok(config.clone())
        }
    }

    struct WebSearchSkill;
    impl Skill for WebSearchSkill {
        fn id(&self) -> &str {
            "web_search"
        }
        fn description(&self) -> &str {
            "Search the web for information"
        }
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
                    api_key,
                    engine_id,
                    urlencoding::encode(query),
                    num_results
                );

                let client = reqwest::Client::new();
                match tokio::task::block_in_place(|| {
                    rt.block_on(async {
                        client
                            .get(&url)
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
                            let results: Vec<Value> = items
                                .iter()
                                .take(num_results)
                                .map(|item| {
                                    serde_json::json!({
                                        "title": item["title"].as_str().unwrap_or(""),
                                        "url": item["link"].as_str().unwrap_or(""),
                                        "snippet": item["snippet"].as_str().unwrap_or(""),
                                    })
                                })
                                .collect();

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
            let ddg_url = format!(
                "https://lite.duckduckgo.com/lite/?q={}",
                urlencoding::encode(query)
            );

            let client = reqwest::Client::new();
            let resp = tokio::task::block_in_place(|| {
                rt.block_on(async {
                    client
                        .get(&ddg_url)
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
                    let rest = &trimmed[href_start + 6..];
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
                        snippet = trimmed[start + 1..end].trim().to_string();
                    } else {
                        snippet = trimmed[start + 1..].trim().to_string();
                    }
                }
                in_link = false;
                if !title.is_empty() {
                    results.push(serde_json::json!({
                        "title": title,
                        "url": url,
                        "snippet": snippet,
                    }));
                    title.clear();
                    url.clear();
                    snippet.clear();
                }
                if results.len() >= max {
                    break;
                }
            }
        }
        results
    }

    struct CodeExecSkill;
    impl Skill for CodeExecSkill {
        fn id(&self) -> &str {
            "code_execute"
        }
        fn description(&self) -> &str {
            "Execute code in a sandboxed environment"
        }
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

    struct CalculatorSkill;
    impl Skill for CalculatorSkill {
        fn id(&self) -> &str {
            "calculator"
        }
        fn description(&self) -> &str {
            "Evaluate arithmetic expressions safely"
        }
        fn parameters_schema(&self) -> Option<Value> {
            Some(serde_json::json!({
                "type": "object",
                "required": ["expression"],
                "properties": {
                    "expression": {
                        "type": "string",
                        "description": "Arithmetic expression using numbers, +, -, *, /, and parentheses"
                    }
                }
            }))
        }
        fn execute(&self, config: &Value) -> anyhow::Result<Value> {
            let expression = config["expression"].as_str().unwrap_or("");

            match evaluate_arithmetic_expression(expression) {
                Ok(result) => Ok(serde_json::json!({
                    "expression": expression,
                    "result": result,
                })),
                Err(e) => Ok(serde_json::json!({
                    "expression": expression,
                    "error": e.to_string(),
                })),
            }
        }
    }

    struct FileOpsSkill;
    impl Skill for FileOpsSkill {
        fn id(&self) -> &str {
            "file_ops"
        }
        fn description(&self) -> &str {
            "Read, write, and list files within workspace"
        }
        fn execute(&self, config: &Value) -> anyhow::Result<Value> {
            let operation = config["operation"].as_str().unwrap_or("list");
            let path = config["path"].as_str().unwrap_or(".");
            let content = config["content"].as_str();
            let max_read_bytes = 65536;

            let workspace_dir = std::env::var("MAPLEOS_WORKSPACE_DIR").unwrap_or_else(|_| {
                std::env::current_dir()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string()
            });
            let requested_path = std::path::Path::new(path);

            // Use canonicalize-based validation to prevent symlink and path traversal attacks
            let safe_path = match crate::sandbox::validate_path(requested_path, &workspace_dir) {
                Ok(p) => p,
                Err(e) => {
                    return Ok(serde_json::json!({
                        "error": e.to_string(),
                        "path": path,
                        "workspace": workspace_dir,
                    }));
                }
            };

            match operation {
                "read" => match std::fs::read_to_string(&safe_path) {
                    Ok(data) => {
                        let size = data.len();
                        let truncated = if size > max_read_bytes {
                            // T5-3: safe truncation at UTF-8 char boundary
                            let mut end = max_read_bytes;
                            while end > 0 && !data.is_char_boundary(end) {
                                end -= 1;
                            }
                            data[..end].to_string() + "...[truncated]"
                        } else {
                            data
                        };
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
                },
                "write" => {
                    // T5-3: write operations require explicit approval via
                    // MAPLEOS_FILE_OPS_WRITE=enabled env var. This is a
                    // coarse-grained gate; per-file approval via the
                    // approval_requests table is T5-3.1.
                    let write_enabled = std::env::var("MAPLEOS_FILE_OPS_WRITE")
                        .map(|v| v == "enabled" || v == "1" || v == "true")
                        .unwrap_or(false);
                    if !write_enabled {
                        return Ok(serde_json::json!({
                            "operation": "write",
                            "path": path,
                            "error": "file_ops write requires MAPLEOS_FILE_OPS_WRITE=enabled",
                            "permission_level": "workspace_write",
                            "hint": "set MAPLEOS_FILE_OPS_WRITE=enabled to allow writes within MAPLEOS_WORKSPACE_DIR",
                        }));
                    }
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
                "list" => match std::fs::read_dir(&safe_path) {
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
                },
                "exists" => Ok(serde_json::json!({
                    "operation": "exists",
                    "path": path,
                    "exists": std::path::Path::new(path).exists(),
                })),
                _ => Ok(serde_json::json!({
                    "error": format!("Unknown operation: {}", operation),
                    "supported": ["read", "write", "list", "exists"],
                })),
            }
        }
    }

    struct HttpRequestSkill;
    impl Skill for HttpRequestSkill {
        fn id(&self) -> &str {
            "http_request"
        }
        fn description(&self) -> &str {
            "Make HTTP requests with timeout, size limits, and domain allowlist"
        }
        fn execute(&self, config: &Value) -> anyhow::Result<Value> {
            let url = config["url"].as_str().unwrap_or("");
            let method = config["method"].as_str().unwrap_or("GET");
            let headers = config["headers"].as_object();
            let body = config["body"].as_str();
            let max_response_bytes = 32768;
            let timeout_secs = config["timeout"].as_u64().unwrap_or(30).min(60);

            if url.is_empty() {
                return Ok(serde_json::json!({"error": "url is required"}));
            }

            // T5-4: parse URL and enforce domain allowlist
            let parsed = match reqwest::Url::parse(url) {
                Ok(u) => u,
                Err(e) => {
                    return Ok(serde_json::json!({
                        "url": url,
                        "error": format!("invalid URL: {e}"),
                    }));
                }
            };
            let host = parsed.host_str().unwrap_or("");
            if host.is_empty() {
                return Ok(serde_json::json!({
                    "url": url,
                    "error": "URL has no host",
                }));
            }

            // Allowlist: comma-separated env var HTTP_ALLOW_DOMAINS
            // (subdomain match: "example.com" allows "api.example.com")
            // Empty env = allow all (backward compat for dev mode).
            let allow_domains: Vec<String> = std::env::var("HTTP_ALLOW_DOMAINS")
                .unwrap_or_default()
                .split(',')
                .map(|s| s.trim().to_lowercase())
                .filter(|s| !s.is_empty())
                .collect();

            if !allow_domains.is_empty() {
                let host_lower = host.to_lowercase();
                let allowed = allow_domains
                    .iter()
                    .any(|d| host_lower == d.as_str() || host_lower.ends_with(&format!(".{d}")));
                if !allowed {
                    return Ok(serde_json::json!({
                        "url": url,
                        "error": format!("host '{host}' not in HTTP_ALLOW_DOMAINS allowlist"),
                        "allowlist": allow_domains,
                    }));
                }
            }

            // T5-4: block private / loopback addresses unless explicitly allowed
            // (prevents SSRF to internal services)
            let block_private = std::env::var("HTTP_BLOCK_PRIVATE")
                .map(|v| v != "false" && v != "0")
                .unwrap_or(true);
            if block_private {
                if let Some(block_reason) = check_private_host(host) {
                    return Ok(serde_json::json!({
                        "url": url,
                        "error": format!("blocked: {block_reason}"),
                        "hint": "set HTTP_BLOCK_PRIVATE=false to allow (SSRF risk)",
                    }));
                }
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
                    req.timeout(std::time::Duration::from_secs(timeout_secs))
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
                    let truncated = if body_size > max_response_bytes {
                        // T5-4: safe truncation — body_text is a String so
                        // indexing by byte position is safe only at char boundaries
                        let mut end = max_response_bytes;
                        while end > 0 && !body_text.is_char_boundary(end) {
                            end -= 1;
                        }
                        body_text[..end].to_string() + "...[truncated]"
                    } else {
                        body_text
                    };
                    Ok(serde_json::json!({
                        "url": url,
                        "method": method,
                        "status": status,
                        "body": truncated,
                        "body_size": body_size,
                        "host": host,
                    }))
                }
                Err(e) => Ok(serde_json::json!({
                    "url": url,
                    "method": method,
                    "error": e.to_string(),
                    "host": host,
                })),
            }
        }
    }

    // #10: Browser automation skill — uses headless browser via
    // puppeteer-core or playwright. Falls back to http_request-style
    // page fetch when no browser is available (disabled by default).
    struct BrowserSkill;
    impl Skill for BrowserSkill {
        fn id(&self) -> &str {
            "browser"
        }
        fn description(&self) -> &str {
            "Browser automation: navigate, click, extract text, take screenshots"
        }
        fn parameters_schema(&self) -> Option<Value> {
            Some(serde_json::json!({
                "type": "object",
                "required": ["action"],
                "properties": {
                    "action": {
                        "type": "string",
                        "description": "navigate | click | extract | screenshot | scroll | wait"
                    },
                    "url": {"type": "string", "description": "URL to navigate to (for 'navigate' action)"},
                    "selector": {"type": "string", "description": "CSS selector (for 'click', 'extract', 'screenshot')"},
                    "wait_ms": {"type": "number", "description": "Wait time in ms (for 'wait' action, default 1000)"}
                }
            }))
        }
        fn execute(&self, config: &Value) -> anyhow::Result<Value> {
            let action = config["action"].as_str().unwrap_or("");
            let url = config["url"].as_str().unwrap_or("");
            let selector = config["selector"].as_str().unwrap_or("");
            let wait_ms = config["wait_ms"].as_u64().unwrap_or(1000);

            if action.is_empty() {
                return Ok(serde_json::json!({"error": "action is required"}));
            }

            // Check if browser automation is enabled
            let browser_enabled = std::env::var("MAPLEOS_BROWSER_ENABLED")
                .map(|v| v == "true" || v == "1")
                .unwrap_or(false);

            if !browser_enabled {
                // Fallback: for 'navigate' + 'extract', use http_request to fetch page
                if action == "navigate" && !url.is_empty() {
                    let rt = tokio::runtime::Handle::current();
                    let _guard = rt.enter();
                    let client = reqwest::Client::builder()
                        .timeout(std::time::Duration::from_secs(15))
                        .build()?;
                    let resp = tokio::task::block_in_place(|| {
                        rt.block_on(async {
                            client
                                .get(url)
                                .header("User-Agent", "Mozilla/5.0 (compatible; MapleOS/1.0)")
                                .send()
                                .await
                        })
                    });
                    return match resp {
                        Ok(r) => {
                            let status = r.status().as_u16();
                            let html = tokio::task::block_in_place(|| {
                                rt.block_on(async { r.text().await.unwrap_or_default() })
                            });
                            // Extract text content (strip HTML tags)
                            let text = strip_html_tags(&html);
                            let snippet = if text.len() > 2000 {
                                let mut end = 2000;
                                while end > 0 && !text.is_char_boundary(end) {
                                    end -= 1;
                                }
                                text[..end].to_string() + "...[truncated]"
                            } else {
                                text
                            };
                            Ok(serde_json::json!({
                                "action": "navigate",
                                "url": url,
                                "status": status,
                                "text": snippet,
                                "html_length": html.len(),
                                "mode": "http_fallback",
                                "hint": "Set MAPLEOS_BROWSER_ENABLED=true to enable full browser automation"
                            }))
                        }
                        Err(e) => Ok(serde_json::json!({
                            "action": "navigate",
                            "url": url,
                            "error": e.to_string(),
                            "mode": "http_fallback",
                        })),
                    };
                }

                return Ok(serde_json::json!({
                    "action": action,
                    "error": "Browser automation is not enabled. Set MAPLEOS_BROWSER_ENABLED=true to use navigate/click/extract/screenshot.",
                    "mode": "disabled",
                    "url": url,
                    "selector": selector,
                }));
            }

            // Browser is enabled — delegate to puppeteer/playwright subprocess
            // This requires a Node.js runtime with puppeteer-core installed.
            // The implementation calls `node scripts/browser/automation.mjs`
            // with the action config as a JSON argument.
            let rt = tokio::runtime::Handle::current();
            let _guard = rt.enter();

            let script_dir = std::env::var("MAPLEOS_BROWSER_SCRIPT_DIR")
                .unwrap_or_else(|_| "scripts/browser".to_string());
            let script_path = format!("{}/automation.mjs", script_dir);

            let result = tokio::task::block_in_place(|| {
                rt.block_on(async {
                    tokio::process::Command::new("node")
                        .arg(&script_path)
                        .arg("--action")
                        .arg(action)
                        .arg("--url")
                        .arg(url)
                        .arg("--selector")
                        .arg(selector)
                        .arg("--wait-ms")
                        .arg(wait_ms.to_string())
                        .stdout(std::process::Stdio::piped())
                        .stderr(std::process::Stdio::piped())
                        .output()
                        .await
                })
            });

            match result {
                Ok(output) => {
                    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                    let exit_code = output.status.code().unwrap_or(-1);

                    if exit_code == 0 {
                        // Try to parse stdout as JSON
                        if let Ok(json) = serde_json::from_str::<Value>(&stdout) {
                            Ok(json)
                        } else {
                            Ok(serde_json::json!({
                                "action": action,
                                "output": stdout,
                                "mode": "browser",
                            }))
                        }
                    } else {
                        Ok(serde_json::json!({
                            "action": action,
                            "error": stderr,
                            "exit_code": exit_code,
                            "mode": "browser",
                        }))
                    }
                }
                Err(e) => Ok(serde_json::json!({
                    "action": action,
                    "error": format!("Failed to launch browser script: {}. Make sure Node.js is installed and {} exists.", e, script_path),
                    "mode": "browser",
                })),
            }
        }
    }

    skill_registry.register(Box::new(EchoSkill)).await;
    skill_registry.register(Box::new(WebSearchSkill)).await;
    skill_registry.register(Box::new(CodeExecSkill)).await;
    skill_registry.register(Box::new(CalculatorSkill)).await;
    skill_registry.register(Box::new(FileOpsSkill)).await;
    skill_registry.register(Box::new(HttpRequestSkill)).await;
    skill_registry.register(Box::new(BrowserSkill)).await;

    tracing::info!(
        "Built-in skills registered: echo, web_search, code_execute, calculator, file_ops, http_request, browser"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_check_private_host_blocks_localhost() {
        assert!(check_private_host("localhost").is_some());
        assert!(check_private_host("foo.localhost").is_some());
        assert!(check_private_host("LOCALHOST").is_some()); // case-insensitive
    }

    #[test]
    fn test_check_private_host_blocks_ipv4_loopback() {
        assert!(check_private_host("127.0.0.1").is_some());
        assert!(check_private_host("127.255.255.255").is_some());
    }

    #[test]
    fn test_check_private_host_blocks_rfc1918() {
        assert!(check_private_host("10.0.0.1").is_some());
        assert!(check_private_host("172.16.0.1").is_some());
        assert!(check_private_host("192.168.1.1").is_some());
    }

    #[test]
    fn test_check_private_host_blocks_link_local() {
        assert!(check_private_host("169.254.1.1").is_some());
    }

    #[test]
    fn test_check_private_host_blocks_unspecified() {
        assert!(check_private_host("0.0.0.0").is_some());
    }

    #[test]
    fn test_check_private_host_blocks_ipv6_loopback() {
        assert!(check_private_host("::1").is_some());
    }

    #[test]
    fn test_check_private_host_allows_public_dns() {
        assert!(check_private_host("example.com").is_none());
        assert!(check_private_host("api.openai.com").is_none());
    }

    #[test]
    fn test_check_private_host_allows_public_ipv4() {
        assert!(check_private_host("8.8.8.8").is_none());
        assert!(check_private_host("1.1.1.1").is_none());
    }

    #[test]
    fn test_calculator_evaluates_basic_arithmetic() {
        assert_eq!(evaluate_arithmetic_expression("1 + 2 * 3").unwrap(), 7.0);
        assert_eq!(evaluate_arithmetic_expression("(1 + 2) * 3").unwrap(), 9.0);
    }

    #[test]
    fn test_calculator_handles_decimals_and_unary_signs() {
        assert_eq!(
            evaluate_arithmetic_expression("-1.5 + +2.25").unwrap(),
            0.75
        );
        assert_eq!(
            evaluate_arithmetic_expression("-(2 + 3) * 4").unwrap(),
            -20.0
        );
    }

    #[test]
    fn test_calculator_rejects_invalid_expressions() {
        assert!(evaluate_arithmetic_expression("").is_err());
        assert!(evaluate_arithmetic_expression("1 +").is_err());
        assert!(evaluate_arithmetic_expression("2 / 0").is_err());
        assert!(evaluate_arithmetic_expression("2 ** 3").is_err());
        assert!(evaluate_arithmetic_expression("alert(1)").is_err());
    }

    #[tokio::test]
    async fn test_register_builtin_skills_includes_calculator_schema() {
        let registry = SkillRegistry::new();
        register_builtin_skills(&registry).await;

        let skills = registry.list_with_schemas().await;
        let calculator = skills
            .iter()
            .find(|(id, _, _, _)| id == "calculator")
            .expect("calculator skill should be registered");

        assert!(calculator.2.is_some());
        let result = registry
            .execute(
                "calculator",
                &serde_json::json!({"expression": "10 / (2 + 3)"}),
            )
            .await
            .unwrap();
        assert_eq!(result["result"].as_f64().unwrap(), 2.0);
    }

    // ── #10: strip_html_tags tests ──

    #[test]
    fn test_strip_html_simple() {
        let html = "<p>Hello World</p>";
        assert_eq!(strip_html_tags(html), "Hello World");
    }

    #[test]
    fn test_strip_html_with_tags() {
        let html = "<div><h1>Title</h1><p>Body text</p></div>";
        assert_eq!(strip_html_tags(html), "Title Body text");
    }

    #[test]
    fn test_strip_html_removes_scripts() {
        let html = "<p>Before</p><script>alert('xss')</script><p>After</p>";
        assert_eq!(strip_html_tags(html), "Before After");
    }

    #[test]
    fn test_strip_html_removes_styles() {
        let html = "<style>body { color: red; }</style><p>Content</p>";
        assert_eq!(strip_html_tags(html), "Content");
    }

    #[test]
    fn test_strip_html_decodes_entities() {
        let html = "<p>Tom &amp; Jerry &lt;3</p>";
        assert_eq!(strip_html_tags(html), "Tom & Jerry <3");
    }

    #[test]
    fn test_strip_html_collapses_whitespace() {
        let html = "<p>  Multiple    spaces  </p>";
        assert_eq!(strip_html_tags(html), "Multiple spaces");
    }

    #[test]
    fn test_strip_html_empty() {
        assert_eq!(strip_html_tags(""), "");
        assert_eq!(strip_html_tags("<div></div>"), "");
    }

    #[test]
    fn test_strip_html_unicode() {
        let html = "<p>你好世界 🌍</p>";
        assert_eq!(strip_html_tags(html), "你好世界 🌍");
    }
}
