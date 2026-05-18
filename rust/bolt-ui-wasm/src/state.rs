// EW2 REUSE: Adapted from bolt-ui/src/state.rs.
// Change: std::time::Instant → web_time::Instant (WASM-compatible).
// All enums, methods, and constants otherwise identical.

use std::time::Duration;
use web_time::Instant;

pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectMode {
    Host,
    Join,
}

#[derive(Debug, Clone)]
pub struct HostInfo {
    pub peer_code: String,
    pub room: String,
    pub session: String,
}

impl HostInfo {
    pub fn share_string(&self) -> String {
        format!("{} / {} / {}", self.room, self.session, self.peer_code)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionState {
    Idle,
    Connecting { started_at: Instant },
    Connected,
    TimedOut,
    Error(String),
}

impl ConnectionState {
    pub fn status_text(&self) -> &str {
        match self {
            Self::Idle => "Ready",
            Self::Connecting { .. } => "Connecting\u{2026}",
            Self::Connected => "Connected",
            Self::TimedOut => "Connection timed out",
            Self::Error(_) => "Connection failed",
        }
    }

    pub fn is_timed_out(&self) -> bool {
        match self {
            Self::Connecting { started_at } => started_at.elapsed() >= CONNECT_TIMEOUT,
            _ => false,
        }
    }

    pub fn can_retry(&self) -> bool {
        matches!(self, Self::Idle | Self::TimedOut | Self::Error(_))
    }

    pub fn can_cancel(&self) -> bool {
        matches!(self, Self::Connecting { .. })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum TransferState {
    Idle,
    Ready,
    Sending { file_name: String, progress: f32 },
    Receiving { file_name: String, progress: f32 },
    Complete { file_name: String },
    Failed { file_name: String, reason: String },
}

impl TransferState {
    pub fn status_text(&self) -> String {
        match self {
            Self::Idle => "No active transfer".to_string(),
            Self::Ready => "Connected — ready to transfer".to_string(),
            Self::Sending {
                file_name,
                progress,
            } => {
                format!("Sending {} ({:.0}%)", file_name, progress * 100.0)
            }
            Self::Receiving {
                file_name,
                progress,
            } => {
                format!("Receiving {} ({:.0}%)", file_name, progress * 100.0)
            }
            Self::Complete { file_name } => format!("{} — complete", file_name),
            Self::Failed { file_name, reason } => {
                format!("{} — failed: {}", file_name, reason)
            }
        }
    }

    pub fn progress(&self) -> f32 {
        match self {
            Self::Sending { progress, .. } | Self::Receiving { progress, .. } => *progress,
            Self::Complete { .. } => 1.0,
            _ => 0.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerifyState {
    NotStarted,
    Pending { sas_code: String },
    Confirmed,
    Rejected,
}

impl VerifyState {
    pub fn status_text(&self) -> &str {
        match self {
            Self::NotStarted => "No active session to verify",
            Self::Pending { .. } => "Compare this code with your peer",
            Self::Confirmed => "Verified — session authenticated",
            Self::Rejected => "Rejected — session terminated",
        }
    }
}
