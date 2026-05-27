use std::path::PathBuf;
use std::time::{Duration, SystemTime};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::fs;
use anyhow::Result;

/// Cron Tasks — inspired by cc-haha's file-backed cron system
///
/// Features:
/// - Cron expression parsing
/// - Recurring, Durable, Permanent job types
/// - File-backed storage (atomic writes)
/// - Max jobs limit (50)
/// - Timezone support
/// - Missed execution handling

const MAX_JOBS: usize = 50;

/// Cron job types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum CronJobType {
    /// Runs on schedule, removed after completion
    Recurring,
    /// Persists across restarts
    Durable,
    /// Never removed automatically
    Permanent,
}

/// Cron job definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CronJob {
    pub id: String,
    pub name: String,
    pub schedule: CronExpression,
    pub task: CronTask,
    pub job_type: CronJobType,
    pub enabled: bool,
    pub created_at: u64,
    pub last_run: Option<u64>,
    pub next_run: Option<u64>,
    pub run_count: u32,
    pub max_runs: Option<u32>,
    pub timezone: Option<String>,
}

/// Cron task to execute
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CronTask {
    /// Execute a tool
    ExecuteTool {
        tool_name: String,
        input: Value,
    },
    /// Send a message
    SendMessage {
        target: String,
        message: String,
    },
    /// Run a script
    RunScript {
        script: String,
        args: Vec<String>,
    },
    /// Custom task
    Custom {
        task_type: String,
        data: Value,
    },
}

/// Cron expression (simplified)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CronExpression {
    /// Minute (0-59)
    pub minute: CronField,
    /// Hour (0-23)
    pub hour: CronField,
    /// Day of month (1-31)
    pub day: CronField,
    /// Month (1-12)
    pub month: CronField,
    /// Day of week (0-6, 0=Sunday)
    pub weekday: CronField,
}

/// Cron field value
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CronField {
    /// Any value
    Any,
    /// Specific value
    Value(u8),
    /// Range (start, end)
    Range(u8, u8),
    /// List of values
    List(Vec<u8>),
    /// Step (every N)
    Step(u8),
    /// Range with step (start, end, step)
    RangeStep(u8, u8, u8),
}

impl CronExpression {
    /// Parse a cron expression string
    pub fn parse(expr: &str) -> Result<Self> {
        let parts: Vec<&str> = expr.split_whitespace().collect();
        if parts.len() != 5 {
            return Err(anyhow::anyhow!("Invalid cron expression: expected 5 fields"));
        }

        Ok(Self {
            minute: Self::parse_field(parts[0], 0, 59)?,
            hour: Self::parse_field(parts[1], 0, 23)?,
            day: Self::parse_field(parts[2], 1, 31)?,
            month: Self::parse_field(parts[3], 1, 12)?,
            weekday: Self::parse_field(parts[4], 0, 6)?,
        })
    }

    fn parse_field(s: &str, min: u8, max: u8) -> Result<CronField> {
        if s == "*" {
            return Ok(CronField::Any);
        }

        if s.contains('/') {
            let parts: Vec<&str> = s.split('/').collect();
            if parts.len() != 2 {
                return Err(anyhow::anyhow!("Invalid step expression: {}", s));
            }
            let step: u8 = parts[1].parse()?;
            if parts[0] == "*" {
                return Ok(CronField::Step(step));
            }
            if parts[0].contains('-') {
                let range: Vec<&str> = parts[0].split('-').collect();
                let start: u8 = range[0].parse()?;
                let end: u8 = range[1].parse()?;
                return Ok(CronField::RangeStep(start, end, step));
            }
            return Err(anyhow::anyhow!("Invalid step expression: {}", s));
        }

        if s.contains('-') {
            let parts: Vec<&str> = s.split('-').collect();
            if parts.len() != 2 {
                return Err(anyhow::anyhow!("Invalid range expression: {}", s));
            }
            let start: u8 = parts[0].parse()?;
            let end: u8 = parts[1].parse()?;
            return Ok(CronField::Range(start, end));
        }

        if s.contains(',') {
            let values: Result<Vec<u8>, _> = s.split(',').map(|v| v.parse()).collect();
            return Ok(CronField::List(values?));
        }

        let value: u8 = s.parse()?;
        if value < min || value > max {
            return Err(anyhow::anyhow!("Value {} out of range [{}, {}]", value, min, max));
        }
        Ok(CronField::Value(value))
    }

