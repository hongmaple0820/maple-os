use anyhow::Result;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReflectionResult { pub agent_id: String, pub insights: Vec<Insight>, pub source_count: usize, pub reflected_at: i64 }
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Insight { pub content: String, pub importance_score: f64, pub memory_id: String }

pub async fn run_reflection(pool: &SqlitePool, agent_id: &str, llm: Option<Box<dyn Fn(&str) -> Result<String> + Send + Sync>>) -> Result<ReflectionResult> {
    let now = chrono::Utc::now().timestamp();
    let cutoff = now - 86400;
    let rows: Vec<(String, String, f64, i64)> = sqlx::query_as("SELECT id, content, importance_score, created_at FROM agent_memories WHERE agent_id = ? AND memory_type = 'episodic' AND created_at >= ? ORDER BY created_at DESC LIMIT 100").bind(agent_id).bind(cutoff).fetch_all(pool).await?;
    let count = rows.len();
    if count == 0 { return Ok(ReflectionResult { agent_id: agent_id.into(), insights: vec![], source_count: 0, reflected_at: now }); }
    let texts: Vec<String> = rows.iter().map(|(_, c, _, _)| c.clone()).collect();
    let insights_text = if let Some(f) = llm.as_ref() { f(&build_prompt(&texts)).unwrap_or_default() } else { texts.iter().take(3).map(|m| format!("- {m}")).collect::<Vec<_>>().join("\n") };
    let mut insights = Vec::new();
    for line in insights_text.lines() {
        let l = line.trim().trim_start_matches(|c: char| c.is_ascii_digit() || c == '.' || c == '-' || c == ' ').to_string();
        if l.len() < 10 { continue; }
        let imp = inline_importance(&l);
        let id = uuid::Uuid::new_v4().to_string();
        let _ = sqlx::query("INSERT INTO agent_memories (id, agent_id, memory_type, content, source_type, relevance_score, importance_score, access_count, created_at, updated_at) VALUES (?, ?, 'semantic', ?, 'manual', ?, ?, 0, ?, ?)").bind(&id).bind(agent_id).bind(&l).bind(imp).bind(imp).bind(now).bind(now).execute(pool).await;
        insights.push(Insight { content: l, importance_score: imp, memory_id: id });
        if insights.len() >= 5 { break; }
    }
    Ok(ReflectionResult { agent_id: agent_id.into(), insights, source_count: count, reflected_at: now })
}

fn build_prompt(memories: &[String]) -> String {
    format!("Synthesize 3-5 insights from:\n{}\n\nInsights (one per line):", memories.iter().take(50).enumerate().map(|(i, m)| format!("{}. {m}", i+1)).collect::<Vec<_>>().join("\n"))
}

fn inline_importance(c: &str) -> f64 {
    let mut s = 0.5_f64; s += (c.len() as f64 / 500.0).min(0.2);
    if c.chars().any(|c| c.is_ascii_digit()) { s += 0.15; }
    if c.contains('%') { s += 0.05; }
    s.clamp(0.1, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn test_prompt() { assert!(build_prompt(&["test".into()]).contains("insights")); }
}
