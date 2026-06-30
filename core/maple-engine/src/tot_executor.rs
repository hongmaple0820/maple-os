use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThoughtNode { pub content: String, pub score: f64, pub depth: usize, pub children: Vec<ThoughtNode> }

#[derive(Debug, Clone)]
pub struct TotConfig { pub branching_factor: usize, pub max_depth: usize, pub beam_width: usize, pub score_threshold: f64 }
impl Default for TotConfig { fn default() -> Self { Self { branching_factor: 3, max_depth: 5, beam_width: 2, score_threshold: 0.9 } } }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TotResult { pub best_thought: String, pub best_score: f64, pub total_nodes_explored: usize, pub depth_reached: usize, pub tree: ThoughtNode }

pub fn solve(problem: &str, config: &TotConfig, expand_fn: &dyn Fn(&str, usize) -> Vec<String>, score_fn: &dyn Fn(&str, &str) -> f64) -> TotResult {
    let mut frontier = vec![ThoughtNode { content: problem.into(), score: 0.5, depth: 0, children: vec![] }];
    let mut total = 0usize; let mut depth_reached = 0usize;
    let root = frontier[0].clone();
    for depth in 0..config.max_depth {
        depth_reached = depth + 1;
        let mut candidates = Vec::new();
        for node in &frontier {
            for exp in expand_fn(&node.content, config.branching_factor) {
                let score = score_fn(&exp, problem);
                candidates.push(ThoughtNode { content: exp, score, depth: depth + 1, children: vec![] });
                total += 1;
            }
        }
        candidates.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        candidates.truncate(config.beam_width);
        if candidates.is_empty() { break; }
        if candidates[0].score >= config.score_threshold { frontier = candidates; break; }
        frontier = candidates;
    }
    let best = frontier.iter().max_by(|a, b| a.score.partial_cmp(&b.score).unwrap_or(std::cmp::Ordering::Equal)).cloned().unwrap_or_else(|| root.clone());
    TotResult { best_thought: best.content, best_score: best.score, total_nodes_explored: total, depth_reached, tree: root }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_expand(c: &str, k: usize) -> Vec<String> {
        (0..k).map(|i| format!("{c} -> {i}")).collect()
    }

    fn test_score(c: &str, _p: &str) -> f64 {
        (c.len() as f64 / 100.0).min(1.0)
    }

    #[test]
    fn test_tot() {
        let cfg = TotConfig { branching_factor: 2, max_depth: 3, beam_width: 1, score_threshold: 0.95 };
        let r = solve("problem", &cfg, &test_expand, &test_score);
        assert!(r.best_score > 0.0);
    }
}
