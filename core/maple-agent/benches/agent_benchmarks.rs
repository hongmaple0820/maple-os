use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use std::sync::Arc;

use maple_agent::{
    lane_events::{Lane, LaneManager, LanePolicy, LaneStatus},
    mcp_client::{strip_credentials, ParallelToolExecutor, ToolSyncManager},
    platform_adapter::{MessageContent, MockAdapter, OutboundMessage, PlatformRegistry},
    skill_discovery::{ActivationRule, Skill, SkillContext, SkillDiscovery},
    trajectory::{TrajectoryCompressor, ScoringWeights},
    workflow_dag::{end_node, start_node, tool_node, WorkflowBuilder, WorkflowExecutor},
    OutcomeType, StepOutcome, TrainingTrajectory, TrajectoryStep,
};

fn bench_trident_compaction(c: &mut Criterion) {
    use maple_agent::trident::{TridentCompactor, TridentConfig};
    use maple_llm::request::Message;

    let mut group = c.benchmark_group("trident_compaction");

    for msg_count in [20, 50, 100] {
        let messages: Vec<Message> = (0..msg_count)
            .map(|i| match i % 3 {
                0 => Message::user(&format!("User message {}", i / 3)),
                1 => Message::assistant(&format!("I'll read file{}.rs", i / 3)),
                _ => Message::tool_result(
                    &format!("call_{}", i / 3),
                    &format!("File content of file{}.rs", i / 3),
                    false,
                ),
            })
            .collect();

        group.bench_with_input(
            BenchmarkId::new("compact", msg_count),
            &messages,
            |b, msgs| {
                b.iter(|| {
                    let mut compactor = TridentCompactor::new(TridentConfig::default());
                    compactor.compact(black_box(msgs));
                });
            },
        );
    }
    group.finish();
}

fn bench_skill_discovery(c: &mut Criterion) {
    let mut group = c.benchmark_group("skill_discovery");

    for skill_count in [10, 50, 100] {
        let skills: Vec<Skill> = (0..skill_count)
            .map(|i| Skill {
                id: format!("skill_{}", i),
                name: format!("Skill {}", i),
                description: format!("Skill number {}", i),
                tools: vec![format!("tool_{}", i)],
                activation_rules: vec![
                    ActivationRule::Keyword {
                        words: vec![format!("keyword_{}", i)],
                    },
                    ActivationRule::FilePath {
                        pattern: format!("**/*.{}", if i % 2 == 0 { "rs" } else { "ts" }),
                    },
                ],
                active: true,
                priority: 0,
            })
            .collect();

        let mut discovery = SkillDiscovery::new();
        for skill in skills {
            discovery.register(skill);
        }
        let context = SkillContext {
            workspace_root: ".".into(),
            current_files: vec!["src/main.rs".into()],
            user_message: "using keyword_5 in my code".into(),
            active_skills: vec![],
        };

        group.bench_with_input(
            BenchmarkId::new("evaluate", skill_count),
            &context,
            |b, ctx| {
                b.iter(|| {
                    discovery.evaluate(black_box(ctx));
                });
            },
        );
    }
    group.finish();
}

fn bench_strip_credentials(c: &mut Criterion) {
    let mut group = c.benchmark_group("credential_stripping");

    let small_json = serde_json::json!({
        "name": "test",
        "api_key": "sk-1234567890abcdef",
        "data": "safe value"
    });

    let large_json = serde_json::json!({
        "users": (0..100).map(|i| serde_json::json!({
            "id": i,
            "name": format!("user_{}", i),
            "token": format!("tok_{}_abcdef1234567890", i),
            "secret": format!("sec_{}_xyz", i),
            "email": format!("user{}@example.com", i),
        })).collect::<Vec<_>>(),
        "config": {
            "api_key": "master-key-123",
            "database_password": "db-pass-456",
            "safe_setting": true
        }
    });

    group.bench_function("small_json", |b| {
        b.iter(|| strip_credentials(black_box(&small_json)));
    });

    group.bench_function("large_json", |b| {
        b.iter(|| strip_credentials(black_box(&large_json)));
    });

    group.finish();
}

fn bench_workflow_dag(c: &mut Criterion) {
    let mut group = c.benchmark_group("workflow_dag");

    group.bench_function("validate_small", |b| {
        let def = WorkflowBuilder::new("w", "test")
            .add_node(start_node("s", "t1"))
            .add_node(tool_node("t1", "read", "t2"))
            .add_node(tool_node("t2", "write", "end"))
            .add_node(end_node("end"))
            .build();
        let executor = WorkflowExecutor::new(def);
        b.iter(|| executor.validate());
    });

    group.bench_function("validate_medium", |b| {
        let mut builder = WorkflowBuilder::new("w", "test").add_node(start_node("s", "t0"));

        for i in 0..20 {
            let next = if i < 19 {
                format!("t{}", i + 1)
            } else {
                "end".into()
            };
            builder =
                builder.add_node(tool_node(&format!("t{}", i), &format!("tool_{}", i), &next));
        }

        let def = builder.add_node(end_node("end")).build();
        let executor = WorkflowExecutor::new(def);
        b.iter(|| executor.validate());
    });

    group.bench_function("ready_nodes", |b| {
        let def = WorkflowBuilder::new("w", "test")
            .add_node(start_node("s", "t1"))
            .add_node(tool_node("t1", "read", "t2"))
            .add_node(tool_node("t2", "write", "t3"))
            .add_node(tool_node("t3", "exec", "end"))
            .add_node(end_node("end"))
            .build();
        let mut executor = WorkflowExecutor::new(def);
        executor.validate().unwrap();
        executor.complete_node("s", None).unwrap();
        executor.complete_node("t1", None).unwrap();
        b.iter(|| executor.ready_nodes());
    });

    group.finish();
}

