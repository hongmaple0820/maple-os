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
}

impl UsageTracker {
    pub fn new(daily_limit_usd: f64) -> Self {
        Self {
            daily_limit_usd,
            current_usage: Arc::new(RwLock::new(DailyUsage::default())),
        }
    }

    pub async fn record(&self, input_tokens: usize, output_tokens: usize, cost_usd: f64) {
        let mut usage = self.current_usage.write().await;
        usage.total_input_tokens += input_tokens as u64;
        usage.total_output_tokens += output_tokens as u64;
        usage.total_usd += cost_usd;
    }

    pub async fn daily_budget_exceeded(&self) -> bool {
        let usage = self.current_usage.read().await;
        usage.total_usd >= self.daily_limit_usd
    }

    pub async fn get_usage(&self) -> (f64, u64, u64) {
        let usage = self.current_usage.read().await;
        (usage.total_usd, usage.total_input_tokens, usage.total_output_tokens)
    }
}
