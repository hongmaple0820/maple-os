use serde::{Deserialize, Serialize};

/// Trajectory Compression — extract training data from agent sessions
///
/// Compresses long agent trajectories into structured training samples:
/// - Extracts decision points and outcomes
/// - Scores trajectory quality
/// - Exports to JSONL for fine-tuning
///
///   Compressed training trajectory
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrainingTrajectory {
    pub id: String,
    pub task_description: String,
    pub steps: Vec<TrajectoryStep>,
    pub tools_used: Vec<String>,
    pub total_tokens: u64,
    pub final_outcome: OutcomeType,
    pub quality_score: f64,
}

/// Single step in trajectory
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrajectoryStep {
    pub summary: String,
    pub decision: Option<String>,
    pub tool: Option<String>,
    pub tool_result_summary: Option<String>,
    pub outcome: StepOutcome,
}

/// Step outcome
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StepOutcome {
    Success,
    Failed,
    Skipped,
}

/// Final outcome
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OutcomeType {
    Success,
    PartialSuccess,
    Failed,
    Aborted,
}

/// Scoring weights
#[derive(Debug, Clone)]
pub struct ScoringWeights {
    pub completion: f64,
    pub efficiency: f64,
    pub decision_quality: f64,
    pub tool_usage: f64,
}

impl Default for ScoringWeights {
    fn default() -> Self {
        Self {
            completion: 0.4,
            efficiency: 0.2,
            decision_quality: 0.25,
            tool_usage: 0.15,
        }
    }
}

/// Trajectory compressor
pub struct TrajectoryCompressor {
    weights: ScoringWeights,
}

impl TrajectoryCompressor {
    pub fn new(weights: ScoringWeights) -> Self {
        Self { weights }
    }

    /// Compress a conversation into a training trajectory
    pub fn compress(
        &self,
        id: &str,
        task: &str,
        messages: &[(String, String)], // (role, content) pairs
    ) -> TrainingTrajectory {
        let mut steps = Vec::new();
        let mut tools_used = Vec::new();
        let mut total_tokens = 0u64;

        for (role, content) in messages {
            total_tokens += content.len() as u64 / 4; // rough estimate

            match role.as_str() {
                "assistant" => {
                    let tool = extract_tool_name(content);
                    if let Some(ref t) = tool
                        && !tools_used.contains(t)
                    {
                        tools_used.push(t.clone());
                    }
                    steps.push(TrajectoryStep {
                        summary: summarize_content(content, 200),
                        decision: extract_decision(content),
                        tool,
                        tool_result_summary: None,
                        outcome: StepOutcome::Success,
                    });
                }
                "tool" => {
                    if let Some(last) = steps.last_mut() {
                        let is_error = content.contains("error") || content.contains("Error");
                        last.tool_result_summary = Some(summarize_content(content, 100));
                        if is_error {
                            last.outcome = StepOutcome::Failed;
                        }
                    }
                }
                _ => {}
            }
        }

        let failed_steps = steps
            .iter()
            .filter(|s| s.outcome == StepOutcome::Failed)
            .count();
        let final_outcome = if failed_steps == 0 {
            OutcomeType::Success
        } else if failed_steps < steps.len() / 2 {
            OutcomeType::PartialSuccess
        } else {
            OutcomeType::Failed
        };

        let trajectory = TrainingTrajectory {
            id: id.into(),
            task_description: task.into(),
            steps,
            tools_used,
            total_tokens,
            final_outcome,
            quality_score: 0.0,
        };

        let quality_score = self.score(&trajectory);

        TrainingTrajectory {
            quality_score,
            ..trajectory
        }
    }

    /// Score trajectory quality
    pub fn score(&self, trajectory: &TrainingTrajectory) -> f64 {
        let completion = match trajectory.final_outcome {
            OutcomeType::Success => 1.0,
            OutcomeType::PartialSuccess => 0.6,
            OutcomeType::Failed => 0.2,
            OutcomeType::Aborted => 0.0,
        };

        let efficiency = if trajectory.steps.is_empty() {
            0.0
        } else {
            // Fewer steps = more efficient (diminishing returns)
            let step_ratio = 1.0 / (1.0 + trajectory.steps.len() as f64 * 0.1);
            // Lower token usage = more efficient
            let token_ratio = 1.0 / (1.0 + trajectory.total_tokens as f64 * 0.00001);
            (step_ratio + token_ratio) / 2.0
        };

        let failed_count = trajectory
            .steps
            .iter()
            .filter(|s| s.outcome == StepOutcome::Failed)
            .count();
        let decision_quality = if trajectory.steps.is_empty() {
            0.5
        } else {
            1.0 - (failed_count as f64 / trajectory.steps.len() as f64)
        };

        let tool_usage = if trajectory.tools_used.is_empty() {
            0.5 // No tools used
        } else {
            // Using diverse tools is good
            (trajectory.tools_used.len() as f64 * 0.2).min(1.0)
        };

        self.weights.completion * completion
            + self.weights.efficiency * efficiency
            + self.weights.decision_quality * decision_quality
            + self.weights.tool_usage * tool_usage
    }

