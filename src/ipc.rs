//! IPC protocol: newline-delimited JSON over a local transport.
//!
//! - Unix: domain socket (prefer `$XDG_RUNTIME_DIR`, fall back to `/tmp`)
//! - Windows: named pipe `\\.\pipe\cargo-shepherd`

use crate::config::Priority;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
#[cfg(unix)]
use std::path::PathBuf;

/// Unix domain socket path for the daemon.
///
/// Prefer the per-user runtime directory so multiple users (and sandboxes)
/// do not collide on a world-writable `/tmp` socket.
#[cfg(unix)]
pub fn socket_path() -> PathBuf {
    if let Ok(runtime) = std::env::var("XDG_RUNTIME_DIR") {
        if !runtime.trim().is_empty() {
            return PathBuf::from(runtime).join("cargo-shepherd.sock");
        }
    }
    PathBuf::from("/tmp/cargo-shepherd.sock")
}

/// Windows named pipe name.
#[cfg(windows)]
pub fn pipe_name() -> String {
    r"\\.\pipe\cargo-shepherd".to_string()
}

/// Client → Daemon
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMsg {
    /// Fire-and-forget: queue a cargo command, return once accepted.
    Run {
        job_id: String,
        project_dir: String,
        args: Vec<String>,
        /// Override project default priority. `None` = use config.
        priority: Option<Priority>,
    },

    /// Keep the connection open until the job finishes. Used by the `cargo`
    /// shim so callers get stdout/stderr and the real exit code.
    RunAttached {
        job_id: String,
        project_dir: String,
        args: Vec<String>,
        priority: Option<Priority>,
    },

    SetJobPriority {
        job_id: String,
        new_priority: Priority,
    },

    CancelJob {
        job_id: String,
    },

    KillProject {
        project_dir: String,
    },

    KillJob {
        job_id: String,
    },

    SetProjectPriority {
        project_dir: String,
        priority: Priority,
    },

    SetProjectAlias {
        project_dir: String,
        alias: String,
    },

    SetSlots {
        slots: usize,
    },

    SetProjectChildJobs {
        project_dir: String,
        child_jobs: usize,
    },

    /// Any `None` field leaves that setting unchanged.
    SetHerdConfig {
        herd_unmanaged: Option<bool>,
        herd_ram_pause_pct: Option<f64>,
        herd_ram_resume_pct: Option<f64>,
        herd_scan_ms: Option<u64>,
        herd_max_active: Option<usize>,
    },

    Status,
    GetConfig,
    Shutdown,
}

/// Daemon → Client
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DaemonMsg {
    Queued {
        job_id: String,
        position: usize,
    },

    Started {
        job_id: String,
        pid: u32,
    },

    Finished {
        job_id: String,
        exit_code: i32,
        duration_ms: u64,
    },

    CargoOutput {
        job_id: String,
        stream: CargoOutputStream,
        line: String,
    },

    Killed {
        description: String,
    },

    PriorityChanged {
        job_id: String,
        new_priority: Priority,
        new_position: usize,
    },

    StatusReport {
        report: StatusReport,
    },

    ConfigText {
        toml: String,
    },

    ConfigUpdated {
        message: String,
    },

    Error {
        message: String,
    },

    ShuttingDown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusReport {
    pub running: Vec<RunningJob>,
    pub queued: Vec<QueuedJobSnapshot>,
    pub slots_total: usize,
    pub slots_active: usize,
    pub cpu_pct: f32,
    pub ram_pct: f64,
    pub herd_unmanaged: bool,
    pub herd_ram_pause_pct: f64,
    pub herd_ram_resume_pct: f64,
    pub herd_scan_ms: u64,
    pub herd_max_active: usize,
    pub herd_active_external: usize,
    pub herd_held_external: usize,
}

impl StatusReport {
    /// Empty report used as the TUI default before the first successful poll.
    pub fn empty() -> Self {
        Self {
            running: Vec::new(),
            queued: Vec::new(),
            slots_total: 0,
            slots_active: 0,
            cpu_pct: 0.0,
            ram_pct: 0.0,
            herd_unmanaged: false,
            herd_ram_pause_pct: 75.0,
            herd_ram_resume_pct: 70.0,
            herd_scan_ms: 250,
            herd_max_active: 1,
            herd_active_external: 0,
            herd_held_external: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunningJob {
    pub job_id: String,
    pub project_dir: String,
    pub alias: String,
    pub args: Vec<String>,
    pub pid: u32,
    pub source: RunningJobSource,
    pub started_at: DateTime<Utc>,
    /// Computed by the daemon at snapshot time.
    pub elapsed_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunningJobSource {
    Sheppard,
    ExternalCargo,
    ExternalRust,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QueuedJobSource {
    Sheppard,
    SuspendedExternalRust,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CargoOutputStream {
    Stdout,
    Stderr,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueuedJobSnapshot {
    pub job_id: String,
    pub project_dir: String,
    pub alias: String,
    pub args: Vec<String>,
    pub priority: Priority,
    pub queued_at: DateTime<Utc>,
    #[serde(default = "default_queue_source")]
    pub source: QueuedJobSource,
    #[serde(default)]
    pub pid: Option<u32>,
    #[serde(default)]
    pub child_count: usize,
    #[serde(default)]
    pub reason: Option<String>,
    /// 0 = next to run.
    pub position: usize,
}

fn default_queue_source() -> QueuedJobSource {
    QueuedJobSource::Sheppard
}
