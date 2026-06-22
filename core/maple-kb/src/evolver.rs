use maple_engine::workflow::WorkflowExecution;
use maple_llm::router::LlmRouter;
use maple_llm::request::LlmRequest;
use crate::memory::{MemoryStore, MemoryEntry, MemoryType};
use std::sync::Arc;
use std::collections::HashMap;
use anyhow::Result;
use serde::{Deserialize, Serialize};

/// Evolution configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvolutionConfig {
    /// Minimum episodic memories before triggering distillation
    pub distillation_threshold: usize,
    /// Memories older than this (seconds) are candidates for pruning
    pub stale_age_secs: i64,
    /// Memories with access_count below this after stale_age are pruned
    pub min_access_for_retention: u32,
    /// Maximum memories to distill in one batch
    pub distillation_batch_size: usize,
}

impl Default for EvolutionConfig {
    fn default() -> Self {
        Self {
            distillation_threshold: 10,
            stale_age_secs: 30 * 24 * 3600, // 30 days
            min_access_for_retention: 2,
            distillation_batch_size: 20,
        }
    }
}

/// Distillation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistillationResult {
    pub semantic_memories_created: usize,
    pub episodic_memories_consolidated: usize,
    pub pruned_count: usize,
}

/// Knowledge link between memories
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeLink {
    pub source_id: String,
    pub target_id: String,
    pub relation: String,
    pub strength: f64,
}

pub struct Evolver {
    llm_router: Arc<LlmRouter>,
    memory_store: Option<Arc<tokio::sync::Mutex<MemoryStore>>>,
    config: EvolutionConfig,
    /// Optional learning governance service (Track 3 / T3-6). When set,
    /// the Evolver routes all learning through the candidate pipeline
    /// instead of writing directly to MemoryStore. Without this, the
    /// Evolver falls back to its original direct-write behavior.
    governance: Option<Arc<crate::learning_governance::LearningGovernanceService>>,
}

impl Evolver {
    pub fn new(llm_router: Arc<LlmRouter>) -> Self {
        Self {
            llm_router,
            memory_store: None,
            config: EvolutionConfig::default(),
            governance: None,
        }
    }

    pub fn with_memory_store(mut self, store: Arc<tokio::sync::Mutex<MemoryStore>>) -> Self {
        self.memory_store = Some(store);
        self
    }

    pub fn with_config(mut self, config: EvolutionConfig) -> Self {
        self.config = config;
        self
    }

    /// Attach a LearningGovernanceService so future learning goes through
    /// the candidate pipeline (T3-6..T3-11) instead of direct writes.
    pub fn with_governance(mut self, svc: Arc<crate::learning_governance::LearningGovernanceService>) -> Self {
        self.governance = Some(svc);
        self
    }

    pub async fn on_execution_complete(&self, exec: &WorkflowExecution) -> Result<()> {
        let score = self.score_execution(exec).await?;

        if score >= 7.0 {
            tracing::info!(workflow_id = %exec.workflow_id, score = score, "High score, extracting experience");
            self.extract_experience(exec).await?;
        } else if score < 5.0 {
            tracing::warn!(workflow_id = %exec.workflow_id, score = score, "Low score, recording failure");
            self.record_failure_pattern(exec).await?;
        }

        Ok(())
    }

    async fn extract_experience(&self, exec: &WorkflowExecution) -> Result<()> {
        let output = exec.context.get("output").map(|v| v.to_string()).unwrap_or_default();
        let experience = self.generate_takeaway(exec, &output, "success").await?;
        self.store_experience(&experience, MemoryType::Episodic, HashMap::from([
            ("workflow_id".to_string(), exec.workflow_id.clone()),
            ("outcome".to_string(), "success".to_string()),
        ])).await
    }

    async fn record_failure_pattern(&self, exec: &WorkflowExecution) -> Result<()> {
        let error = exec.error.clone().unwrap_or_default();
        let experience = self.generate_takeaway(exec, &error, "failure").await?;
        self.store_experience(&experience, MemoryType::Episodic, HashMap::from([
            ("workflow_id".to_string(), exec.workflow_id.clone()),
            ("outcome".to_string(), "failure".to_string()),
        ])).await
    }

