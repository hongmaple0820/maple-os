use serde::{Deserialize, Serialize};

/// Mixture-of-Agents (MoA) — parallel multi-model reasoning
///
/// Runs the same prompt through multiple models in parallel,
/// then aggregates results using various strategies.
///
///   MoA model configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MoAModel {
    pub model_id: String,
    pub temperature: Option<f32>,
    pub weight: f64,
}

/// Aggregation strategy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AggregationStrategy {
    /// Majority voting (for classification/judgment)
    MajorityVote,
    /// Weighted voting (by model weight)
    WeightedVote,
    /// Longest best (for code generation)
    LongestBest,
    /// First success
    FirstSuccess,
    /// Custom selection by quality score
    QualityScore,
}

/// Single model response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MoAResponse {
    pub model_id: String,
    pub content: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub latency_ms: u64,
    pub success: bool,
}

/// Aggregated MoA result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MoAResult {
    pub selected: MoAResponse,
    pub all_responses: Vec<MoAResponse>,
    pub strategy_used: String,
}

/// MoA configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MoAConfig {
    pub models: Vec<MoAModel>,
    pub strategy: AggregationStrategy,
    pub timeout_ms: u64,
    pub min_responses: usize,
}

impl Default for MoAConfig {
    fn default() -> Self {
        Self {
            models: Vec::new(),
            strategy: AggregationStrategy::MajorityVote,
            timeout_ms: 30000,
            min_responses: 1,
        }
    }
}

/// Mixture-of-Agents engine
pub struct MixtureOfAgents {
    config: MoAConfig,
}

impl MixtureOfAgents {
    pub fn new(config: MoAConfig) -> Self {
        Self { config }
    }

    /// Aggregate multiple responses using the configured strategy
    pub fn aggregate(&self, responses: &[MoAResponse]) -> Option<MoAResult> {
        if responses.len() < self.config.min_responses {
            return None;
        }

        let successful: Vec<&MoAResponse> = responses.iter().filter(|r| r.success).collect();
        if successful.is_empty() {
            return None;
        }

        let selected = match self.config.strategy {
            AggregationStrategy::MajorityVote => self.majority_vote(&successful),
            AggregationStrategy::WeightedVote => self.weighted_vote(&successful),
            AggregationStrategy::LongestBest => self.longest_best(&successful),
            AggregationStrategy::FirstSuccess => successful[0].clone(),
            AggregationStrategy::QualityScore => self.quality_score(&successful),
        };

        Some(MoAResult {
            selected,
            all_responses: responses.to_vec(),
            strategy_used: format!("{:?}", self.config.strategy),
        })
    }