fn bench_parallel_tool_executor(c: &mut Criterion) {
    let mut group = c.benchmark_group("parallel_tools");

    for concurrency in [2, 5, 10] {
        group.bench_with_input(
            BenchmarkId::new("execute", concurrency),
            &concurrency,
            |b, &conc| {
                let rt = tokio::runtime::Runtime::new().unwrap();
                let executor = ParallelToolExecutor::new(conc);
                b.iter(|| {
                    let calls: Vec<(String, String)> = (0..10)
                        .map(|i| (format!("s{}", i), format!("tool_{}", i)))
                        .collect();
                    rt.block_on(executor.execute_parallel(calls, |_, _| async {
                        Ok(serde_json::json!({ "ok": true }))
                    }));
                });
            },
        );
    }
    group.finish();
}

fn bench_lane_manager(c: &mut Criterion) {
    let mut group = c.benchmark_group("lane_manager");

    group.bench_function("complete_lifecycle", |b| {
        b.iter(|| {
            let mut mgr = LaneManager::new();
            for i in 0..10 {
                mgr.add_lane(Lane {
                    id: format!("l{}", i),
                    name: format!("Lane {}", i),
                    steps: (0..5).map(|s| format!("step_{}", s)).collect(),
                    policy: LanePolicy::default(),
                    status: LaneStatus::Idle,
                    current_step_idx: 0,
                });
            }
            for i in 0..10 {
                let id = format!("l{}", i);
                mgr.start_lane(&id).unwrap();
                for _ in 0..5 {
                    mgr.complete_step(&id).unwrap();
                }
            }
        });
    });

    group.finish();
}

fn bench_trajectory_scoring(c: &mut Criterion) {
    let mut group = c.benchmark_group("trajectory");

    let compressor = TrajectoryCompressor::new(ScoringWeights::default());
    let trajectory = TrainingTrajectory {
        id: "bench".into(),
        task_description: "benchmark task".into(),
        steps: (0..20)
            .map(|i| TrajectoryStep {
                summary: format!("Step {}", i),
                decision: None,
                tool: Some(format!("tool_{}", i % 5)),
                tool_result_summary: Some("ok".into()),
                outcome: StepOutcome::Success,
            })
            .collect(),
        tools_used: (0..5).map(|i| format!("tool_{}", i)).collect(),
        total_tokens: 5000,
        final_outcome: OutcomeType::Success,
        quality_score: 0.0,
    };

    group.bench_function("score", |b| {
        b.iter(|| compressor.score(black_box(&trajectory)));
    });

    group.finish();
}

fn bench_platform_registry(c: &mut Criterion) {
    let mut group = c.benchmark_group("platform_registry");

    let mut reg = PlatformRegistry::new();
    for name in &["telegram", "discord", "slack", "feishu", "dingtalk"] {
        reg.register(Arc::new(MockAdapter::new(name)));
    }

    group.bench_function("route_message", |b| {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let msg = OutboundMessage {
            content: MessageContent::Text("hello".into()),
            reply_to: None,
            thread_id: None,
        };
        b.iter(|| {
            rt.block_on(reg.route_message("telegram", "ch1", black_box(&msg)))
                .unwrap();
        });
    });

    group.bench_function("with_capability", |b| {
        b.iter(|| reg.with_capability(|c| c.rich_text));
    });

    group.finish();
}

fn bench_tool_sync(c: &mut Criterion) {
    let mut group = c.benchmark_group("tool_sync");

    let mut sync = ToolSyncManager::new();
    let tools: Vec<String> = (0..50).map(|i| format!("tool_{}", i)).collect();
    sync.update_tools("server1", tools.clone());

    group.bench_function("update_no_change", |b| {
        b.iter(|| sync.update_tools("server1", black_box(tools.clone())));
    });

    group.bench_function("update_with_changes", |b| {
        let mut counter = 0u32;
        b.iter(|| {
            counter += 1;
            let mut new_tools = tools.clone();
            new_tools.push(format!("new_tool_{}", counter));
            sync.update_tools("server1", black_box(new_tools));
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_trident_compaction,
    bench_skill_discovery,
    bench_strip_credentials,
    bench_workflow_dag,
    bench_parallel_tool_executor,
    bench_lane_manager,
    bench_trajectory_scoring,
    bench_platform_registry,
    bench_tool_sync,
);

criterion_main!(benches);
