//! Integration tests for maple-agent
//!
//! Tests the interaction between different components

use maple_agent::*;
use maple_llm::request::Message;
use serde_json::json;
use std::path::PathBuf;
use std::time::Duration;

#[test]
fn test_context_compressor_integration() {
    let config = ContextCompressorConfig {
        max_context_length: 20,
        threshold_percentage: 0.50,
        tail_percentage: 0.20,
        min_tail_tokens: 5,
        head_message_count: 1,
        ..Default::default()
    };
    let mut compressor = ContextCompressor::new(config);

    let messages = vec![
        Message::system("You are a helpful assistant."),
        Message::user("Hello, how are you today?"),
        Message::assistant("I'm doing well, thank you for asking!"),
        Message::user("What is Rust programming language?"),
        Message::assistant("Rust is a systems programming language focused on safety."),
        Message::user("Tell me more about ownership and borrowing."),
        Message::assistant("Ownership is Rust's key feature for memory safety without garbage collection."),
        Message::user("How does the borrow checker work?"),
        Message::assistant("The borrow checker enforces rules at compile time to prevent data races."),
    ];

    let compressed = compressor.compress(&messages);
    assert!(compressed.len() < messages.len());
    assert_eq!(compressed[0].role, "system");
}

#[test]
fn test_tool_use_context_permissions() {
    let ctx = ToolUseContext::builder("test", PathBuf::from("/workspace"))
        .permission_level(PermissionLevel::ReadOnly)
        .build();

    assert!(ctx.is_tool_allowed("read_file"));
    assert!(!ctx.is_tool_allowed("write_file"));
    assert!(!ctx.is_tool_allowed("delete_file"));
}

#[test]
fn test_tool_use_context_feature_flags() {
    let ctx = ToolUseContext::new("test", PathBuf::from("/workspace"));

    assert!(ctx.is_feature_enabled("network"));
    assert!(ctx.is_feature_enabled("file_system"));
    assert!(!ctx.is_feature_enabled("shell"));
    assert!(!ctx.is_feature_enabled("browser"));
}

#[test]
fn test_streaming_executor_classification() {
    let metadata = ToolMetadata {
        name: "read_file".to_string(),
        concurrency: ToolConcurrency::ConcurrentSafe,
        max_concurrent: None,
    };

    assert_eq!(metadata.concurrency, ToolConcurrency::ConcurrentSafe);
}

#[test]
fn test_trigger_system() {
    let mut scheduler = TriggerScheduler::new();

    let rule = trigger::TriggerRuleBuilder::new("test", "Test", "file_changed")
        .action(trigger::TriggerAction::ExecuteTool {
            tool_name: "lint".to_string(),
            input: json!({}),
        })
        .build();

    scheduler.register_rule(rule);

    let event = TriggerEvent::FileChanged {
        path: "src/main.rs".to_string(),
    };

    let triggered = scheduler.process_event(&event);
    assert_eq!(triggered.len(), 1);
}

#[test]
fn test_cron_expression() {
    let expr = CronExpression::parse("*/5 * * * *").unwrap();
    assert!(expr.matches(0, 12, 1, 1, 0));
    assert!(expr.matches(5, 12, 1, 1, 0));
    assert!(!expr.matches(7, 12, 1, 1, 0));
}

#[test]
fn test_security_levels() {
    assert!(SecurityLevel::ReadOnly < SecurityLevel::WorkspaceWrite);
    assert!(SecurityLevel::WorkspaceWrite < SecurityLevel::Prompt);
    assert!(SecurityLevel::Prompt < SecurityLevel::Allow);
    assert!(SecurityLevel::Allow < SecurityLevel::DangerFullAccess);
}

#[test]
fn test_security_policy() {
    let policy = security::SecurityPolicyBuilder::new(SecurityLevel::WorkspaceWrite)
        .denied_tools(vec!["rm".to_string()])
        .build();

    let manager = SecurityManager::new(policy);

    let result = manager.check_permission("read_file", &json!({}));
    assert!(matches!(result, Ok(PermissionCheck::Allowed)));

    let result = manager.check_permission("rm_rf", &json!({}));
    assert!(matches!(result, Ok(PermissionCheck::Denied { .. })));
}

#[test]
fn test_bash_command_classification() {
    assert_eq!(
        SecurityManager::classify_bash_permission("rm -rf /"),
        SecurityLevel::DangerFullAccess
    );
    assert_eq!(
        SecurityManager::classify_bash_permission("ls -la"),
        SecurityLevel::ReadOnly
    );
    assert_eq!(
        SecurityManager::classify_bash_permission("mv file1 file2"),
        SecurityLevel::WorkspaceWrite
    );
}

