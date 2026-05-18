//! Daemon process lifecycle for bolt-ui desktop shell.
//!
//! Per-session spawn model: each Host/Join spawns a daemon with rendezvous args.
//! Uses bolt-app-core for binary resolution, platform paths, and process management.
//! Owns child handle directly (not supervised — session-scoped).

use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;

// bolt_app_core re-exports available for future use:
// platform paths, IPC types, watchdog, signal monitor, etc.

/// Daemon process handle with stderr capture.
pub struct DaemonProcess {
    child: Child,
    pid: u32,
    pub stderr_lines: Arc<Mutex<Vec<String>>>,
    _stderr_thread: thread::JoinHandle<()>,
}

/// Resolve daemon binary using bolt-app-core shared resolution + extra desktop paths.
pub fn find_daemon_binary() -> Result<PathBuf, String> {
    // Desktop-specific search paths (bolt-ui development locations)
    let home = std::env::var("HOME").unwrap_or_default();
    let extra_paths = vec![
        // Ecosystem workspace paths (bolt-ui is at bolt-core-sdk/rust/bolt-ui,
        // daemon is at bolt-ecosystem/bolt-daemon — 3 levels up from CARGO_MANIFEST_DIR)
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../bolt-daemon/target/release/bolt-daemon"),
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../bolt-daemon/target/debug/bolt-daemon"),
        // Desktop deployment paths
        PathBuf::from(format!("{home}/Desktop/bolt-daemon")),
        // Explicit ecosystem paths
        PathBuf::from(format!(
            "{home}/Desktop/the9ines.com/bolt-ecosystem/bolt-daemon/target/release/bolt-daemon"
        )),
    ];

    for path in &extra_paths {
        if path.exists() {
            return Ok(path.clone());
        }
    }

    // Sibling of executable
    if let Ok(exe) = std::env::current_exe() {
        if let Some(exe_dir) = exe.parent() {
            let sibling = exe_dir.join("bolt-daemon");
            if sibling.exists() {
                return Ok(sibling);
            }
        }
    }

    // System PATH (via bolt-app-core shared resolution)
    let mut lifecycle = bolt_app_core::daemon_lifecycle::DaemonLifecycle::new("0.0.0");
    lifecycle.add_binary_search_paths(Vec::new());
    if let Ok(p) = lifecycle.resolve_daemon_binary() {
        return Ok(p);
    }

    Err("bolt-daemon binary not found. Build with: cd bolt-daemon && cargo build --release".into())
}

/// Check if rendezvous server is reachable (quick TCP probe).
/// Delegates to bolt-app-core signal_monitor probe logic.
pub fn probe_rendezvous(url: &str) -> bool {
    let addr = url.trim_start_matches("ws://").trim_start_matches("wss://");
    std::net::TcpStream::connect_timeout(
        &addr
            .parse()
            .unwrap_or_else(|_| "127.0.0.1:3001".parse().unwrap()),
        std::time::Duration::from_secs(2),
    )
    .is_ok()
}

/// Find the rendezvous server binary.
pub fn find_rendezvous_binary() -> Option<PathBuf> {
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../bolt-rendezvous/target/release/bolt-rendezvous");
    if workspace.exists() {
        return Some(workspace);
    }
    None
}

impl DaemonProcess {
    /// Spawn daemon as Host (answerer).
    #[allow(clippy::too_many_arguments)]
    pub fn spawn_host(
        daemon_bin: &PathBuf,
        peer_id: &str,
        expect_peer: &str,
        room: &str,
        session: &str,
        socket_path: &str,
        data_dir: &str,
        rendezvous_url: &str,
    ) -> Result<Self, String> {
        // Respect the URL as given — may be ws:// or wss://
        let ws_url = if rendezvous_url.starts_with("ws://") || rendezvous_url.starts_with("wss://")
        {
            rendezvous_url.to_string()
        } else {
            format!("ws://{rendezvous_url}")
        };
        Self::spawn(
            daemon_bin,
            &[
                "--role",
                "answerer",
                "--signal",
                "rendezvous",
                "--rendezvous-url",
                &ws_url,
                "--room",
                room,
                "--session",
                session,
                "--peer-id",
                peer_id,
                "--expect-peer",
                expect_peer,
                "--socket-path",
                socket_path,
                "--data-dir",
                data_dir,
                "--pairing-policy",
                "allow",
                "--phase-timeout-secs",
                "3600",
            ],
        )
    }

    /// Spawn daemon as Join (offerer).
    #[allow(clippy::too_many_arguments)]
    pub fn spawn_join(
        daemon_bin: &PathBuf,
        peer_id: &str,
        to_peer: &str,
        room: &str,
        session: &str,
        socket_path: &str,
        data_dir: &str,
        rendezvous_url: &str,
    ) -> Result<Self, String> {
        // Respect the URL as given — may be ws:// or wss://
        let ws_url = if rendezvous_url.starts_with("ws://") || rendezvous_url.starts_with("wss://")
        {
            rendezvous_url.to_string()
        } else {
            format!("ws://{rendezvous_url}")
        };
        Self::spawn(
            daemon_bin,
            &[
                "--role",
                "offerer",
                "--signal",
                "rendezvous",
                "--rendezvous-url",
                &ws_url,
                "--room",
                room,
                "--session",
                session,
                "--peer-id",
                peer_id,
                "--to",
                to_peer,
                "--socket-path",
                socket_path,
                "--data-dir",
                data_dir,
                "--pairing-policy",
                "allow",
                "--phase-timeout-secs",
                "3600",
            ],
        )
    }