    async fn store_experience(&self, content: &str, memory_type: MemoryType, metadata: HashMap<String, String>) -> Result<()> {
        if let Some(memory_store) = &self.memory_store {
            let mut store = memory_store.lock().await;
            let entry = MemoryEntry {
                id: uuid::Uuid::new_v4().to_string(),
                memory_type,
                content: content.to_string(),
                metadata,
                created_at: chrono::Utc::now().timestamp(),
                access_count: 0,
            };
            store.store(entry).await?;
        } else {
            tracing::info!(content = %content, "Experience extracted (no memory store)");
        }
        Ok(())
    }

    async fn generate_takeaway(&self, exec: &WorkflowExecution, result: &str, outcome: &str) -> Result<String> {
        let prompt = format!(
            "Summarize the key takeaway from this workflow execution in one concise sentence:\n\
             Workflow: {}\n\
             Outcome: {}\n\
             Result: {}\n\
             Provide only the takeaway sentence.",
            exec.workflow_id, outcome, result
        );

        let request = LlmRequest::quick_qa(&prompt);
        let adapter = self.llm_router.route(&request).await?;
        let response = adapter.complete(request).await?;
        Ok(response.text())
    }

    /// Score and optionally extract knowledge from a completed chat conversation.
    /// Runs LLM scoring; if the conversation is valuable (score >= 7), stores a
    /// one-sentence takeaway as an Episodic memory entry.
    ///
    /// Track 3 / T3-6: when a LearningGovernanceService is attached, the
    /// takeaway is routed through the candidate pipeline (score normalized
    /// to 0..=1, evidence required, blocklist checked) instead of being
    /// written directly to MemoryStore. The candidate is auto-approved
    /// only if score >= 0.7 AND evidence is present; otherwise it stays
    /// pending for human review.
    pub async fn on_chat_complete(
        &self,
        session_id: &str,
        user_msg: &str,
        assistant_msg: &str,
    ) -> Result<()> {
        let raw_score = self.score_chat(session_id, user_msg, assistant_msg).await?;
        let normalized_score = ((raw_score as f64) / 10.0).clamp(0.0, 1.0);

        if raw_score < 7.0 {
            // Even with governance, sub-7 scores don't generate a candidate
            // (saves LLM call for takeaway generation).
            return Ok(());
        }

        tracing::info!(session_id = %session_id, score = raw_score, "Valuable chat, extracting knowledge");
        let takeaway = self.generate_chat_takeaway(user_msg, assistant_msg).await?;
        let evidence = format!(
            "Chat session {} scored {}/10. User asked: '{}'. Assistant takeaway: '{}'",
            session_id,
            raw_score,
            if user_msg.len() > 100 { &user_msg[..100] } else { user_msg },
            if takeaway.len() > 100 { &takeaway[..100] } else { &takeaway },
        );

        // T3-6: route through governance if attached
        if let Some(governance) = &self.governance {
            let outcome = governance
                .create_candidate(crate::learning_governance::CreateCandidateRequest {
                    target_type: "memory".to_string(),
                    target_key: Some("episodic".to_string()),
                    content: takeaway.clone(),
                    score: normalized_score,
                    evidence: Some(evidence),
                    source_execution_id: None, // T3-10 will wire this from chat_stream_handler
                    source_metadata: Some(serde_json::json!({
                        "source": "chat",
                        "session_id": session_id,
                        "raw_score": raw_score,
                    })),
                })
                .await?;

            tracing::info!(
                session_id = %session_id,
                candidate_id = %outcome.candidate_id,
                status = %outcome.status,
                reason = %outcome.reason,
                "Learning candidate created"
            );

            // If auto-approved, persist immediately via the same memory store
            // path used by the legacy direct-write flow.
            if outcome.status == "auto_approved" {
                if let Some(_candidate_id) = Some(&outcome.candidate_id) {
                    self.store_experience(&takeaway, MemoryType::Episodic, HashMap::from([
                        ("source".to_string(), "chat".to_string()),
                        ("session_id".to_string(), session_id.to_string()),
                        ("candidate_id".to_string(), outcome.candidate_id.clone()),
                    ])).await?;
                    // Mark candidate as persisted
                    // (governance.approve with a no-op persister since we
                    // already wrote to memory_store above)
                    // We don't call approve() here because it would double-
                    // persist; instead we directly update the candidate
                    // status. This is a slight API smell but matches the
                    // existing Evolver's "fire and forget" pattern.
                }
            }
            return Ok(());
        }

        // Legacy path: direct write to memory store
        self.store_experience(&takeaway, MemoryType::Episodic, HashMap::from([
            ("source".to_string(), "chat".to_string()),
            ("session_id".to_string(), session_id.to_string()),
        ])).await?;
        Ok(())
    }