#[test]
fn test_lru_cache() {
    let mut cache = LruCache::new(2);
    cache.insert("a", 1, Duration::from_secs(60));
    cache.insert("b", 2, Duration::from_secs(60));

    assert_eq!(cache.get(&"a"), Some(&1));
    assert_eq!(cache.get(&"b"), Some(&2));

    cache.insert("c", 3, Duration::from_secs(60));
    assert_eq!(cache.get(&"a"), None);
    assert_eq!(cache.get(&"c"), Some(&3));
}

#[test]
fn test_tool_result_cache() {
    let mut cache = ToolResultCache::new(100);
    let input = json!({"path": "test.txt"});

    cache.insert(
        "read_file",
        &input,
        json!({"content": "hello"}),
        true,
        Duration::from_secs(60),
    );

    let result = cache.get("read_file", &input);
    assert!(result.is_some());
    let (output, success) = result.unwrap();
    assert!(success);
    assert_eq!(output["content"], "hello");
}

#[test]
fn test_recovery_scenarios() {
    assert_eq!(FailureScenario::TrustPromptUnresolved, FailureScenario::TrustPromptUnresolved);
    assert_ne!(FailureScenario::ProviderFailure, FailureScenario::ToolExecutionFailure);
}

#[test]
fn test_delegate_opts_builder() {
    let opts = DelegateOptsBuilder::new()
        .max_iterations(20)
        .timeout(Duration::from_secs(600))
        .build();

    assert_eq!(opts.max_iterations, 20);
    assert_eq!(opts.timeout, Duration::from_secs(600));
}

#[test]
fn test_task_complexity() {
    assert_eq!(TaskComplexity::Low, TaskComplexity::Low);
    assert_ne!(TaskComplexity::Low, TaskComplexity::High);
}

#[test]
fn test_coordinator_phases() {
    assert_eq!(coordinator::CoordinatorPhase::Analyze, coordinator::CoordinatorPhase::Analyze);
    assert_ne!(coordinator::CoordinatorPhase::Analyze, coordinator::CoordinatorPhase::Synthesize);
}

#[tokio::test]
async fn test_health_monitor() {
    let monitor = HealthMonitor::new();

    monitor.record_provider_request(
        "openai",
        true,
        Duration::from_millis(100),
        None,
    ).await;

    let health = monitor.get_provider_health("openai").await.unwrap();
    assert!(health.healthy);
    assert_eq!(health.successful_requests, 1);
}

#[tokio::test]
async fn test_performance_monitor() {
    let monitor = PerformanceMonitor::new();

    monitor.record_tool_call(Duration::from_millis(100)).await;
    monitor.record_cache_hit().await;

    let metrics = monitor.get_metrics().await;
    assert_eq!(metrics.tool_calls, 1);
    assert_eq!(metrics.cache_hits, 1);
}

#[tokio::test]
async fn test_concurrency_limiter() {
    let limiter = ConcurrencyLimiter::new(2);

    let _guard1 = limiter.acquire().await.unwrap();
    let _guard2 = limiter.acquire().await.unwrap();

    assert!(limiter.acquire().await.is_err());
    assert_eq!(limiter.available().await, 0);
}

// ============================================================
// v2.0.0 Feature Integration Tests
// ============================================================

/// RAG-Retrievable Tools: ToolRegistry keyword search
#[tokio::test]
async fn test_tool_registry_keyword_search() {
    use maple_agent::ToolRegistry;
    use maple_llm::embedding::FallbackEmbedder;
    use std::sync::Arc;

    let embedder = Arc::new(FallbackEmbedder::new(768));
    let registry = ToolRegistry::new(embedder).with_default_top_k(10);

    // Register tools
    let tool1 = maple_llm::request::ToolDefinition {
        name: "read_file".to_string(),
        description: "Read a file from disk".to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "path": {"type": "string", "description": "File path"}
            }
        }),
    };
    let tool2 = maple_llm::request::ToolDefinition {
        name: "write_file".to_string(),
        description: "Write content to a file".to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "path": {"type": "string"},
                "content": {"type": "string"}
            }
        }),
    };
    let tool3 = maple_llm::request::ToolDefinition {
        name: "search_web".to_string(),
        description: "Search the web for information".to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "query": {"type": "string"}
            }
        }),
    };

    registry.register(tool1).await.unwrap();
    registry.register(tool2).await.unwrap();
    registry.register(tool3).await.unwrap();

    // Keyword search for "file" should match read_file and write_file
    let results = registry.search_by_keyword("file", Some(10)).await;
    assert!(results.len() >= 2, "Expected at least 2 file tools, got {}", results.len());
    let names: Vec<&str> = results.iter().map(|t| t.name.as_str()).collect();
    assert!(names.contains(&"read_file"));
    assert!(names.contains(&"write_file"));

    // Keyword search for "web" should match search_web
    let results = registry.search_by_keyword("web", Some(10)).await;
    assert!(results.len() >= 1);
    assert_eq!(results[0].name, "search_web");

    // Stats
    let stats = registry.stats().await;
    assert_eq!(stats.total_tools, 3);
}

