use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};
use tokio::fs;

/// Cron Tasks — inspired by cc-haha's file-backed cron system
///
/// Features:
/// - Cron expression parsing
/// - Recurring, Durable, Permanent job types
/// - File-backed storage (atomic writes)
/// - Max jobs limit (50)
/// - Timezone support
/// - Missed execution handling
///
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
    ExecuteTool { tool_name: String, input: Value },
    /// Send a message
    SendMessage { target: String, message: String },
    /// Run a script
    RunScript { script: String, args: Vec<String> },
    /// Custom task
    Custom { task_type: String, data: Value },
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
            return Err(anyhow::anyhow!(
                "Invalid cron expression: expected 5 fields"
            ));
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
            return Err(anyhow::anyhow!(
                "Value {} out of range [{}, {}]",
                value,
                min,
                max
            ));
        }
        Ok(CronField::Value(value))
    }

    /// Parse natural language description into a cron expression
    ///
    /// Supports:
    /// - "every N minutes/hours/days"
    /// - "daily at HH:MM"
    /// - "weekly on Monday at HH:MM"
    /// - "monthly on day N at HH:MM"
    /// - "hourly", "daily", "weekly", "monthly"
    pub fn parse_natural_language(input: &str) -> Result<Self> {
        let input = input.trim().to_lowercase();

        // "every N minutes"
        if let Some(caps) = input.strip_prefix("every ") {
            let parts: Vec<&str> = caps.split_whitespace().collect();
            if parts.len() >= 2
                && let Ok(n) = parts[0].parse::<u8>()
            {
                match parts[1] {
                    "minute" | "minutes" | "min" | "mins" => {
                        return Ok(Self {
                            minute: CronField::Step(n),
                            hour: CronField::Any,
                            day: CronField::Any,
                            month: CronField::Any,
                            weekday: CronField::Any,
                        });
                    }
                    "hour" | "hours" => {
                        return Ok(Self {
                            minute: CronField::Value(0),
                            hour: if n == 1 { CronField::Any } else { CronField::Step(n) },
                            day: CronField::Any,
                            month: CronField::Any,
                            weekday: CronField::Any,
                        });
                    }
                    "day" | "days" => {
                        return Ok(Self {
                            minute: CronField::Value(0),
                            hour: CronField::Value(0),
                            day: if n == 1 { CronField::Any } else { CronField::Step(n) },
                            month: CronField::Any,
                            weekday: CronField::Any,
                        });
                    }
                    _ => {}
                }
            }
        }

        // "daily at HH:MM"
        if let Some(time_str) = input
            .strip_prefix("daily at ")
            .or_else(|| input.strip_prefix("every day at "))
            && let Some((h, m)) = parse_time(time_str)
        {
            return Ok(Self {
                minute: CronField::Value(m),
                hour: CronField::Value(h),
                day: CronField::Any,
                month: CronField::Any,
                weekday: CronField::Any,
            });
        }

        // "hourly"
        if input == "hourly" || input == "every hour" {
            return Ok(Self {
                minute: CronField::Value(0),
                hour: CronField::Any,
                day: CronField::Any,
                month: CronField::Any,
                weekday: CronField::Any,
            });
        }

        // "daily"
        if input == "daily" || input == "every day" {
            return Ok(Self {
                minute: CronField::Value(0),
                hour: CronField::Value(0),
                day: CronField::Any,
                month: CronField::Any,
                weekday: CronField::Any,
            });
        }

        // "weekly on Monday at HH:MM"
        if let Some(rest) = input.strip_prefix("weekly on ") {
            let days = [
                ("sunday", 0), ("monday", 1), ("tuesday", 2), ("wednesday", 3),
                ("thursday", 4), ("friday", 5), ("saturday", 6),
            ];
            for (day_name, day_num) in &days {
                if let Some(after_day) = rest.strip_prefix(day_name) {
                    let time_part = after_day.trim();
                    let (h, m) = if let Some(time_str) = time_part.strip_prefix("at ") {
                        parse_time(time_str).unwrap_or((0, 0))
                    } else {
                        (0, 0)
                    };
                    return Ok(Self {
                        minute: CronField::Value(m),
                        hour: CronField::Value(h),
                        day: CronField::Any,
                        month: CronField::Any,
                        weekday: CronField::Value(*day_num),
                    });
                }
            }
        }

        // "monthly on day N at HH:MM"
        if let Some(rest) = input.strip_prefix("monthly on day ") {
            let parts: Vec<&str> = rest.split_whitespace().collect();
            if let Ok(day) = parts[0].parse::<u8>() {
                let (h, m) = if parts.len() > 2 && parts[1] == "at" {
                    parse_time(parts[2]).unwrap_or((0, 0))
                } else {
                    (0, 0)
                };
                return Ok(Self {
                    minute: CronField::Value(m),
                    hour: CronField::Value(h),
                    day: CronField::Value(day),
                    month: CronField::Any,
                    weekday: CronField::Any,
                });
            }
        }

        // "weekly"
        if input == "weekly" || input == "every week" {
            return Ok(Self {
                minute: CronField::Value(0),
                hour: CronField::Value(0),
                day: CronField::Any,
                month: CronField::Any,
                weekday: CronField::Value(1), // Monday
            });
        }

        // "monthly"
        if input == "monthly" || input == "every month" {
            return Ok(Self {
                minute: CronField::Value(0),
                hour: CronField::Value(0),
                day: CronField::Value(1),
                month: CronField::Any,
                weekday: CronField::Any,
            });
        }

        Err(anyhow::anyhow!(
            "Could not parse natural language cron: '{}'. Try 'every 5 minutes', 'daily at 9:00', 'weekly on Monday at 14:00'",
            input
        ))
    }

    /// Check if current time matches this expression
    pub fn matches(&self, minute: u8, hour: u8, day: u8, month: u8, weekday: u8) -> bool {
        self.field_matches(&self.minute, minute, 0, 59)
            && self.field_matches(&self.hour, hour, 0, 23)
            && self.field_matches(&self.day, day, 1, 31)
            && self.field_matches(&self.month, month, 1, 12)
            && self.field_matches(&self.weekday, weekday, 0, 6)
    }

    fn field_matches(&self, field: &CronField, value: u8, _min: u8, _max: u8) -> bool {
        match field {
            CronField::Any => true,
            CronField::Value(v) => value == *v,
            CronField::Range(start, end) => value >= *start && value <= *end,
            CronField::List(values) => values.contains(&value),
            CronField::Step(step) => value.is_multiple_of(*step),
            CronField::RangeStep(start, end, step) => {
                (*start..=*end).contains(&value) && (value - start).is_multiple_of(*step)
            }
        }
    }

    /// Calculate next run time based on cron fields
    /// Searches forward from `after` up to 366 days to find the next matching minute.
    pub fn next_run_time(&self, after: SystemTime) -> Option<SystemTime> {
        // Start from the next minute boundary
        let after_secs = after.duration_since(SystemTime::UNIX_EPOCH).ok()?.as_secs();
        let start_minute = (after_secs / 60) + 1; // next minute boundary

        // Search up to 366 days worth of minutes
        let max_minutes = 366 * 24 * 60;

        for offset in 0..max_minutes {
            let candidate_secs = (start_minute + offset) * 60;
            let (minute, hour, day, month, weekday) = Self::timestamp_to_fields(candidate_secs);

            if self.matches(minute, hour, day, month, weekday) {
                return Some(SystemTime::UNIX_EPOCH + Duration::from_secs(candidate_secs));
            }
        }

        None
    }

    /// Convert a Unix timestamp (seconds) to cron field values (UTC)
    /// Returns (minute, hour, day, month, weekday)
    fn timestamp_to_fields(secs: u64) -> (u8, u8, u8, u8, u8) {
        let minutes_total = secs / 60;
        let minute = (minutes_total % 60) as u8;

        let hours_total = minutes_total / 60;
        let hour = (hours_total % 24) as u8;

        let days_total = hours_total / 24;

        // Calculate date from days since Unix epoch (1970-01-01 = day 0)
        // Using a simplified algorithm for date calculation
        let (_year, month, day) = Self::days_to_ymd(days_total);

        // Calculate day of week (1970-01-01 was a Thursday = 4)
        let weekday = ((days_total + 4) % 7) as u8;

        (minute, hour, day, month, weekday)
    }

    /// Convert days since Unix epoch to (year, month, day)
    fn days_to_ymd(days: u64) -> (u16, u8, u8) {
        // Simplified civil calendar calculation
        // Based on Howard Hinnant's algorithm
        let z = days + 719468;
        let era = z / 146097;
        let doe = z - era * 146097;
        let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
        let y = yoe as u16 + era as u16 * 400;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = (doy - (153 * mp + 2) / 5 + 1) as u8;
        let m = if mp < 10 { mp + 3 } else { mp - 9 } as u8;
        let y = if m <= 2 { y + 1 } else { y };

        (y, m, d)
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
            return Err(anyhow::anyhow!(
                "Maximum number of jobs ({}) reached",
                MAX_JOBS
            ));
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
        let duration_since_epoch = now
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default();
        let current_timestamp = duration_since_epoch.as_secs();

        self.jobs
            .iter()
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
                    .as_secs(),
            );
            job.run_count += 1;

            // Remove if max runs reached
            if let Some(max_runs) = job.max_runs
                && job.run_count >= max_runs
            {
                job.enabled = false;
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

/// Parse time string like "9:00", "14:30", "9am", "2pm"
fn parse_time(s: &str) -> Option<(u8, u8)> {
    let s = s.trim().to_lowercase();

    // Try "HH:MM" format
    if let Some((h, m)) = s.split_once(':')
        && let (Ok(h), Ok(m)) = (h.parse::<u8>(), m.parse::<u8>())
        && h <= 23
        && m <= 59
    {
        return Some((h, m));
    }

    // Try "Ham" or "Hpm" format
    if let Some(h_str) = s.strip_suffix("am")
        && let Ok(h) = h_str.parse::<u8>()
        && (1..=12).contains(&h)
    {
        return Some((if h == 12 { 0 } else { h }, 0));
    }
    if let Some(h_str) = s.strip_suffix("pm")
        && let Ok(h) = h_str.parse::<u8>()
        && (1..=12).contains(&h)
    {
        return Some((if h == 12 { 12 } else { h + 12 }, 0));
    }

    None
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

    #[test]
    fn test_next_run_time_every_5_minutes() {
        let expr = CronExpression::parse("*/5 * * * *").unwrap();

        // At 12:03:00, next run should be 12:05:00
        let after = SystemTime::UNIX_EPOCH + Duration::from_secs(12 * 3600 + 3 * 60);
        let next = expr.next_run_time(after).unwrap();
        let next_secs = next
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        assert_eq!(next_secs, 12 * 3600 + 5 * 60);
    }

    #[test]
    fn test_next_run_time_specific_hour() {
        let expr = CronExpression::parse("0 9 * * *").unwrap();

        // At 08:00, next run should be 09:00
        let after = SystemTime::UNIX_EPOCH + Duration::from_secs(8 * 3600);
        let next = expr.next_run_time(after).unwrap();
        let next_secs = next
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        assert_eq!(next_secs, 9 * 3600);
    }

    #[test]
    fn test_next_run_time_after_target_hour() {
        let expr = CronExpression::parse("0 9 * * *").unwrap();

        // At 09:30, next run should be next day 09:00
        let after = SystemTime::UNIX_EPOCH + Duration::from_secs(9 * 3600 + 30 * 60);
        let next = expr.next_run_time(after).unwrap();
        let next_secs = next
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        assert_eq!(next_secs, 24 * 3600 + 9 * 3600);
    }

    #[test]
    fn test_parse_time() {
        assert_eq!(parse_time("9:00"), Some((9, 0)));
        assert_eq!(parse_time("14:30"), Some((14, 30)));
        assert_eq!(parse_time("9am"), Some((9, 0)));
        assert_eq!(parse_time("2pm"), Some((14, 0)));
        assert_eq!(parse_time("12pm"), Some((12, 0)));
        assert_eq!(parse_time("12am"), Some((0, 0)));
        assert_eq!(parse_time("invalid"), None);
    }

    #[test]
    fn test_natural_language_every_minutes() {
        let expr = CronExpression::parse_natural_language("every 5 minutes").unwrap();
        assert!(matches!(expr.minute, CronField::Step(5)));
        assert!(matches!(expr.hour, CronField::Any));
    }

    #[test]
    fn test_natural_language_every_hours() {
        let expr = CronExpression::parse_natural_language("every 2 hours").unwrap();
        assert!(matches!(expr.minute, CronField::Value(0)));
        assert!(matches!(expr.hour, CronField::Step(2)));
    }

    #[test]
    fn test_natural_language_daily_at() {
        let expr = CronExpression::parse_natural_language("daily at 9:00").unwrap();
        assert!(matches!(expr.minute, CronField::Value(0)));
        assert!(matches!(expr.hour, CronField::Value(9)));
        assert!(matches!(expr.day, CronField::Any));
    }

    #[test]
    fn test_natural_language_hourly() {
        let expr = CronExpression::parse_natural_language("hourly").unwrap();
        assert!(matches!(expr.minute, CronField::Value(0)));
        assert!(matches!(expr.hour, CronField::Any));
    }

    #[test]
    fn test_natural_language_daily() {
        let expr = CronExpression::parse_natural_language("daily").unwrap();
        assert!(matches!(expr.minute, CronField::Value(0)));
        assert!(matches!(expr.hour, CronField::Value(0)));
    }

    #[test]
    fn test_natural_language_weekly_on_monday() {
        let expr = CronExpression::parse_natural_language("weekly on Monday at 14:00").unwrap();
        assert!(matches!(expr.minute, CronField::Value(0)));
        assert!(matches!(expr.hour, CronField::Value(14)));
        assert!(matches!(expr.weekday, CronField::Value(1)));
    }

    #[test]
    fn test_natural_language_monthly_on_day() {
        let expr = CronExpression::parse_natural_language("monthly on day 15 at 10:00").unwrap();
        assert!(matches!(expr.minute, CronField::Value(0)));
        assert!(matches!(expr.hour, CronField::Value(10)));
        assert!(matches!(expr.day, CronField::Value(15)));
    }

    #[test]
    fn test_natural_language_invalid() {
        assert!(CronExpression::parse_natural_language("invalid input").is_err());
    }
}