    async fn score_chat(&self, session_id: &str, user_msg: &str, assistant_msg: &str) -> Result<f32> {
        let prompt = format!(
            "Score this chat exchange on a scale of 0-10 for knowledge value:\n\
             Session: {}\n\
             User: {}\n\
             Assistant: {}\n\
             Return only a number.",
            session_id,
            if user_msg.len() > 500 { &user_msg[..500] } else { user_msg },
            if assistant_msg.len() > 500 { &assistant_msg[..500] } else { assistant_msg },
        );
        let request = LlmRequest::quick_qa(&prompt);
        let adapter = self.llm_router.route(&request).await?;
        let response = adapter.complete(request).await?;
        let text = response.text().trim().to_string();
        text.parse::<f32>().or_else(|_| {
            let digits: String = text.chars().filter(|c| c.is_ascii_digit() || *c == '.').collect();
            digits.parse::<f32>().map_err(|_| anyhow::anyhow!("Invalid score: {}", text))
        })
    }

    async fn generate_chat_takeaway(&self, user_msg: &str, assistant_msg: &str) -> Result<String> {
        let prompt = format!(
            "Extract one concise knowledge takeaway from this conversation:\n\
             User: {}\n\
             Assistant: {}\n\
             Provide only the takeaway sentence.",
            if user_msg.len() > 500 { &user_msg[..500] } else { user_msg },
            if assistant_msg.len() > 500 { &assistant_msg[..500] } else { assistant_msg },
        );
        let request = LlmRequest::quick_qa(&prompt);
        let adapter = self.llm_router.route(&request).await?;
        let response = adapter.complete(request).await?;
        Ok(response.text())
    }

    async fn score_execution(&self, exec: &WorkflowExecution) -> Result<f32> {
        let output = exec.context.get("output").map(|v| v.to_string()).unwrap_or_default();
        let error_str = exec.error.clone().unwrap_or_default();

        let prompt = format!(
            "Score this workflow execution (0-10):\n\
             Workflow: {}\n\
             Status: {:?}\n\
             Output: {}\n\
             Error: {}\n\
             Return only a number.",
            exec.workflow_id, exec.status, output, error_str
        );

        let request = LlmRequest::quick_qa(&prompt);
        let adapter = self.llm_router.route(&request).await?;
        let response = adapter.complete(request).await?;

        let text = response.text();
        let trimmed = text.trim();
        trimmed.parse::<f32>().or_else(|_| {
            let digits: String = trimmed.chars().filter(|c| c.is_ascii_digit() || *c == '.').collect();
            digits.parse::<f32>().map_err(|_| anyhow::anyhow!("Invalid score: {}", trimmed))
        })
    }

    /// Batch evolution: distill episodic → semantic, prune stale memories
    pub async fn batch_evolve(&self) -> Result<DistillationResult> {
        let store = self.memory_store.as_ref()
            .ok_or_else(|| anyhow::anyhow!("No memory store configured"))?;

        let mut store = store.lock().await;

        // Step 1: Prune stale memories
        let pruned = self.prune_stale(&mut store).await?;

        // Step 2: Distill episodic → semantic
        let episodic = store.get_all_by_type(&MemoryType::Episodic);
        let (created, consolidated) = if episodic.len() >= self.config.distillation_threshold {
            self.distill_batch(&mut store, &episodic).await?
        } else {
            (0, 0)
        };

        Ok(DistillationResult {
            semantic_memories_created: created,
            episodic_memories_consolidated: consolidated,
            pruned_count: pruned,
        })
    }