    /// Spawn daemon as a direct WS endpoint server for browser connections.
    /// Uses ws-endpoint mode — no WebRTC/file-signal path, just WS serving.
    pub fn spawn_ws_server(
        daemon_bin: &PathBuf,
        ws_listen: &str,
        socket_path: &str,
        data_dir: &str,
    ) -> Result<Self, String> {
        Self::spawn(
            daemon_bin,
            &[
                "--mode",
                "ws-endpoint",
                "--ws-listen",
                ws_listen,
                "--socket-path",
                socket_path,
                "--data-dir",
                data_dir,
                "--pairing-policy",
                "allow",
            ],
        )
    }

    fn spawn(daemon_bin: &PathBuf, args: &[&str]) -> Result<Self, String> {
        // Ensure data-dir exists with secure permissions
        if let Some(idx) = args.iter().position(|a| *a == "--data-dir") {
            if let Some(dir) = args.get(idx + 1) {
                let _ = std::fs::create_dir_all(dir);
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700));
                }
            }
        }

        let mut child = Command::new(daemon_bin)
            .args(args)
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("Failed to spawn daemon: {e}"))?;

        let pid = child.id();
        let stderr_lines: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let lines_clone = Arc::clone(&stderr_lines);

        let stderr = child.stderr.take().ok_or("No stderr handle")?;
        let stderr_thread = thread::spawn(move || {
            let reader = BufReader::new(stderr);
            for line in reader.lines().map_while(Result::ok) {
                if let Ok(mut buf) = lines_clone.lock() {
                    buf.push(line);
                }
            }
        });

        Ok(Self {
            child,
            pid,
            stderr_lines,
            _stderr_thread: stderr_thread,
        })
    }

    pub fn pid(&self) -> u32 {
        self.pid
    }

    pub fn is_running(&mut self) -> bool {
        self.child.try_wait().ok().flatten().is_none()
    }

    pub fn recent_stderr(&self, last_n: usize) -> Vec<String> {
        self.stderr_lines
            .lock()
            .map(|buf| {
                let start = buf.len().saturating_sub(last_n);
                buf[start..].to_vec()
            })
            .unwrap_or_default()
    }

    pub fn has_connected(&self) -> bool {
        self.stderr_lines
            .lock()
            .map(|buf| {
                buf.iter().any(|l| {
                    l.contains("hello/ack complete")
                        || l.contains("connection state: Connected")
                        || l.contains("[DC_OPEN]")
                })
            })
            .unwrap_or(false)
    }

    pub fn sas_code(&self) -> Option<String> {
        self.stderr_lines.lock().ok().and_then(|buf| {
            buf.iter().find_map(|l| {
                if l.contains("[SAS]") {
                    l.split("[SAS]").nth(1).map(|s| s.trim().to_string())
                } else {
                    None
                }
            })
        })
    }

    pub fn is_transfer_ready(&self) -> bool {
        self.stderr_lines
            .lock()
            .map(|buf| buf.iter().any(|l| l.contains("[DC_OPEN]")))
            .unwrap_or(false)
    }

    pub fn last_error(&self) -> Option<String> {
        self.stderr_lines.lock().ok().and_then(|buf| {
            buf.iter().rev().find_map(|l| {
                if l.contains("FATAL") || l.contains("ERROR") || l.contains("error") {
                    Some(l.clone())
                } else {
                    None
                }
            })
        })
    }

    pub fn kill(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for DaemonProcess {
    fn drop(&mut self) {
        self.kill();
    }
}

/// Generate a short random ID for room/session.
pub fn generate_room_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    format!("r{:x}", ts % 0xFFFFFF)
}

pub fn generate_session_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("s{:x}", ts % 0xFFFFFFFF)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn room_session_ids_not_empty() {
        let room = generate_room_id();
        let session = generate_session_id();
        assert!(!room.is_empty());
        assert!(!session.is_empty());
        assert!(room.starts_with('r'));
        assert!(session.starts_with('s'));
    }

    #[test]
    fn find_daemon_binary_does_not_panic() {
        let _ = find_daemon_binary();
    }

    #[test]
    fn probe_rendezvous_does_not_hang() {
        let start = std::time::Instant::now();
        let _ = probe_rendezvous("127.0.0.1:39999");
        assert!(start.elapsed().as_secs() < 5);
    }

    #[test]
    fn platform_paths_available() {
        // bolt-app-core platform paths should work
        assert!(!bolt_app_core::platform::default_ipc_path().is_empty());
        assert!(!bolt_app_core::platform::default_data_dir().is_empty());
    }
}
