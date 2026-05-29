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