    /// Export trajectories to JSONL format
    pub fn to_jsonl(trajectories: &[TrainingTrajectory]) -> String {
        trajectories
            .iter()
            .filter_map(|t| serde_json::to_string(t).ok())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

impl Default for TrajectoryCompressor {
    fn default() -> Self {
        Self::new(ScoringWeights::default())
    }
}

fn extract_tool_name(content: &str) -> Option<String> {
    // Try to find tool call in content
    if let Some(start) = content.find("\"name\":\"") {
        let rest = &content[start + 8..];
        if let Some(end) = rest.find('"') {
            return Some(rest[..end].to_string());
        }
    }
    // Check for common tool patterns
    for tool in &[
        "read_file",
        "write_file",
        "execute",
        "search",
        "bash",
        "grep",
    ] {
        if content.contains(tool) {
            return Some(tool.to_string());
        }
    }
    None
}

fn extract_decision(content: &str) -> Option<String> {
    // Look for decision indicators
    let lower = content.to_lowercase();
    for phrase in &[
        "i'll use",
        "let me use",
        "decided to",
        "choosing",
        "switching to",
        "the best approach",
    ] {
        if let Some(pos) = lower.find(phrase) {
            let end = content[pos..]
                .find('.')
                .unwrap_or(content.len() - pos)
                .min(200);
            return Some(content[pos..pos + end].to_string());
        }
    }
    None
}

fn summarize_content(content: &str, max_len: usize) -> String {
    if content.len() <= max_len {
        content.to_string()
    } else {
        format!("{}...", &content[..max_len.saturating_sub(3)])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compress_success() {
        let compressor = TrajectoryCompressor::default();
        let messages = vec![
            ("user".into(), "Write hello world in Python".into()),
            ("assistant".into(), r#"{"name":"write_file","arguments":{"path":"hello.py"}}"#.into()),
            ("tool".into(), "File written successfully".into()),
            ("assistant".into(), "Done! Created hello.py".into()),
        ];

        let trajectory = compressor.compress("t1", "Write hello world", &messages);
        assert_eq!(trajectory.final_outcome, OutcomeType::Success);
        assert!(trajectory.quality_score > 0.0);
        assert!(trajectory.tools_used.contains(&"write_file".to_string()));
    }

    #[test]
    fn test_compress_with_failure() {
        let compressor = TrajectoryCompressor::default();
        // 4 tool calls, 1 fails: 1 < 4/2=2 → PartialSuccess
        let messages = vec![
            ("user".into(), "Run tests".into()),
            ("assistant".into(), r#"{"name":"execute","arguments":{"command":"test1"}}"#.into()),
            ("tool".into(), "Error: test failed".into()),
            ("assistant".into(), r#"{"name":"execute","arguments":{"command":"test2"}}"#.into()),
            ("tool".into(), "passed".into()),
            ("assistant".into(), r#"{"name":"execute","arguments":{"command":"test3"}}"#.into()),
            ("tool".into(), "passed".into()),
            ("assistant".into(), r#"{"name":"execute","arguments":{"command":"test4"}}"#.into()),
            ("tool".into(), "passed".into()),
        ];

        let trajectory = compressor.compress("t1", "Run tests", &messages);
        assert_eq!(trajectory.final_outcome, OutcomeType::PartialSuccess);
    }

    #[test]
    fn test_score_perfect() {
        let compressor = TrajectoryCompressor::default();
        let trajectory = TrainingTrajectory {
            id: "t1".into(),
            task_description: "task".into(),
            steps: vec![
                TrajectoryStep {
                    summary: "step 1".into(),
                    decision: None,
                    tool: Some("read".into()),
                    tool_result_summary: None,
                    outcome: StepOutcome::Success,
                },
            ],
            tools_used: vec!["read".into(), "write".into()],
            total_tokens: 100,
            final_outcome: OutcomeType::Success,
            quality_score: 0.0,
        };

        let score = compressor.score(&trajectory);
        assert!(score > 0.7); // Should be high
    }

    #[test]
    fn test_score_failed() {
        let compressor = TrajectoryCompressor::default();
        let trajectory = TrainingTrajectory {
            id: "t1".into(),
            task_description: "task".into(),
            steps: vec![
                TrajectoryStep {
                    summary: "step".into(),
                    decision: None,
                    tool: None,
                    tool_result_summary: None,
                    outcome: StepOutcome::Failed,
                },
            ],
            tools_used: vec![],
            total_tokens: 100000,
            final_outcome: OutcomeType::Failed,
            quality_score: 0.0,
        };

        let score = compressor.score(&trajectory);
        assert!(score < 0.4); // Should be low
    }

    #[test]
    fn test_to_jsonl() {
        let trajectory = TrainingTrajectory {
            id: "t1".into(),
            task_description: "test".into(),
            steps: vec![],
            tools_used: vec![],
            total_tokens: 0,
            final_outcome: OutcomeType::Success,
            quality_score: 0.8,
        };

        let jsonl = TrajectoryCompressor::to_jsonl(&[trajectory]);
        assert!(jsonl.contains("\"id\":\"t1\""));
        assert!(jsonl.contains("quality_score"));
    }

    #[test]
    fn test_extract_decision() {
        let content = "I'll use the read_file tool to check the contents first.";
        let decision = extract_decision(content);
        assert!(decision.is_some());
        assert!(decision.unwrap().contains("read_file"));
    }

    #[test]
    fn test_summarize_content() {
        let short = summarize_content("hello", 100);
        assert_eq!(short, "hello");

        let long = summarize_content(&"x".repeat(200), 50);
        assert!(long.len() <= 50);
        assert!(long.ends_with("..."));
    }
}
