use std::sync::Arc;
use tokio::sync::RwLock;

pub struct UsageTracker {
    daily_limit_usd: f64,
    current_usage: Arc<RwLock<DailyUsage>>,
}

#[derive(Default)]
struct DailyUsage {
    total_usd: f64,
    total_input_tokens: u64,
    total_output_tokens: u64,
    total_cached_tokens: u64,
    request_count: u64,
}

/// Snapshot of current usage metrics
#[derive(Debug, Clone)]
pub struct UsageSnapshot {
    pub total_usd: f64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cached_tokens: u64,
    pub request_count: u64,
}

impl UsageTracker {
    pub fn new(daily_limit_usd: f64) -> Self {
        Self {
            daily_limit_usd,
            current_usage: Arc::new(RwLock::new(DailyUsage::default())),
        }
    }

    /// Record a completed LLM call
    pub async fn record(&self, input_tokens: usize, output_tokens: usize, cost_usd: f64) {
        self.record_with_cache(input_tokens, output_tokens, 0, cost_usd).await;
    }

    /// Record a completed LLM call with cache token tracking
    pub async fn record_with_cache(
        &self,
        input_tokens: usize,
        output_tokens: usize,
        cached_tokens: usize,
        cost_usd: f64,
    ) {
        let mut usage = self.current_usage.write().await;
        usage.total_input_tokens += input_tokens as u64;
        usage.total_output_tokens += output_tokens as u64;
        usage.total_cached_tokens += cached_tokens as u64;
        usage.total_usd += cost_usd;
        usage.request_count += 1;
    }

    pub async fn daily_budget_exceeded(&self) -> bool {
        let usage = self.current_usage.read().await;
        usage.total_usd >= self.daily_limit_usd
    }

    pub async fn get_usage(&self) -> (f64, u64, u64) {
        let usage = self.current_usage.read().await;
        (
            usage.total_usd,
            usage.total_input_tokens,
            usage.total_output_tokens,
        )
    }

    /// Get detailed usage snapshot
    pub async fn get_snapshot(&self) -> UsageSnapshot {
        let usage = self.current_usage.read().await;
        UsageSnapshot {
            total_usd: usage.total_usd,
            input_tokens: usage.total_input_tokens,
            output_tokens: usage.total_output_tokens,
            cached_tokens: usage.total_cached_tokens,
            request_count: usage.request_count,
        }
    }

    /// Get daily limit
    pub fn daily_limit(&self) -> f64 {
        self.daily_limit_usd
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_basic_usage() {
        let tracker = UsageTracker::new(10.0);

        tracker.record(1000, 500, 0.05).await;

        let (usd, input, output) = tracker.get_usage().await;
        assert_eq!(usd, 0.05);
        assert_eq!(input, 1000);
        assert_eq!(output, 500);
    }

    #[tokio::test]
    async fn test_cache_tracking() {
        let tracker = UsageTracker::new(10.0);

        tracker.record_with_cache(1000, 500, 300, 0.05).await;

        let snapshot = tracker.get_snapshot().await;
        assert_eq!(snapshot.input_tokens, 1000);
        assert_eq!(snapshot.output_tokens, 500);
        assert_eq!(snapshot.cached_tokens, 300);
        assert_eq!(snapshot.request_count, 1);
    }

    #[tokio::test]
    async fn test_budget_exceeded() {
        let tracker = UsageTracker::new(1.0);

        assert!(!tracker.daily_budget_exceeded().await);

        tracker.record(1000, 500, 1.5).await;

        assert!(tracker.daily_budget_exceeded().await);
    }
}