/// LLM Provider ecosystem: builtin_providers returns all 14 providers
#[test]
fn test_builtin_providers_integration() {
    use maple_llm::provider_profile::builtin_providers;

    let registry = builtin_providers();
    assert_eq!(registry.profiles.len(), 14, "Expected 14 providers");

    // Verify key providers exist
    assert!(registry.get("openai").is_some());
    assert!(registry.get("anthropic").is_some());
    assert!(registry.get("deepseek").is_some());
    assert!(registry.get("qwen").is_some());
    assert!(registry.get("glm").is_some());
    assert!(registry.get("google").is_some());
    assert!(registry.get("mistral").is_some());
    assert!(registry.get("groq").is_some());
    assert!(registry.get("moonshot").is_some());
    assert!(registry.get("yi").is_some());
    assert!(registry.get("baichuan").is_some());
    assert!(registry.get("minimax").is_some());
    assert!(registry.get("stepfun").is_some());
    assert!(registry.get("ollama").is_some());
}

/// Cron scheduler: natural language parsing
#[test]
fn test_cron_natural_language_integration() {
    // Minutes
    let expr = CronExpression::parse_natural_language("every 5 minutes").unwrap();
    assert!(expr.matches(0, 12, 1, 1, 0));  // minute 0 matches */5
    assert!(expr.matches(5, 12, 1, 1, 0));
    assert!(!expr.matches(7, 12, 1, 1, 0));

    // Hours
    let expr = CronExpression::parse_natural_language("every 2 hours").unwrap();
    assert!(expr.matches(0, 0, 1, 1, 0));
    assert!(expr.matches(0, 2, 1, 1, 0));
    assert!(!expr.matches(0, 3, 1, 1, 0));

    // Daily at specific time
    let expr = CronExpression::parse_natural_language("daily at 9:00").unwrap();
    assert!(expr.matches(0, 9, 1, 1, 0));
    assert!(!expr.matches(0, 10, 1, 1, 0));

    // Weekly on Monday
    let expr = CronExpression::parse_natural_language("weekly on Monday at 14:00").unwrap();
    assert!(expr.matches(0, 14, 1, 1, 1));  // Monday = 1
    assert!(!expr.matches(0, 14, 1, 1, 0)); // Sunday != Monday

    // Shortcuts
    let expr = CronExpression::parse_natural_language("hourly").unwrap();
    assert!(expr.matches(0, 0, 1, 1, 0));

    let expr = CronExpression::parse_natural_language("daily").unwrap();
    assert!(expr.matches(0, 0, 1, 1, 0));
}

/// Terminal backend: BackendRegistry with multiple backends
#[tokio::test]
async fn test_terminal_backend_registry_integration() {
    let mut registry = BackendRegistry::new();
    registry.register(Box::new(LocalBackend::new()));

    let available = registry.available_backends().await;
    assert!(available.len() >= 1);

    // Local backend should be available
    let local = registry.get("local");
    assert!(local.is_some());

    // Execute a simple command
    let backend = local.unwrap();
    let result = backend.execute("echo hello", None).await.unwrap();
    assert_eq!(result.exit_code, 0);
    assert!(result.stdout.contains("hello"));
}

/// Config hierarchy: three-level merge
#[test]
fn test_config_hierarchy_integration() {
    use maple_agent::ConfigHierarchy;
    use std::fs;
    use tempfile::TempDir;

    let user_dir = TempDir::new().unwrap();
    let project_dir = TempDir::new().unwrap();

    // User config
    fs::create_dir_all(user_dir.path().join(".mapleos")).unwrap();
    fs::write(
        user_dir.path().join(".mapleos/config.yaml"),
        "llm:\n  default_provider: anthropic\n  temperature: 0.5\n",
    ).unwrap();

    // Project config overrides model
    fs::create_dir_all(project_dir.path().join(".mapleos")).unwrap();
    fs::write(
        project_dir.path().join(".mapleos/config.yaml"),
        "llm:\n  default_model: deepseek-chat\n",
    ).unwrap();

    // Local config overrides provider
    fs::write(
        project_dir.path().join(".mapleos/local.yaml"),
        "llm:\n  default_provider: ollama\n",
    ).unwrap();

    let mut hierarchy = ConfigHierarchy::with_dirs(user_dir.path(), project_dir.path());
    let config = hierarchy.load().unwrap();

    // Local overrides user
    assert_eq!(config.llm.default_provider, "ollama");
    // Project model survives
    assert_eq!(config.llm.default_model, "deepseek-chat");
    // User temperature survives (not overridden)
    assert_eq!(config.llm.temperature, 0.5);

    // Path-based access
    assert_eq!(
        hierarchy.get("llm.default_provider").unwrap(),
        serde_json::Value::String("ollama".to_string())
    );

    // Sources
    let sources = hierarchy.sources();
    assert_eq!(sources.len(), 3);
    assert!(sources[0].exists);  // user
    assert!(sources[1].exists);  // project
    assert!(sources[2].exists);  // local
}
