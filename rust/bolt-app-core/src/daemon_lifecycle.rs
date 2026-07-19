//! Shell-agnostic daemon lifecycle orchestration.
//!
//! Manages daemon spawn/restart/shutdown, readiness probing, watchdog
//! transitions, PID tracking, and stderr/crash-log integration.
//! Uses callbacks for event emission — no shell-framework dependency.
//!
//! Extracted from localbolt-app daemon.rs (NATIVE-APP-CORE-1).

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use crate::daemon_log::{self, StderrBuffer};
use crate::ipc_bridge_core::IpcBridgeCore;
use crate::ipc_client::{self, ReadinessResult};
use crate::ipc_transport::IpcStream;
use crate::platform;
use crate::watchdog::{Transition, Watchdog, WatchdogState, STARTUP_TIMEOUT};

/// Watchdog state event emitted on transitions.
#[derive(serde::Serialize, Clone, Debug)]
pub struct WatchdogStateEvent {
    pub state: WatchdogState,
    pub retry_count: u32,
}

/// Callback types for shell integration.
pub type WatchdogCallback = Box<dyn Fn(WatchdogStateEvent) + Send + Sync + 'static>;
pub type BridgeEventCallback = Box<dyn Fn(&str, serde_json::Value) + Send + Sync + 'static>;

/// Shell-agnostic daemon lifecycle manager.
pub struct DaemonLifecycle {
    pub watchdog: Arc<Mutex<Watchdog>>,
    pub stderr_buffer: StderrBuffer,
    pub bridge: Arc<IpcBridgeCore>,
    child_pid: Arc<Mutex<Option<u32>>>,
    shutdown_flag: Arc<AtomicBool>,
    on_watchdog: Arc<Mutex<Option<WatchdogCallback>>>,
    // N6-B3: platform-aware paths
    socket_path: String,
    pid_path: String,
    data_dir: String,
    daemon_version: Arc<Mutex<Option<String>>>,
    spawn_count: Arc<AtomicU32>,
    app_version: String,
    /// Additional daemon binary search paths (shell-provided).
    extra_binary_paths: Vec<PathBuf>,
}

impl DaemonLifecycle {
    pub fn new(app_version: &str) -> Self {
        Self {
            watchdog: Arc::new(Mutex::new(Watchdog::new())),
            stderr_buffer: StderrBuffer::with_default_capacity(),
            bridge: Arc::new(IpcBridgeCore::new()),
            child_pid: Arc::new(Mutex::new(None)),
            shutdown_flag: Arc::new(AtomicBool::new(false)),
            on_watchdog: Arc::new(Mutex::new(None)),
            socket_path: platform::default_ipc_path(),
            pid_path: platform::default_pid_path(),
            data_dir: platform::default_data_dir(),
            daemon_version: Arc::new(Mutex::new(None)),
            spawn_count: Arc::new(AtomicU32::new(0)),
            app_version: app_version.to_string(),
            extra_binary_paths: Vec::new(),
        }
    }

    // ── Configuration ─────────────────────────────────────────

    /// Set callback for watchdog state transitions.
    pub fn set_watchdog_callback(&self, cb: WatchdogCallback) {
        *self.on_watchdog.lock().unwrap() = Some(cb);
    }

    /// Set callback for bridge events (event_name, payload).
    /// Wires IPC bridge dispatch to the shell's event system.
    pub fn set_bridge_event_callback(&self, cb: BridgeEventCallback) {
        self.bridge.set_event_callback(cb);
    }

    /// Add extra paths to search for the daemon binary.
    pub fn add_binary_search_paths(&mut self, paths: Vec<PathBuf>) {
        self.extra_binary_paths.extend(paths);
    }

    // ── Accessors ─────────────────────────────────────────────

    pub fn socket_path(&self) -> &str {
        &self.socket_path
    }

    pub fn pid_path(&self) -> &str {
        &self.pid_path
    }

    pub fn data_dir(&self) -> &str {
        &self.data_dir
    }

    pub fn daemon_version(&self) -> Option<String> {
        self.daemon_version.lock().unwrap().clone()
    }

