use chrono::{Datelike, Timelike};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;
use anyhow::Result;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduledJob {
    pub id: String,
    pub workflow_id: String,
    pub cron_expr: String,
    pub timezone: String,
    pub last_run_at: Option<i64>,
    pub next_run_at: i64,
    pub enabled: bool,
}

pub struct Scheduler {
    jobs: Arc<Mutex<Vec<ScheduledJob>>>,
}

impl Default for Scheduler {
    fn default() -> Self {
        Self::new()
    }
}

impl Scheduler {
    pub fn new() -> Self {
        Self {
            jobs: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub async fn add_job(&self, job: ScheduledJob) -> Result<()> {
        let mut jobs = self.jobs.lock().await;
        jobs.push(job);
        Ok(())
    }

    pub async fn remove_job(&self, id: &str) -> Result<()> {
        let mut jobs = self.jobs.lock().await;
        jobs.retain(|j| j.id != id);
        Ok(())
    }

    pub async fn get_due_jobs(&self, now: i64) -> Vec<ScheduledJob> {
        let jobs = self.jobs.lock().await;
        jobs.iter()
            .filter(|j| j.enabled && j.next_run_at <= now)
            .cloned()
            .collect()
    }

    pub async fn update_last_run(&self, id: &str, ran_at: i64) -> Result<()> {
        let mut jobs = self.jobs.lock().await;
        if let Some(job) = jobs.iter_mut().find(|j| j.id == id) {
            job.last_run_at = Some(ran_at);
            let cron = parse_cron(&job.cron_expr)?;
            job.next_run_at = next_timestamp(&cron, ran_at)?;
        }
        Ok(())
    }

    pub async fn start_loop<F, Fut>(&self, interval_secs: u64, executor: F)
    where
        F: Fn(ScheduledJob) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = Result<()>> + Send,
    {
        let jobs = self.jobs.clone();
        let interval = std::time::Duration::from_secs(interval_secs);

        tokio::spawn(async move {
            loop {
                tokio::time::sleep(interval).await;
                let now = chrono::Utc::now().timestamp();
                let due: Vec<ScheduledJob> = {
                    let guard = jobs.lock().await;
                    guard.iter()
                        .filter(|j| j.enabled && j.next_run_at <= now)
                        .cloned()
                        .collect()
                };

                for job in due {
                    let job_id = job.id.clone();
                    if let Err(e) = executor(job).await {
                        tracing::error!(job_id = %job_id, error = %e, "Scheduled job execution failed");
                    } else {
                        let mut guard = jobs.lock().await;
                        if let Some(j) = guard.iter_mut().find(|j| j.id == job_id) {
                            j.last_run_at = Some(now);
                            if let Ok(cron) = parse_cron(&j.cron_expr)
                                && let Ok(next) = next_timestamp(&cron, now)
                            {
                                j.next_run_at = next;
                            }
                        }
                    }
                }
            }
        });
    }
}

#[derive(Debug)]
struct CronField {
    values: Vec<i64>,
}

#[derive(Debug)]
struct CronExpr {
    minute: CronField,
    hour: CronField,
    day: CronField,
    month: CronField,
    weekday: CronField,
}

fn parse_field(input: &str, min: i64, max: i64) -> Result<CronField> {
    let mut values = Vec::new();
    for part in input.split(',') {
        if part == "*" {
            values = (min..=max).collect();
            break;
        }
        if let Some((start_s, end_s)) = part.split_once('-') {
            let start: i64 = start_s.parse()?;
            let end: i64 = end_s.parse()?;
            values.extend(start..=end);
        } else if let Some((base_s, step_s)) = part.split_once('/') {
            let step: i64 = step_s.parse()?;
            let base: i64 = if base_s == "*" { min } else { base_s.parse()? };
            let mut v = base;
            while v <= max {
                values.push(v);
                v += step;
            }
        } else {
            values.push(part.parse::<i64>()?);
        }
    }
    values.retain(|v| *v >= min && *v <= max);
    values.sort();
    values.dedup();
    Ok(CronField { values })
}

fn parse_cron(expr: &str) -> Result<CronExpr> {
    let parts: Vec<&str> = expr.split_whitespace().collect();
    if parts.len() != 5 {
        return Err(anyhow::anyhow!("Cron expression must have 5 fields: min hour day month weekday"));
    }
    Ok(CronExpr {
        minute: parse_field(parts[0], 0, 59)?,
        hour: parse_field(parts[1], 0, 23)?,
        day: parse_field(parts[2], 1, 31)?,
        month: parse_field(parts[3], 1, 12)?,
        weekday: parse_field(parts[4], 0, 6)?,
    })
}

fn next_timestamp(cron: &CronExpr, from: i64) -> Result<i64> {
    let from_dt = chrono::DateTime::from_timestamp(from, 0)
        .unwrap_or(chrono::Utc::now());
    let mut dt = from_dt + chrono::Duration::minutes(1);
    for _ in 0..525600 {
        if cron.month.values.contains(&(dt.month() as i64))
            && cron.weekday.values.contains(&(dt.weekday().num_days_from_sunday() as i64))
            && cron.day.values.contains(&(dt.day() as i64))
            && cron.hour.values.contains(&(dt.hour() as i64))
            && cron.minute.values.contains(&(dt.minute() as i64))
        {
            return Ok(dt.timestamp());
        }
        dt += chrono::Duration::minutes(1);
    }
    Err(anyhow::anyhow!("No matching time found within 1 year"))
}

/// Public helper: compute next timestamp from a cron expression
pub fn next_timestamp_from_cron(expr: &str, from: i64) -> Result<i64> {
    let cron = parse_cron(expr)?;
    next_timestamp(&cron, from)
}

/// Public helper: list all jobs from the scheduler
impl Scheduler {
    pub async fn list_jobs(&self) -> Vec<ScheduledJob> {
        let jobs = self.jobs.lock().await;
        jobs.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_add_and_remove_job() {
        let scheduler = Scheduler::new();
        let job = ScheduledJob {
            id: "job-1".to_string(),
            workflow_id: "wf-1".to_string(),
            cron_expr: "0 * * * *".to_string(),
            timezone: "UTC".to_string(),
            last_run_at: None,
            next_run_at: 0,
            enabled: true,
        };
        scheduler.add_job(job.clone()).await.unwrap();
        let due = scheduler.get_due_jobs(0).await;
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].id, "job-1");

        scheduler.remove_job("job-1").await.unwrap();
        let due = scheduler.get_due_jobs(0).await;
        assert_eq!(due.len(), 0);
    }

    #[test]
    fn test_parse_cron_wildcard() {
        let cron = parse_cron("* * * * *").unwrap();
        assert_eq!(cron.minute.values.len(), 60);
        assert_eq!(cron.hour.values.len(), 24);
    }

    #[test]
    fn test_parse_cron_specific() {
        let cron = parse_cron("30 9 * * 1").unwrap();
        assert_eq!(cron.minute.values, vec![30]);
        assert_eq!(cron.hour.values, vec![9]);
        assert_eq!(cron.weekday.values, vec![1]);
    }

    #[test]
    fn test_next_timestamp() {
        let cron = parse_cron("0 12 * * *").unwrap();
        let from = chrono::Utc::now().timestamp();
        let next = next_timestamp(&cron, from).unwrap();
        let next_dt = chrono::DateTime::from_timestamp(next, 0).unwrap();
        assert_eq!(next_dt.hour(), 12);
        assert_eq!(next_dt.minute(), 0);
    }
}