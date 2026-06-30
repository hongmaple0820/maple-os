use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryForScoring { pub id: String, pub content: String, pub importance_score: f64, pub created_at: i64, pub last_accessed_at: Option<i64>, pub memory_type: String }

pub fn recency_score(last_accessed: Option<i64>, created_at: i64, now: i64) -> f64 {
    let t = last_accessed.unwrap_or(created_at);
    (-(now - t).max(0) as f64 / 3600.0 / 24.0).exp()
}

pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f64 {
    let len = a.len().min(b.len()); if len == 0 { return 0.0; }
    let (mut dot, mut na, mut nb) = (0.0f64, 0.0f64, 0.0f64);
    for i in 0..len { let (av, bv) = (a[i] as f64, b[i] as f64); dot += av*bv; na += av*av; nb += bv*bv; }
    let d = na.sqrt() * nb.sqrt(); if d == 0.0 { 0.0 } else { (dot/d).max(0.0) }
}

pub fn heuristic_importance(content: &str) -> f64 {
    let mut s = 0.5_f64; s += (content.len() as f64 / 500.0).min(0.2);
    if content.chars().any(|c| c.is_ascii_digit()) { s += 0.15; }
    if content.contains('%') { s += 0.05; }
    let l = content.to_lowercase();
    if l.starts_with("what") || l.starts_with("how") { s -= 0.1; }
    if l.contains("hello") { s -= 0.15; }
    s.clamp(0.1, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn test_recency() { let n = 1000000_i64; assert!((recency_score(None, n, n) - 1.0).abs() < 0.01); }
    #[test] fn test_cosine() { assert!((cosine_similarity(&[1.0, 0.0], &[1.0, 0.0]) - 1.0).abs() < 0.01); }
    #[test] fn test_importance() { assert!(heuristic_importance("hello") < 0.5); assert!(heuristic_importance("Revenue 15% Q3") > 0.6); }
}