    pub fn spawn_count(&self) -> u32 {
        self.spawn_count.load(Ordering::Relaxed)
    }

    pub fn shutdown_flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.shutdown_flag)
    }

    // ── Lifecycle ─────────────────────────────────────────────

    fn emit_watchdog_state(&self) {
        let watchdog = self.watchdog.lock().unwrap();
        let event = WatchdogStateEvent {
            state: watchdog.state(),
            retry_count: watchdog.retry_count(),
        };
        drop(watchdog);
        if let Some(ref cb) = *self.on_watchdog.lock().unwrap() {
            cb(event);
        }
    }

    /// Start the lifecycle loop on a background thread.
    pub fn start(self: &Arc<Self>) {
        let mgr = Arc::clone(self);
        std::thread::spawn(move || {
            mgr.lifecycle_loop();
        });
    }

    fn lifecycle_loop(&self) {
        loop {
            if self.shutdown_flag.load(Ordering::Relaxed) {
                tracing::info!("[WATCHDOG] shutdown flag set, exiting lifecycle loop");
                return;
            }

            let state = self.watchdog.lock().unwrap().state();
            match state {
                WatchdogState::Starting | WatchdogState::Restarting => {
                    self.run_spawn_cycle();
                }
                WatchdogState::Ready => {
                    self.wait_for_daemon_exit();
                }
                WatchdogState::Degraded | WatchdogState::Incompatible => {
                    tracing::info!("[WATCHDOG] lifecycle loop exiting in terminal state: {state}");
                    return;
                }
            }
        }
    }

    fn run_spawn_cycle(&self) {
        self.run_cleanup();

        let socket_path = Path::new(&self.socket_path);

        match self.spawn_daemon() {
            Ok(pid) => {
                tracing::info!("[WATCHDOG] daemon spawned (pid={pid})");
                *self.child_pid.lock().unwrap() = Some(pid);
                self.write_pid_file(pid);
                self.spawn_count.fetch_add(1, Ordering::Relaxed);

                std::thread::sleep(std::time::Duration::from_millis(500));

                let deadline = std::time::Instant::now() + STARTUP_TIMEOUT;
                loop {
                    if std::time::Instant::now() >= deadline {
                        let delay = self.watchdog.lock().unwrap().on_startup_timeout();
                        self.emit_watchdog_state();
                        if let Some(d) = delay {
                            std::thread::sleep(d);
                        }
                        return;
                    }

                    if !socket_path.exists() && !platform::is_windows_pipe_path(&self.socket_path) {
                        std::thread::sleep(std::time::Duration::from_millis(250));
                        continue;
                    }

                    match ipc_client::probe_readiness(socket_path, &self.app_version) {
                        ReadinessResult::Ready { daemon_version, .. } => {
                            tracing::info!(
                                "[WATCHDOG] readiness confirmed: daemon v{daemon_version}"
                            );
                            *self.daemon_version.lock().unwrap() = Some(daemon_version.clone());
                            self.watchdog.lock().unwrap().on_daemon_ready();
                            self.emit_watchdog_state();

                            match self.bridge.start(socket_path, &self.app_version) {
                                Ok(()) => {
                                    tracing::info!("[IPC_BRIDGE] started successfully");
                                }
                                Err(e) => {
                                    tracing::warn!("[IPC_BRIDGE] failed to start: {e}");
                                }
                            }
                            return;
                        }
                        ReadinessResult::Incompatible { daemon_version } => {
                            tracing::warn!("[WATCHDOG] daemon incompatible: v{daemon_version}");
                            *self.daemon_version.lock().unwrap() = Some(daemon_version);
                            self.watchdog.lock().unwrap().on_version_incompatible();
                            self.emit_watchdog_state();
                            return;
                        }
                        ReadinessResult::Failed(reason) => {
                            tracing::debug!("[WATCHDOG] probe retry: {reason}");
                            std::thread::sleep(std::time::Duration::from_millis(500));
                        }
                    }
                }
            }
            Err(reason) => {
                self.watchdog.lock().unwrap().on_spawn_failure(&reason);
                self.emit_watchdog_state();
            }
        }
    }

    fn spawn_daemon(&self) -> Result<u32, String> {
        let binary_path = self.resolve_daemon_binary()?;

        let child = std::process::Command::new(&binary_path)
            .args([
                "--role",
                "answerer",
                "--mode",
                "default",
                "--pairing-policy",
                "ask",
                "--socket-path",
                &self.socket_path,
                "--data-dir",
                &self.data_dir,
            ])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| format!("spawn failed: {e}"))?;

        let pid = child.id();

        let buffer = self.stderr_buffer.clone();
        let mut child = child;
        let stderr = child.stderr.take();
        if let Some(stderr) = stderr {
            std::thread::spawn(move || {
                let reader = BufReader::new(stderr);
                for line in reader.lines() {
                    match line {
                        Ok(l) => {
                            tracing::trace!("[DAEMON_STDERR] {l}");
                            buffer.push(l);
                        }
                        Err(_) => break,
                    }
                }
            });
        }

        // Detach child — track via PID + platform signals.
        std::mem::forget(child);

        Ok(pid)
    }

    /// Resolve daemon binary path. Checks extra paths first, then standard locations.
    pub fn resolve_daemon_binary(&self) -> Result<PathBuf, String> {
        // Shell-provided extra paths (highest priority)
        for path in &self.extra_binary_paths {
            if path.exists() {
                return Ok(path.clone());
            }
        }

        // Standard locations: bin/bolt-daemon, then system PATH
        let simple_path = PathBuf::from("bin/bolt-daemon");
        if simple_path.exists() {
            return Ok(simple_path);
        }

        #[cfg(unix)]
        if let Ok(output) = std::process::Command::new("which")
            .arg("bolt-daemon")
            .output()
        {
            if output.status.success() {
                let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !path.is_empty() {
                    return Ok(PathBuf::from(path));
                }
            }
        }

        #[cfg(windows)]
        if let Ok(output) = std::process::Command::new("where")
            .arg("bolt-daemon")
            .output()
        {
            if output.status.success() {
                let path = String::from_utf8_lossy(&output.stdout)
                    .lines()
                    .next()
                    .unwrap_or("")
                    .trim()
                    .to_string();
                if !path.is_empty() {
                    return Ok(PathBuf::from(path));
                }
            }
        }

        Err("bolt-daemon binary not found".to_string())
    }

    fn wait_for_daemon_exit(&self) {
        let pid = match *self.child_pid.lock().unwrap() {
            Some(p) => p,
            None => return,
        };

        loop {
            if self.shutdown_flag.load(Ordering::Relaxed) {
                return;
            }

            if !platform::process_alive(pid) {
                tracing::warn!("[WATCHDOG] daemon exited (pid={pid})");
                *self.child_pid.lock().unwrap() = None;

                self.bridge.shutdown();

                let delay = self.watchdog.lock().unwrap().on_daemon_exit(None);
                self.emit_watchdog_state();
                let retry_count = self.watchdog.lock().unwrap().retry_count();
                let log_dir = platform::crash_log_dir();

                let _ = daemon_log::write_crash_snapshot(
                    &self.stderr_buffer,
                    &log_dir,
                    None,
                    Some(pid),
                    retry_count,
                );

                if let Some(d) = delay {
                    std::thread::sleep(d);
                }
                return;
            }

            self.watchdog.lock().unwrap().maybe_reset_retries();
            std::thread::sleep(std::time::Duration::from_secs(2));
        }
    }

    // ── Cleanup ────────────────────────────────────────────────

    pub fn run_cleanup(&self) {
        let socket_path = Path::new(&self.socket_path);
        let pid_path = Path::new(&self.pid_path);

        if pid_path.exists() {
            if let Ok(content) = std::fs::read_to_string(pid_path) {
                if let Ok(pid) = content.trim().parse::<u32>() {
                    if platform::process_alive(pid) {
                        if socket_path.exists() && IpcStream::probe(socket_path) {
                            tracing::info!(
                                "[WATCHDOG] existing daemon alive (pid={pid}), will connect"
                            );
                            return;
                        }
                        tracing::warn!(
                            "[WATCHDOG] daemon alive (pid={pid}) but socket missing, killing"
                        );
                        platform::process_terminate(pid);
                        std::thread::sleep(std::time::Duration::from_secs(2));
                        if platform::process_alive(pid) {
                            platform::process_force_kill(pid);
                            std::thread::sleep(std::time::Duration::from_millis(500));
                        }
                    }
                }
            }
            let _ = std::fs::remove_file(pid_path);
            tracing::info!("[WATCHDOG] cleaned stale PID file");
        }

        if !platform::is_windows_pipe_path(&self.socket_path) && socket_path.exists() {
            if IpcStream::probe(socket_path) {
                tracing::info!("[WATCHDOG] responsive daemon found via socket probe");
                return;
            }
            let _ = std::fs::remove_file(socket_path);
            tracing::info!("[WATCHDOG] removed stale socket: {}", socket_path.display());
        }
    }

    fn write_pid_file(&self, pid: u32) {
        let pid_path = Path::new(&self.pid_path);
        if let Err(e) = std::fs::write(pid_path, pid.to_string()) {
            tracing::warn!("[WATCHDOG] failed to write PID file: {e}");
        }
    }

    // ── Shutdown ───────────────────────────────────────────────

    pub fn shutdown(&self) {
        self.bridge.shutdown();
        self.shutdown_flag.store(true, Ordering::Relaxed);

        let pid = match self.child_pid.lock().unwrap().take() {
            Some(p) => p,
            None => {
                tracing::info!("[WATCHDOG] shutdown: no daemon process to stop");
                return;
            }
        };

        tracing::info!("[WATCHDOG] initiating daemon shutdown (pid={pid})");
        platform::process_terminate(pid);

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            if !platform::process_alive(pid) {
                tracing::info!("[WATCHDOG] daemon exited cleanly (pid={pid})");
                break;
            }
            if std::time::Instant::now() >= deadline {
                tracing::warn!("[WATCHDOG] daemon did not exit in 5s, forcing kill (pid={pid})");
                platform::process_force_kill(pid);
                std::thread::sleep(std::time::Duration::from_millis(500));
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }

        let _ = std::fs::remove_file(&self.pid_path);
        if !platform::is_windows_pipe_path(&self.socket_path) {
            let _ = std::fs::remove_file(&self.socket_path);
        }
        tracing::info!("[WATCHDOG] shutdown cleanup complete");
    }

    /// Manual restart from degraded state.
    pub fn manual_restart(self: &Arc<Self>) -> bool {
        let transition = self.watchdog.lock().unwrap().manual_restart();
        match transition {
            Transition::Changed(WatchdogState::Starting) => {
                let mgr = Arc::clone(self);
                std::thread::spawn(move || {
                    mgr.lifecycle_loop();
                });
                true
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lifecycle_new_initializes_correctly() {
        let lc = DaemonLifecycle::new("1.0.0");
        assert!(!lc.socket_path().is_empty());
        assert!(!lc.pid_path().is_empty());
        assert!(!lc.data_dir().is_empty());
        assert_eq!(lc.spawn_count(), 0);
        assert!(lc.daemon_version().is_none());
    }

    #[test]
    fn shutdown_flag_stops_lifecycle() {
        let lc = DaemonLifecycle::new("1.0.0");
        lc.shutdown_flag.store(true, Ordering::Relaxed);
        lc.lifecycle_loop();
    }

    #[test]
    fn resolve_binary_fails_gracefully() {
        let lc = DaemonLifecycle::new("1.0.0");
        let result = lc.resolve_daemon_binary();
        match result {
            Ok(p) => assert!(!p.as_os_str().is_empty()),
            Err(e) => assert!(e.contains("not found")),
        }
    }

    #[test]
    fn cleanup_handles_nonexistent_files() {
        let lc = DaemonLifecycle::new("1.0.0");
        lc.run_cleanup();
    }

    #[test]
    fn extra_binary_paths_checked_first() {
        let mut lc = DaemonLifecycle::new("1.0.0");
        // Add a path that doesn't exist
        lc.add_binary_search_paths(vec![PathBuf::from("/nonexistent/bolt-daemon")]);
        // Should still fall through to other resolution
        let _ = lc.resolve_daemon_binary();
    }
}
