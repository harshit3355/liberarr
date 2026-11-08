//! Jobs context: resumable pipeline steps, a queue port and the one-job runner.
use crate::Result;

/// The pipeline, in order. A step's success enqueues the next one.
pub const STEPS: [&str; 3] = ["transcribe", "summarize", "export"];
pub const MAX_ERROR: usize = 2000;

pub fn next_step(step: &str) -> Option<&'static str> {
    let i = STEPS.iter().position(|s| *s == step)?;
    STEPS.get(i + 1).copied()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Job {
    pub id: i64,
    pub recording_id: i64,
    pub step: String,
    pub attempts: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobStatus {
    Queued,
    Running,
    Failed,
    Done,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobInfo {
    pub id: i64,
    pub step: String,
    pub status: JobStatus,
    pub attempts: u32,
    pub error: Option<String>,
    pub updated_at: jiff::Timestamp,
}

pub trait JobQueue {
    async fn enqueue(&self, recording_id: i64, step: &str) -> Result<i64>;
    /// Atomic: two workers never get the same job.
    async fn claim_next(&self) -> Result<Option<Job>>;
    /// Marks the job done and, in the same transaction, enqueues `then`.
    async fn complete(&self, job: &Job, then: Option<&str>) -> Result<()>;
    /// `error` is at most `MAX_ERROR` chars (the runner truncates).
    async fn fail(&self, job: &Job, error: &str) -> Result<()>;
    /// Re-queues jobs left "running" by a dead process.
    async fn reset_stale(&self) -> Result<u64>;
    async fn list_for(&self, recording_id: i64) -> Result<Vec<JobInfo>>;
}

/// Runs one pipeline step for a job; the composition root dispatches on `job.step`.
pub trait StepHandler {
    /// `Ok(false)` ends the chain there (e.g. no speech: nothing to summarize or export).
    async fn run(&self, job: &Job) -> Result<bool>;
}

/// Claims and runs one job. `Ok(false)` when the queue is empty. A handler error is recorded on
/// the job (prior outputs stay) and is not an error of the runner itself.
pub async fn run_job_once<Q: JobQueue, H: StepHandler>(q: &Q, handlers: &H) -> Result<bool> {
    let Some(job) = q.claim_next().await? else {
        return Ok(false);
    };
    match handlers.run(&job).await {
        Ok(go_on) => {
            q.complete(&job, next_step(&job.step).filter(|_| go_on))
                .await?
        }
        Err(e) => {
            let msg: String = e.to_string().chars().take(MAX_ERROR).collect();
            q.fail(&job, &msg).await?;
        }
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Error;
    use crate::fakes::{MemJobs, block_on};

    /// Fails the chosen step, succeeds on the others.
    struct FailAt(&'static str);
    impl StepHandler for FailAt {
        async fn run(&self, job: &Job) -> Result<bool> {
            if job.step == self.0 {
                Err(Error::Provider("boom".into()))
            } else {
                Ok(true)
            }
        }
    }

    #[test]
    fn job_chain_and_failure() {
        let q = MemJobs::default();
        let h = FailAt("summarize");
        block_on(q.enqueue(1, "transcribe")).unwrap();

        // Success enqueues the next step.
        assert!(block_on(run_job_once(&q, &h)).unwrap());
        assert_eq!(q.rows()[1].0, "summarize");

        // Failure records the error and stops the chain: nothing is enqueued after it.
        assert!(block_on(run_job_once(&q, &h)).unwrap());
        let rows = q.rows();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].1, JobStatus::Done);
        assert_eq!(rows[1].1, JobStatus::Failed);
        assert_eq!(rows[1].2.as_deref(), Some("provider: boom"));
        assert!(!block_on(run_job_once(&q, &h)).unwrap());
    }
}