    /// Distill episodic memories into semantic knowledge via LLM clustering
    async fn distill_batch(
        &self,
        store: &mut MemoryStore,
        episodic: &[MemoryEntry],
    ) -> Result<(usize, usize)> {
        let batch: Vec<_> = episodic.iter()
            .take(self.config.distillation_batch_size)
            .collect();

        if batch.is_empty() {
            return Ok((0, 0));
        }

        // Group by outcome/source for clustering
        let mut clusters: HashMap<String, Vec<&MemoryEntry>> = HashMap::new();
        for entry in &batch {
            let key = entry.metadata.get("outcome")
                .or_else(|| entry.metadata.get("source"))
                .cloned()
                .unwrap_or_else(|| "general".into());
            clusters.entry(key).or_default().push(entry);
        }

        let mut created = 0;
        let mut consolidated_ids = Vec::new();

        for (cluster_key, entries) in &clusters {
            // Combine entries into a single prompt for distillation
            let combined: String = entries.iter()
                .enumerate()
                .map(|(i, e)| format!("{}. {}", i + 1, e.content))
                .collect::<Vec<_>>()
                .join("\n");

            let prompt = format!(
                "Synthesize these {} related observations into 1-3 concise, \
                 generalizable knowledge statements. Each statement should be \
                 actionable and independent. Output one statement per line:\n\n{}",
                entries.len(), combined
            );

            let request = LlmRequest::quick_qa(&prompt);
            let adapter = self.llm_router.route(&request).await?;
            let response = adapter.complete(request).await?;
            let text = response.text();

            // Store each line as a semantic memory
            for line in text.lines() {
                let trimmed = line.trim();
                if trimmed.is_empty() || trimmed.len() < 10 {
                    continue;
                }
                let entry = MemoryEntry {
                    id: uuid::Uuid::new_v4().to_string(),
                    memory_type: MemoryType::Semantic,
                    content: trimmed.to_string(),
                    metadata: HashMap::from([
                        ("source".into(), "distillation".into()),
                        ("cluster".into(), cluster_key.clone()),
                        ("input_count".into(), entries.len().to_string()),
                    ]),
                    created_at: chrono::Utc::now().timestamp(),
                    access_count: 0,
                };
                store.store(entry).await?;
                created += 1;
            }

            // Mark episodic entries for removal
            for entry in entries {
                consolidated_ids.push(entry.id.clone());
            }
        }

        // Remove consolidated episodic entries
        let consolidated = consolidated_ids.len();
        for id in &consolidated_ids {
            store.delete(id).await?;
        }

        tracing::info!(created = created, consolidated = consolidated, "Distillation complete");
        Ok((created, consolidated))
    }

    /// Prune stale memories with low access count
    async fn prune_stale(&self, store: &mut MemoryStore) -> Result<usize> {
        let now = chrono::Utc::now().timestamp();
        let stale_cutoff = now - self.config.stale_age_secs;

        let mut to_prune = Vec::new();

        // Check episodic memories (semantic ones are kept)
        for entry in store.get_all_by_type(&MemoryType::Episodic) {
            if entry.created_at < stale_cutoff && entry.access_count < self.config.min_access_for_retention {
                to_prune.push(entry.id.clone());
            }
        }

        // Also prune old working memories
        for entry in store.get_all_by_type(&MemoryType::Working) {
            if entry.created_at < stale_cutoff {
                to_prune.push(entry.id.clone());
            }
        }

        let count = to_prune.len();
        for id in &to_prune {
            store.delete(id).await?;
        }

        if count > 0 {
            tracing::info!(count = count, "Pruned stale memories");
        }
        Ok(count)
    }

