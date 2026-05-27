use prometheus::{Encoder, Gauge, Histogram, IntCounter, IntGauge, Registry, TextEncoder};
use std::sync::Arc;

/// 应用监控指标
#[derive(Debug, Clone)]
pub struct AppMetrics {
    /// 指标注册表
    registry: Arc<Registry>,
    
    /// HTTP请求总数
    pub http_requests_total: IntCounter,
    
    /// HTTP请求持续时间
    pub http_request_duration: Histogram,
    
    /// 活跃连接数
    pub active_connections: IntGauge,
    
    /// LLM请求总数
    pub llm_requests_total: IntCounter,
    
    /// LLM请求持续时间
    pub llm_request_duration: Histogram,
    
    /// LLM token使用量
    pub llm_tokens_used: IntCounter,
    
    /// 数据库查询总数
    pub db_queries_total: IntCounter,
    
    /// 数据库查询持续时间
    pub db_query_duration: Histogram,
    
    /// 缓存命中率
    pub cache_hits: IntCounter,
    
    /// 缓存未命中率
    pub cache_misses: IntCounter,
    
    /// 活跃Agent数
    pub active_agents: IntGauge,
    
    /// 活跃工作流数
    pub active_workflows: IntGauge,
    
    /// 系统内存使用量
    pub memory_usage: Gauge,
    
    /// 系统CPU使用率
    pub cpu_usage: Gauge,
}

impl AppMetrics {
    pub fn new() -> Self {
        let registry = Registry::new();
        
        let http_requests_total = IntCounter::new(
            "http_requests_total",
            "Total number of HTTP requests"
        ).unwrap();
        
        let http_request_duration = Histogram::with_opts(
            prometheus::HistogramOpts::new(
                "http_request_duration_seconds",
                "HTTP request duration in seconds"
            )
            .buckets(vec![0.001, 0.005, 0.01, 0.05, 0.1, 0.5, 1.0, 5.0, 10.0])
        ).unwrap();
        
        let active_connections = IntGauge::new(
            "active_connections",
            "Number of active connections"
        ).unwrap();
        
        let llm_requests_total = IntCounter::new(
            "llm_requests_total",
            "Total number of LLM requests"
        ).unwrap();
        
        let llm_request_duration = Histogram::with_opts(
            prometheus::HistogramOpts::new(
                "llm_request_duration_seconds",
                "LLM request duration in seconds"
            )
            .buckets(vec![0.1, 0.5, 1.0, 2.0, 5.0, 10.0, 30.0, 60.0])
        ).unwrap();
        
        let llm_tokens_used = IntCounter::new(
            "llm_tokens_used_total",
            "Total number of LLM tokens used"
        ).unwrap();
        
        let db_queries_total = IntCounter::new(
            "db_queries_total",
            "Total number of database queries"
        ).unwrap();
        
        let db_query_duration = Histogram::with_opts(
            prometheus::HistogramOpts::new(
                "db_query_duration_seconds",
                "Database query duration in seconds"
            )
            .buckets(vec![0.001, 0.005, 0.01, 0.05, 0.1, 0.5, 1.0])
        ).unwrap();
        
        let cache_hits = IntCounter::new(
            "cache_hits_total",
            "Total number of cache hits"
        ).unwrap();
        
        let cache_misses = IntCounter::new(
            "cache_misses_total",
            "Total number of cache misses"
        ).unwrap();
        
        let active_agents = IntGauge::new(
            "active_agents",
            "Number of active agents"
        ).unwrap();
        
        let active_workflows = IntGauge::new(
            "active_workflows",
            "Number of active workflows"
        ).unwrap();
        
        let memory_usage = Gauge::new(
            "memory_usage_bytes",
            "Memory usage in bytes"
        ).unwrap();
        
        let cpu_usage = Gauge::new(
            "cpu_usage_percent",
            "CPU usage percentage"
        ).unwrap();
        
        // 注册所有指标
        registry.register(Box::new(http_requests_total.clone())).unwrap();
        registry.register(Box::new(http_request_duration.clone())).unwrap();
        registry.register(Box::new(active_connections.clone())).unwrap();
        registry.register(Box::new(llm_requests_total.clone())).unwrap();
        registry.register(Box::new(llm_request_duration.clone())).unwrap();
        registry.register(Box::new(llm_tokens_used.clone())).unwrap();
        registry.register(Box::new(db_queries_total.clone())).unwrap();
        registry.register(Box::new(db_query_duration.clone())).unwrap();
        registry.register(Box::new(cache_hits.clone())).unwrap();
        registry.register(Box::new(cache_misses.clone())).unwrap();
        registry.register(Box::new(active_agents.clone())).unwrap();
        registry.register(Box::new(active_workflows.clone())).unwrap();
        registry.register(Box::new(memory_usage.clone())).unwrap();
        registry.register(Box::new(cpu_usage.clone())).unwrap();
        
        Self {
            registry: Arc::new(registry),
            http_requests_total,
            http_request_duration,
            active_connections,
            llm_requests_total,
            llm_request_duration,
            llm_tokens_used,
            db_queries_total,
            db_query_duration,
            cache_hits,
            cache_misses,
            active_agents,
            active_workflows,
            memory_usage,
            cpu_usage,
        }
    }
    
