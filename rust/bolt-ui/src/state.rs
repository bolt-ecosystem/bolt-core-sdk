use std::time::{Duration, Instant};

pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);

// ── Discovered Peer ──────────────────────────────────────────

/// Which signaling plane discovered this peer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SignalingPlane {
    Local,
    Cloud,
}

/// A discovered nearby device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredPeer {
    pub peer_code: String,
    pub device_name: String,
    pub device_type: DeviceType,
    /// Which signaling plane this peer was discovered on.
    pub plane: SignalingPlane,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceType {
    Desktop,
    Laptop,
    Phone,
    Tablet,
    Browser,
    Unknown,
}

impl DeviceType {
    /// Retro icon glyph for device type.
    pub fn icon(&self) -> &str {
        match self {
            Self::Desktop => "\u{25A3}", // ▣
            Self::Laptop => "\u{25A1}",  // □
            Self::Phone => "\u{25AB}",   // ▫
            Self::Tablet => "\u{25AD}",  // ▭
            Self::Browser => "\u{25C7}", // ◇
            Self::Unknown => "\u{25CB}", // ○
        }
    }
}

// ── Discovery State ──────────────────────────────────────────

/// Discovery/signaling connection status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiscoveryStatus {
    /// Not connected to signaling.
    Offline,
    /// Connected, actively searching.
    Searching,
    /// Connected, peers visible.
    Active,
}

impl DiscoveryStatus {
    pub fn label(&self) -> &str {
        match self {
            Self::Offline => "OFFLINE",
            Self::Searching => "SEARCHING",
            Self::Active => "NEARBY",
        }
    }
}

// ── Connection State ─────────────────────────────────────────

/// Connection lifecycle (matches web app phases).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionState {
    /// No connection, discovery running.
    Idle,
    /// Waiting for remote to accept.
    Requesting {
        peer_code: String,
        peer_name: String,
        started_at: Instant,
    },
    /// Establishing transport (daemon spawning / WebRTC handshake).
    Establishing {
        peer_code: String,
        peer_name: String,
        started_at: Instant,
    },
    /// Connected to peer.
    Connected,
    /// Connection timed out.
    TimedOut,
    /// Connection failed.
    Error(String),
}

impl ConnectionState {
    pub fn status_text(&self) -> String {
        match self {
            Self::Idle => "Ready".to_string(),
            Self::Requesting { peer_name, .. } => {
                format!("Waiting for {} to accept...", peer_name)
            }
            Self::Establishing { peer_name, .. } => {
                format!("Connecting to {}...", peer_name)
            }
            Self::Connected => "Connected".to_string(),
            Self::TimedOut => "Connection timed out".to_string(),
            Self::Error(msg) => format!("Failed: {msg}"),
        }
    }

    pub fn is_timed_out(&self) -> bool {
        match self {
            Self::Requesting { started_at, .. } | Self::Establishing { started_at, .. } => {
                started_at.elapsed() >= CONNECT_TIMEOUT
            }
            _ => false,
        }
    }

    pub fn is_connecting(&self) -> bool {
        matches!(self, Self::Requesting { .. } | Self::Establishing { .. })
    }

    pub fn can_retry(&self) -> bool {
        matches!(self, Self::Idle | Self::TimedOut | Self::Error(_))
    }

    pub fn can_cancel(&self) -> bool {
        self.is_connecting()
    }
}

// ── Connected Peer ───────────────────────────────────────────

/// Info about the currently connected peer.
#[derive(Debug, Clone)]
pub struct ConnectedPeer {
    pub peer_code: String,
    pub device_name: String,
    pub device_type: DeviceType,
}

// ── Incoming Connection Request ───────────────────────────────

/// An inbound connection request from another peer.
#[derive(Debug, Clone)]
pub struct IncomingRequest {
    pub peer_code: String,
    pub device_name: String,
    pub device_type: DeviceType,
    /// Which signaling plane this request arrived on.
    pub plane: SignalingPlane,
}

// ── Legacy Host/Join (manual fallback) ───────────────────────

/// Connection info for manual pairing fallback.
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

// ── Transfer State ───────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum TransferState {
    Idle,
    Ready,
    #[allow(dead_code)]
    Sending {
        file_name: String,
        progress: f32,
    },
    #[allow(dead_code)]
    Receiving {
        file_name: String,
        progress: f32,
    },
    #[allow(dead_code)]
    Complete {
        file_name: String,
        save_path: Option<String>,
    },
    #[allow(dead_code)]
    Failed {
        file_name: String,
        reason: String,
    },
}

impl TransferState {
    pub fn status_text(&self) -> String {
        match self {
            Self::Idle => "No active transfer".to_string(),
            Self::Ready => "Ready to transfer".to_string(),
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
            Self::Complete {
                file_name,
                save_path,
            } => {
                if let Some(path) = save_path {
                    format!("{} — saved to {}", file_name, path)
                } else {
                    format!("{} — complete", file_name)
                }
            }
            Self::Failed { file_name, reason } => format!("{} — failed: {}", file_name, reason),
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

// ── Verification State ───────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerifyState {
    NotStarted,
    /// Legacy mode — no identity configured, transfer allowed immediately.
    Legacy,
    Pending {
        sas_code: String,
    },
    Confirmed,
    Rejected,
}

impl VerifyState {
    pub fn status_text(&self) -> &str {
        match self {
            Self::NotStarted => "No active session",
            Self::Legacy => "Legacy Peer",
            Self::Pending { .. } => "Compare this code with your peer",
            Self::Confirmed => "Verified",
            Self::Rejected => "Rejected",
        }
    }

    /// Whether file transfer is allowed — matches web app policy.
    /// Transfer is allowed only when verified or in legacy mode.
    pub fn is_transfer_allowed(&self) -> bool {
        matches!(self, Self::Confirmed | Self::Legacy)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connection_timeout() {
        let past = Instant::now() - CONNECT_TIMEOUT - Duration::from_millis(100);
        let state = ConnectionState::Requesting {
            peer_code: "T1".into(),
            peer_name: "test".into(),
            started_at: past,
        };
        assert!(state.is_timed_out());
    }

    #[test]
    fn connection_states() {
        assert!(ConnectionState::Idle.can_retry());
        assert!(ConnectionState::TimedOut.can_retry());
        assert!(ConnectionState::Error("x".into()).can_retry());
        assert!(!ConnectionState::Connected.can_retry());

        let req = ConnectionState::Requesting {
            peer_code: "P1".into(),
            peer_name: "p".into(),
            started_at: Instant::now(),
        };
        assert!(req.can_cancel());
        assert!(req.is_connecting());
        assert!(!ConnectionState::Idle.can_cancel());
    }

    #[test]
    fn discovery_labels() {
        assert_eq!(DiscoveryStatus::Offline.label(), "OFFLINE");
        assert_eq!(DiscoveryStatus::Active.label(), "NEARBY");
    }

    #[test]
    fn device_icons() {
        assert!(!DeviceType::Desktop.icon().is_empty());
        assert!(!DeviceType::Browser.icon().is_empty());
    }

    #[test]
    fn transfer_ready_state() {
        assert!(TransferState::Ready.status_text().contains("Ready"));
    }
}