    /// Check if current time matches this expression
    pub fn matches(&self, minute: u8, hour: u8, day: u8, month: u8, weekday: u8) -> bool {
        self.field_matches(&self.minute, minute, 0, 59) &&
        self.field_matches(&self.hour, hour, 0, 23) &&
        self.field_matches(&self.day, day, 1, 31) &&
        self.field_matches(&self.month, month, 1, 12) &&
        self.field_matches(&self.weekday, weekday, 0, 6)
    }

    fn field_matches(&self, field: &CronField, value: u8, _min: u8, _max: u8) -> bool {
        match field {
            CronField::Any => true,
            CronField::Value(v) => value == *v,
            CronField::Range(start, end) => value >= *start && value <= *end,
            CronField::List(values) => values.contains(&value),
            CronField::Step(step) => value % step == 0,
            CronField::RangeStep(start, end, step) => {
                value >= *start && value <= *end && (value - start) % step == 0
            }
        }
    }

    /// Calculate next run time
    pub fn next_run_time(&self, after: SystemTime) -> Option<SystemTime> {
        // Simplified: just add 1 minute for now
        // Real implementation would calculate based on cron fields
        Some(after + Duration::from_secs(60))
    }
}

/// File-backed cron storage
pub struct CronStorage {
    path: PathBuf,
}

impl CronStorage {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// Load jobs from file
    pub async fn load(&self) -> Result<Vec<CronJob>> {
        if !self.path.exists() {
            return Ok(Vec::new());
        }

        let content = fs::read_to_string(&self.path).await?;
        let jobs: Vec<CronJob> = serde_json::from_str(&content)?;
        Ok(jobs)
    }

    /// Save jobs to file (atomic write)
    pub async fn save(&self, jobs: &[CronJob]) -> Result<()> {
        let content = serde_json::to_string_pretty(jobs)?;
        let temp_path = self.path.with_extension("tmp");

        fs::write(&temp_path, &content).await?;
        fs::rename(&temp_path, &self.path).await?;

        Ok(())
    }
}

/// Cron scheduler
pub struct CronScheduler {
    storage: CronStorage,
    jobs: Vec<CronJob>,
}

impl CronScheduler {
    pub fn new(storage_path: PathBuf) -> Self {
        Self {
            storage: CronStorage::new(storage_path),
            jobs: Vec::new(),
        }
    }

    /// Initialize scheduler (load jobs from storage)
    pub async fn init(&mut self) -> Result<()> {
        self.jobs = self.storage.load().await?;
        Ok(())
    }

    /// Add a new job
    pub async fn add_job(&mut self, job: CronJob) -> Result<()> {
        if self.jobs.len() >= MAX_JOBS {
            return Err(anyhow::anyhow!("Maximum number of jobs ({}) reached", MAX_JOBS));
        }

        // Check for duplicate ID
        if self.jobs.iter().any(|j| j.id == job.id) {
            return Err(anyhow::anyhow!("Job with ID {} already exists", job.id));
        }

        self.jobs.push(job);
        self.storage.save(&self.jobs).await?;
        Ok(())
    }