    /// 导出Prometheus格式的指标
    pub fn export(&self) -> String {
        let encoder = TextEncoder::new();
        let metric_families = self.registry.gather();
        let mut buffer = Vec::new();
        encoder.encode(&metric_families, &mut buffer).unwrap();
        String::from_utf8(buffer).unwrap()
    }
    
    /// 更新系统指标 (使用 /proc 或 sysinfo 获取真实值)
    pub fn update_system_metrics(&self) {
        // Read memory usage from /proc/self/status (Linux) or estimate from process
        #[cfg(target_os = "linux")]
        {
            if let Ok(status) = std::fs::read_to_string("/proc/self/status") {
                for line in status.lines() {
                    if line.starts_with("VmRSS:") {
                        if let Some(kb_str) = line.split_whitespace().nth(1) {
                            if let Ok(kb) = kb_str.parse::<f64>() {
                                self.memory_usage.set(kb * 1024.0); // Convert KB to bytes
                            }
                        }
                    }
                }
            }
        }

        // On non-Linux or if /proc read fails, use a conservative estimate
        #[cfg(not(target_os = "linux"))]
        {
            // Use 50MB as baseline estimate when we can't read real values
            self.memory_usage.set(1024.0 * 1024.0 * 50.0);
        }

        // CPU usage requires sampling over time; use 0 as "not yet measured"
        // A background task should periodically compute delta and update this
        // For now, leave at 0 to indicate "no data" rather than a fake value
    }
}

impl Default for AppMetrics {
    fn default() -> Self {
        Self::new()
    }
}

/// 指标中间件
pub async fn metrics_middleware(
    axum::extract::State(state): axum::extract::State<Arc<crate::AppState>>,
    req: axum::http::Request<axum::body::Body>,
    next: axum::middleware::Next,
) -> impl axum::response::IntoResponse {
    let start = std::time::Instant::now();
    let method = req.method().clone();
    let path = req.uri().path().to_string();

    let response = next.run(req).await;

    let duration = start.elapsed();
    let status = response.status().as_u16();

    // Record HTTP metrics to Prometheus
    state.metrics.http_requests_total.inc();
    state.metrics.http_request_duration.observe(duration.as_secs_f64());

    // Log slow requests (> 2s)
    if duration.as_secs() >= 2 {
        tracing::warn!(
            method = %method,
            path = %path,
            status = status,
            duration_ms = duration.as_millis(),
            "Slow request"
        );
    }

    response
}

/// 指标端点处理函数
pub async fn metrics_handler(
    axum::extract::State(state): axum::extract::State<Arc<crate::AppState>>,
) -> String {
    state.metrics.update_system_metrics();
    state.metrics.export()
}

/// 健康检查端点
pub async fn health_handler() -> &'static str {
    "ok"
}

/// 深度健康检查端点
pub async fn deep_health_handler(
    axum::extract::State(state): axum::extract::State<Arc<crate::AppState>>,
) -> axum::Json<serde_json::Value> {
    let db_healthy = sqlx::query("SELECT 1")
        .execute(&state.db)
        .await
        .is_ok();
    
    let llm_healthy = state.llm_router.list_models().await.len() > 0;
    
    axum::Json(serde_json::json!({
        "status": if db_healthy && llm_healthy { "ok" } else { "degraded" },
        "timestamp": chrono::Utc::now().timestamp(),
        "checks": {
            "database": db_healthy,
            "llm": llm_healthy,
        }
    }))
}