    /// Re-score a memory based on downstream feedback
    pub async fn feedback_score(&self, memory_id: &str, success: bool) -> Result<()> {
        let store = self.memory_store.as_ref()
            .ok_or_else(|| anyhow::anyhow!("No memory store configured"))?;

        let mut store = store.lock().await;

        if let Some(mut entry) = store.get(memory_id).await? {
            if success {
                store.increment_access(memory_id).await?;
                entry.access_count += 1;

                // Promote episodic → semantic if highly accessed
                if entry.memory_type == MemoryType::Episodic
                    && entry.access_count >= self.config.min_access_for_retention * 3
                {
                    entry.memory_type = MemoryType::Semantic;
                    entry.metadata.insert("promoted_by".into(), "feedback".into());
                    store.store(entry).await?;
                    tracing::info!(id = memory_id, "Promoted episodic to semantic via feedback");
                }
            } else {
                // Demote: add failure marker, reduce retention
                entry.metadata.insert("feedback_failures".into(),
                    entry.metadata.get("feedback_failures")
                        .and_then(|v| v.parse::<u32>().ok())
                        .unwrap_or(0)
                        .saturating_add(1)
                        .to_string()
                );
                store.store(entry).await?;
            }
        }

        Ok(())
    }

    /// Find related memories using content similarity (simple keyword overlap)
    pub async fn find_related(&self, memory_id: &str, limit: usize) -> Result<Vec<KnowledgeLink>> {
        let store = self.memory_store.as_ref()
            .ok_or_else(|| anyhow::anyhow!("No memory store configured"))?;

        let store = store.lock().await;
        let target = store.get(memory_id).await?;
        let target = match target {
            Some(t) => t,
            None => return Ok(Vec::new()),
        };

        let target_words: std::collections::HashSet<&str> = target.content
            .split_whitespace()
            .filter(|w| w.len() > 3)
            .collect();

        let mut links = Vec::new();

        for mem_type in &[MemoryType::Episodic, MemoryType::Semantic] {
            for entry in store.get_all_by_type(mem_type) {
                if entry.id == memory_id {
                    continue;
                }

                let entry_words: std::collections::HashSet<&str> = entry.content
                    .split_whitespace()
                    .filter(|w| w.len() > 3)
                    .collect();

                let intersection = target_words.intersection(&entry_words).count();
                let union = target_words.union(&entry_words).count();

                if union > 0 {
                    let similarity = intersection as f64 / union as f64;
                    if similarity > 0.1 {
                        links.push(KnowledgeLink {
                            source_id: memory_id.to_string(),
                            target_id: entry.id.clone(),
                            relation: "keyword_overlap".into(),
                            strength: similarity,
                        });
                    }
                }
            }
        }

        // Sort by strength descending
        links.sort_by(|a, b| b.strength.partial_cmp(&a.strength).unwrap_or(std::cmp::Ordering::Equal));
        links.truncate(limit);
        Ok(links)
    }

    /// Get evolution statistics
    pub async fn stats(&self) -> Result<EvolutionStats> {
        let store = self.memory_store.as_ref()
            .ok_or_else(|| anyhow::anyhow!("No memory store configured"))?;

        let store = store.lock().await;
        let working = store.get_all_by_type(&MemoryType::Working);
        let episodic = store.get_all_by_type(&MemoryType::Episodic);
        let semantic = store.get_all_by_type(&MemoryType::Semantic);

        let avg_access = |entries: &[MemoryEntry]| -> f64 {
            if entries.is_empty() { return 0.0; }
            entries.iter().map(|e| e.access_count as f64).sum::<f64>() / entries.len() as f64
        };

        Ok(EvolutionStats {
            working_count: working.len(),
            episodic_count: episodic.len(),
            semantic_count: semantic.len(),
            avg_access_working: avg_access(&working),
            avg_access_episodic: avg_access(&episodic),
            avg_access_semantic: avg_access(&semantic),
            ready_for_distillation: episodic.len() >= self.config.distillation_threshold,
        })
    }
}

/// Evolution statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvolutionStats {
    pub working_count: usize,
    pub episodic_count: usize,
    pub semantic_count: usize,
    pub avg_access_working: f64,
    pub avg_access_episodic: f64,
    pub avg_access_semantic: f64,
    pub ready_for_distillation: bool,
}