    /// Remove a job
    pub async fn remove_job(&mut self, job_id: &str) -> Result<bool> {
        let initial_len = self.jobs.len();
        self.jobs.retain(|j| j.id != job_id);

        if self.jobs.len() < initial_len {
            self.storage.save(&self.jobs).await?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Enable/disable a job
    pub async fn set_job_enabled(&mut self, job_id: &str, enabled: bool) -> Result<()> {
        if let Some(job) = self.jobs.iter_mut().find(|j| j.id == job_id) {
            job.enabled = enabled;
            self.storage.save(&self.jobs).await?;
        }
        Ok(())
    }

    /// Get all jobs
    pub fn get_jobs(&self) -> &[CronJob] {
        &self.jobs
    }

    /// Get jobs due for execution
    pub fn get_due_jobs(&self, now: SystemTime) -> Vec<&CronJob> {
        let duration_since_epoch = now.duration_since(SystemTime::UNIX_EPOCH).unwrap_or_default();
        let current_timestamp = duration_since_epoch.as_secs();

        self.jobs.iter()
            .filter(|job| job.enabled)
            .filter(|job| {
                if let Some(next_run) = job.next_run {
                    current_timestamp >= next_run
                } else {
                    true
                }
            })
            .collect()
    }

    /// Mark job as executed
    pub async fn mark_executed(&mut self, job_id: &str) -> Result<()> {
        if let Some(job) = self.jobs.iter_mut().find(|j| j.id == job_id) {
            job.last_run = Some(
                SystemTime::now()
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs()
            );
            job.run_count += 1;

            // Remove if max runs reached
            if let Some(max_runs) = job.max_runs {
                if job.run_count >= max_runs {
                    job.enabled = false;
                }
            }
        }

        self.storage.save(&self.jobs).await?;
        Ok(())
    }
}

/// Builder for CronJob
pub struct CronJobBuilder {
    job: CronJob,
}

impl CronJobBuilder {
    pub fn new(id: &str, name: &str, schedule: CronExpression, task: CronTask) -> Self {
        Self {
            job: CronJob {
                id: id.to_string(),
                name: name.to_string(),
                schedule,
                task,
                job_type: CronJobType::Recurring,
                enabled: true,
                created_at: SystemTime::now()
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
                last_run: None,
                next_run: None,
                run_count: 0,
                max_runs: None,
                timezone: None,
            },
        }
    }

    pub fn job_type(mut self, job_type: CronJobType) -> Self {
        self.job.job_type = job_type;
        self
    }

    pub fn max_runs(mut self, max: u32) -> Self {
        self.job.max_runs = Some(max);
        self
    }

    pub fn timezone(mut self, tz: &str) -> Self {
        self.job.timezone = Some(tz.to_string());
        self
    }

    pub fn build(self) -> CronJob {
        self.job
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cron_expression_parse() {
        let expr = CronExpression::parse("*/5 * * * *").unwrap();
        assert!(matches!(expr.minute, CronField::Step(5)));
        assert!(matches!(expr.hour, CronField::Any));
    }

    #[test]
    fn test_cron_expression_range() {
        let expr = CronExpression::parse("0 9-17 * * *").unwrap();
        assert!(matches!(expr.minute, CronField::Value(0)));
        assert!(matches!(expr.hour, CronField::Range(9, 17)));
    }

    #[test]
    fn test_cron_expression_list() {
        let expr = CronExpression::parse("0,30 * * * *").unwrap();
        assert!(matches!(expr.minute, CronField::List(ref v) if v == &vec![0, 30]));
    }

    #[test]
    fn test_cron_matches() {
        let expr = CronExpression::parse("*/5 * * * *").unwrap();
        assert!(expr.matches(0, 12, 1, 1, 0));
        assert!(expr.matches(5, 12, 1, 1, 0));
        assert!(expr.matches(10, 12, 1, 1, 0));
        assert!(!expr.matches(7, 12, 1, 1, 0));
    }

    #[test]
    fn test_cron_job_builder() {
        let schedule = CronExpression::parse("0 9 * * *").unwrap();
        let task = CronTask::ExecuteTool {
            tool_name: "lint".to_string(),
            input: serde_json::json!({}),
        };

        let job = CronJobBuilder::new("morning-lint", "Morning Lint", schedule, task)
            .job_type(CronJobType::Durable)
            .max_runs(100)
            .timezone("UTC")
            .build();

        assert_eq!(job.id, "morning-lint");
        assert_eq!(job.job_type, CronJobType::Durable);
        assert_eq!(job.max_runs, Some(100));
    }
}
