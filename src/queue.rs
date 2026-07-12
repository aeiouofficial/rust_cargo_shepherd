//! Priority queue for scheduled Cargo jobs.
//!
//! Ordering rules:
//! - higher priority first
//! - FIFO within the same priority (earlier `queued_at` wins)
//!
//! The queue is a small sorted `Vec`. With ≤5 priority levels and a human-scale
//! job count, this is simpler and faster than a binary heap for our access
//! patterns (prefix lookup, remove-by-id, project bulk remove).

use crate::config::Priority;
use crate::ipc::DaemonMsg;
use chrono::{DateTime, Utc};
use tokio::sync::mpsc;

/// One waiting job, already resolved against config (alias / child_jobs).
#[derive(Debug, Clone)]
pub struct QueuedJob {
    pub job_id: String,
    pub project_dir: String,
    pub alias: String,
    pub args: Vec<String>,
    pub priority: Priority,
    pub queued_at: DateTime<Utc>,
    /// Per-invocation `CARGO_BUILD_JOBS`.
    pub child_jobs: usize,
    /// When set, the daemon streams lifecycle events back over this channel
    /// (used by the `cargo.exe` shim / `RunAttached`).
    pub attached_tx: Option<mpsc::UnboundedSender<DaemonMsg>>,
}

/// Sorted queue: index 0 is always the next job to run.
#[derive(Debug, Default)]
pub struct PriorityQueue {
    inner: Vec<QueuedJob>,
}

impl PriorityQueue {
    pub fn new() -> Self {
        Self { inner: Vec::new() }
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Peek at the next job without removing it.
    pub fn peek(&self) -> Option<&QueuedJob> {
        self.inner.first()
    }

    /// Insert keeping sort order (priority desc, then enqueue time asc).
    pub fn push(&mut self, job: QueuedJob) {
        let pos = self.inner.partition_point(|existing| {
            existing.priority > job.priority
                || (existing.priority == job.priority && existing.queued_at <= job.queued_at)
        });
        self.inner.insert(pos, job);
    }

    /// Pop the highest-priority / earliest job.
    pub fn pop_next(&mut self) -> Option<QueuedJob> {
        if self.inner.is_empty() {
            None
        } else {
            Some(self.inner.remove(0))
        }
    }

    /// Change priority of a queued job and re-sort. Returns false if missing.
    pub fn set_priority(&mut self, job_id: &str, new_priority: Priority) -> bool {
        if let Some(pos) = self.inner.iter().position(|j| j.job_id == job_id) {
            let mut job = self.inner.remove(pos);
            job.priority = new_priority;
            self.push(job);
            true
        } else {
            false
        }
    }

    /// Remove one job by id (user cancel).
    pub fn remove(&mut self, job_id: &str) -> Option<QueuedJob> {
        self.inner
            .iter()
            .position(|j| j.job_id == job_id)
            .map(|pos| self.inner.remove(pos))
    }

    /// Remove every job for a project directory.
    pub fn remove_project(&mut self, project_dir: &str) -> Vec<QueuedJob> {
        let (removed, kept): (Vec<_>, Vec<_>) = self
            .inner
            .drain(..)
            .partition(|j| j.project_dir == project_dir);
        self.inner = kept;
        removed
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Ordered snapshot for status reporting.
    pub fn snapshot(&self) -> Vec<QueuedJob> {
        self.inner.clone()
    }

    /// Position in queue (0 = next), or None if not found.
    pub fn position_of(&self, job_id: &str) -> Option<usize> {
        self.inner.iter().position(|j| j.job_id == job_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_job(id: &str, priority: Priority, offset_ms: i64) -> QueuedJob {
        QueuedJob {
            job_id: id.to_string(),
            project_dir: "/tmp/test".to_string(),
            alias: "test".to_string(),
            args: vec!["check".to_string()],
            priority,
            queued_at: Utc::now() + chrono::Duration::milliseconds(offset_ms),
            child_jobs: 2,
            attached_tx: None,
        }
    }

    #[test]
    fn priority_ordering() {
        let mut q = PriorityQueue::new();
        q.push(make_job("low", Priority::Low, 0));
        q.push(make_job("high", Priority::High, 100));
        q.push(make_job("norm", Priority::Normal, 50));
        q.push(make_job("crit", Priority::Critical, 200));

        assert_eq!(q.pop_next().unwrap().job_id, "crit");
        assert_eq!(q.pop_next().unwrap().job_id, "high");
        assert_eq!(q.pop_next().unwrap().job_id, "norm");
        assert_eq!(q.pop_next().unwrap().job_id, "low");
    }

    #[test]
    fn fifo_within_same_priority() {
        let mut q = PriorityQueue::new();
        q.push(make_job("first", Priority::Normal, 0));
        q.push(make_job("second", Priority::Normal, 10));
        q.push(make_job("third", Priority::Normal, 20));

        assert_eq!(q.pop_next().unwrap().job_id, "first");
        assert_eq!(q.pop_next().unwrap().job_id, "second");
        assert_eq!(q.pop_next().unwrap().job_id, "third");
    }

    #[test]
    fn reprioritize_moves_job() {
        let mut q = PriorityQueue::new();
        q.push(make_job("a", Priority::Normal, 0));
        q.push(make_job("b", Priority::Low, 10));

        assert!(q.set_priority("b", Priority::Critical));
        assert_eq!(q.pop_next().unwrap().job_id, "b");
        assert_eq!(q.pop_next().unwrap().job_id, "a");
    }

    #[test]
    fn remove_project_keeps_others() {
        let mut q = PriorityQueue::new();
        let mut job_a = make_job("a", Priority::Normal, 0);
        job_a.project_dir = "/project/foo".to_string();
        let mut job_b = make_job("b", Priority::Normal, 10);
        job_b.project_dir = "/project/bar".to_string();
        let mut job_c = make_job("c", Priority::High, 20);
        job_c.project_dir = "/project/foo".to_string();

        q.push(job_a);
        q.push(job_b);
        q.push(job_c);

        let removed = q.remove_project("/project/foo");
        assert_eq!(removed.len(), 2);
        assert_eq!(q.len(), 1);
        assert_eq!(q.pop_next().unwrap().job_id, "b");
    }

    #[test]
    fn peek_shows_next_without_pop() {
        let mut q = PriorityQueue::new();
        q.push(make_job("bg", Priority::Background, 0));
        q.push(make_job("hi", Priority::High, 1));
        assert_eq!(q.peek().unwrap().job_id, "hi");
        assert_eq!(q.len(), 2);
    }
}