    fn majority_vote<'a>(&self, responses: &[&'a MoAResponse]) -> MoAResponse {
        // Group by content similarity (simplified: exact match)
        let mut counts: std::collections::HashMap<String, (usize, &'a MoAResponse)> =
            std::collections::HashMap::new();

        for resp in responses {
            let entry = counts
                .entry(resp.content.clone())
                .or_insert((0, resp));
            entry.0 += 1;
        }

        counts
            .values()
            .max_by_key(|(count, _)| *count)
            .map(|(_, resp)| (*resp).clone())
            .unwrap_or_else(|| responses[0].clone())
    }

    fn weighted_vote(&self, responses: &[&MoAResponse]) -> MoAResponse {
        // Score each response by model weight
        let mut best_score = 0.0f64;
        let mut best = responses[0];

        for resp in responses {
            let weight = self
                .config
                .models
                .iter()
                .find(|m| m.model_id == resp.model_id)
                .map(|m| m.weight)
                .unwrap_or(1.0);

            if weight > best_score {
                best_score = weight;
                best = resp;
            }
        }

        best.clone()
    }

    fn longest_best(&self, responses: &[&MoAResponse]) -> MoAResponse {
        responses
            .iter()
            .max_by_key(|r| r.content.len())
            .map(|r| (*r).clone())
            .unwrap_or_else(|| responses[0].clone())
    }

    fn quality_score(&self, responses: &[&MoAResponse]) -> MoAResponse {
        // Score by: content length + model weight - latency penalty
        let mut best_score = f64::MIN;
        let mut best = responses[0];

        for resp in responses {
            let weight = self
                .config
                .models
                .iter()
                .find(|m| m.model_id == resp.model_id)
                .map(|m| m.weight)
                .unwrap_or(1.0);

            let length_score = resp.content.len() as f64;
            let latency_penalty = resp.latency_ms as f64 * 0.001;
            let score = weight * length_score - latency_penalty;

            if score > best_score {
                best_score = score;
                best = resp;
            }
        }

        best.clone()
    }

    /// Get configured models
    pub fn models(&self) -> &[MoAModel] {
        &self.config.models
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_response(model_id: &str, content: &str) -> MoAResponse {
        MoAResponse {
            model_id: model_id.into(),
            content: content.into(),
            input_tokens: 100,
            output_tokens: 50,
            latency_ms: 1000,
            success: true,
        }
    }

    fn failed_response(model_id: &str) -> MoAResponse {
        MoAResponse {
            model_id: model_id.into(),
            content: String::new(),
            input_tokens: 0,
            output_tokens: 0,
            latency_ms: 0,
            success: false,
        }
    }

    #[test]
    fn test_majority_vote() {
        let config = MoAConfig {
            strategy: AggregationStrategy::MajorityVote,
            min_responses: 1,
            ..Default::default()
        };
        let moa = MixtureOfAgents::new(config);

        let responses = vec![
            test_response("gpt4", "answer A"),
            test_response("claude", "answer A"),
            test_response("local", "answer B"),
        ];

        let result = moa.aggregate(&responses).unwrap();
        assert_eq!(result.selected.content, "answer A");
    }

    #[test]
    fn test_weighted_vote() {
        let config = MoAConfig {
            models: vec![
                MoAModel {
                    model_id: "gpt4".into(),
                    temperature: None,
                    weight: 3.0,
                },
                MoAModel {
                    model_id: "claude".into(),
                    temperature: None,
                    weight: 1.0,
                },
            ],
            strategy: AggregationStrategy::WeightedVote,
            min_responses: 1,
            ..Default::default()
        };
        let moa = MixtureOfAgents::new(config);

        let responses = vec![
            test_response("gpt4", "gpt answer"),
            test_response("claude", "claude answer"),
        ];

        let result = moa.aggregate(&responses).unwrap();
        assert_eq!(result.selected.model_id, "gpt4");
    }

    #[test]
    fn test_longest_best() {
        let config = MoAConfig {
            strategy: AggregationStrategy::LongestBest,
            min_responses: 1,
            ..Default::default()
        };
        let moa = MixtureOfAgents::new(config);

        let responses = vec![
            test_response("a", "short"),
            test_response("b", "this is a much longer and more detailed answer"),
        ];

        let result = moa.aggregate(&responses).unwrap();
        assert_eq!(result.selected.model_id, "b");
    }

    #[test]
    fn test_first_success() {
        let config = MoAConfig {
            strategy: AggregationStrategy::FirstSuccess,
            min_responses: 1,
            ..Default::default()
        };
        let moa = MixtureOfAgents::new(config);

        let responses = vec![
            failed_response("a"),
            test_response("b", "success"),
        ];

        let result = moa.aggregate(&responses).unwrap();
        assert_eq!(result.selected.model_id, "b");
    }

    #[test]
    fn test_min_responses_not_met() {
        let config = MoAConfig {
            min_responses: 3,
            ..Default::default()
        };
        let moa = MixtureOfAgents::new(config);

        let responses = vec![
            test_response("a", "answer"),
            test_response("b", "answer"),
        ];

        assert!(moa.aggregate(&responses).is_none());
    }

    #[test]
    fn test_all_failed() {
        let config = MoAConfig {
            min_responses: 1,
            ..Default::default()
        };
        let moa = MixtureOfAgents::new(config);

        let responses = vec![
            failed_response("a"),
            failed_response("b"),
        ];

        assert!(moa.aggregate(&responses).is_none());
    }

    #[test]
    fn test_empty_responses() {
        let config = MoAConfig::default();
        let moa = MixtureOfAgents::new(config);
        assert!(moa.aggregate(&[]).is_none());
    }
}
