use std::sync::mpsc;
use std::time::Instant;

use eframe::egui;

use bolt_app_core::signaling_client::{self, DiscoveryEvent, Plane, SignalingConfig};

use crate::daemon::{self, DaemonProcess};
use crate::ipc::IpcClient;
use crate::screens;
use crate::state::*;
use crate::theme;

/// Rendezvous server address. Override with BOLT_RENDEZVOUS_URL env var.
/// Default: 127.0.0.1:3001 (embedded signal server started by app).
fn rendezvous_addr() -> String {
    if let Ok(url) = std::env::var("BOLT_RENDEZVOUS_URL") {
        return url;
    }
    "127.0.0.1:3001".to_string()
}

/// Get the local LAN IP address for the browser to reach us.
fn local_ip() -> String {
    // Try to determine LAN IP by connecting to a remote address (doesn't send data)
    if let Ok(socket) = std::net::UdpSocket::bind("0.0.0.0:0") {
        if socket.connect("8.8.8.8:80").is_ok() {
            if let Ok(addr) = socket.local_addr() {
                return addr.ip().to_string();
            }
        }
    }
    "127.0.0.1".to_string()
}

/// Signal pending until daemon WS endpoint is ready.
struct PendingWsSignal {
    peer_code: String,
    signal_type: String, // "connection_request" or "connection_accepted"
}

fn parse_device_type(s: &str) -> DeviceType {
    match s {
        "desktop" => DeviceType::Desktop,
        "laptop" => DeviceType::Laptop,
        "phone" => DeviceType::Phone,
        "tablet" => DeviceType::Tablet,
        _ => DeviceType::Browser,
    }
}

pub struct BoltApp {
    // ── Discovery state (primary) ────────────────────────────
    pub discovery: DiscoveryStatus,
    pub discovered_peers: Vec<DiscoveredPeer>,
    pub connected_peer: Option<ConnectedPeer>,

    // ── Connection lifecycle ─────────────────────────────────
    pub connection: ConnectionState,
    pub incoming_request: Option<IncomingRequest>,
    pub transfer: TransferState,
    pub verify: VerifyState,

    // ── Manual pairing fallback (secondary) ──────────────────
    pub show_manual_pair: bool,
    pub mode: ConnectMode,
    pub host_info: Option<HostInfo>,
    pub join_room: String,
    pub join_session: String,
    pub join_peer_code: String,

    // ── Runtime ──────────────────────────────────────────────
    pub local_peer_code: String,
    pub daemon_proc: Option<DaemonProcess>,
    pub ipc_client: Option<IpcClient>,
    pub prereq_error: Option<String>,
    pub signal_healthy: bool,
    /// Track which signaling planes are currently connected.
    local_plane_connected: bool,
    cloud_plane_connected: bool,
    daemon_bin: Option<std::path::PathBuf>,
    data_dir: String,
    socket_path: String,
    #[allow(dead_code)]
    cloud_signal_url: Option<String>,
    /// Port the daemon WS endpoint listens on (for direct browser connections).
    daemon_ws_port: u16,
    /// WT endpoint cert hash (captured from daemon stderr, SECURE-DIRECT-1).
    wt_cert_hash: Option<String>,
    /// Whether the WT endpoint has confirmed it is listening.
    wt_endpoint_ready: bool,
    /// Signal waiting to be sent once daemon WS endpoint is ready.
    pending_ws_signal: Option<PendingWsSignal>,

    // ── Signaling ────────────────────────────────────────────
    signaling_rx: mpsc::Receiver<DiscoveryEvent>,
    signaling_handle: signaling_client::SignalingHandle,
}

impl BoltApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::apply_theme(&cc.egui_ctx);

        let local_peer_code = bolt_core::peer_code::generate_secure_peer_code();
        let pid = std::process::id();
        let data_dir = format!("/tmp/bolt-ui-{pid}");
        let socket_path = format!("/tmp/bolt-ui-{pid}.sock");
        // Per-instance WS port for direct browser connections (9100 + pid hash to avoid collisions)
        let daemon_ws_port = 9100 + (pid % 900) as u16;

        let (daemon_bin, prereq_error) = match daemon::find_daemon_binary() {
            Ok(path) => (Some(path), None),
            Err(e) => (None, Some(e)),
        };

        let signal_healthy = bolt_app_core::signal_monitor::probe_signal_health();

        // Spawn real signaling client for peer discovery.
        let (tx, rx) = mpsc::channel();
        let device_name = hostname::get()
            .map(|h| h.to_string_lossy().to_string())
            .unwrap_or_else(|_| "Desktop".to_string());
        // Cloud signaling URL — matches website's VITE_SIGNAL_URL.
        // Override with BOLT_CLOUD_SIGNAL_URL env var.
        let cloud_url = std::env::var("BOLT_CLOUD_SIGNAL_URL")
            .ok()
            .or_else(|| Some("wss://bolt-rendezvous.fly.dev".to_string()));

        let cloud_url_for_storage = cloud_url.clone();
        let signaling_handle = signaling_client::spawn_signaling_client(
            SignalingConfig {
                server_url: rendezvous_addr(),
                cloud_url,
                peer_code: local_peer_code.clone(),
                device_name,
                device_type: "desktop".to_string(),
                wt_url: None,
                wt_cert_hash: None,
            },
            Box::new(move |event| {
                let _ = tx.send(event);
            }),
        );

        Self {
            discovery: if signal_healthy {
                DiscoveryStatus::Searching
            } else {
                DiscoveryStatus::Offline
            },
            discovered_peers: Vec::new(),
            connected_peer: None,
            connection: ConnectionState::Idle,
            incoming_request: None,
            transfer: TransferState::Idle,
            verify: VerifyState::NotStarted,
            show_manual_pair: false,
            mode: ConnectMode::Host,
            host_info: None,
            join_room: String::new(),
            join_session: String::new(),
            join_peer_code: String::new(),
            local_peer_code,
            daemon_proc: None,
            ipc_client: None,
            prereq_error,
            signal_healthy,
            local_plane_connected: false,
            cloud_plane_connected: false,
            daemon_bin,
            data_dir,
            socket_path,
            cloud_signal_url: cloud_url_for_storage,
            daemon_ws_port,
            wt_cert_hash: None,
            wt_endpoint_ready: false,
            pending_ws_signal: None,
            signaling_rx: rx,
            signaling_handle,
        }
    }

    /// Get the daemon's data directory path (used for signal files).
    pub fn data_dir(&self) -> &str {
        &self.data_dir
    }

    // ── Discovery actions ────────────────────────────────────

    /// Send a connection request to a discovered peer.
    /// Spawns daemon WS server first, includes wsUrl in the request.
    pub fn connect_to_peer(&mut self, peer: &DiscoveredPeer) {
        if self.connection != ConnectionState::Idle {
            return;
        }

        // Spawn daemon WS server (non-blocking)
        self.spawn_daemon_ws_server();

        // Store target peer info — signal will be sent from poll_daemon once WS is ready
        self.connection = ConnectionState::Requesting {
            peer_code: peer.peer_code.clone(),
            peer_name: peer.device_name.clone(),
            started_at: Instant::now(),
        };
        self.pending_ws_signal = Some(PendingWsSignal {
            peer_code: peer.peer_code.clone(),
            signal_type: "connection_request".into(),
        });

        tracing::info!(
            "[UI] daemon spawning, will send connection_request to {} when WS ready",
            peer.peer_code
        );
    }

    /// Wait for the daemon WS endpoint to be listening (max 5s).
    /// Blocking — only used in manual pair fallback, not the primary path.
    #[allow(dead_code)]
    fn wait_for_daemon_ws_ready(&self) -> bool {
        let deadline = Instant::now() + std::time::Duration::from_secs(5);
        while Instant::now() < deadline {
            if let Some(ref proc) = self.daemon_proc {
                if proc
                    .recent_stderr(30)
                    .iter()
                    .any(|l| l.contains("[WS_ENDPOINT] listening"))
                {
                    return true;
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        false
    }

    /// Kill the current daemon and reset WT cert state so the next daemon's
    /// cert hash will be captured fresh (SECURE-DIRECT-1 cert lifecycle fix).
    fn kill_daemon(&mut self) {
        if let Some(mut proc) = self.daemon_proc.take() {
            proc.kill();
        }
        self.wt_cert_hash = None;
        self.wt_endpoint_ready = false;
    }

    /// Spawn daemon as a direct WS endpoint server (for browser connections).
    fn spawn_daemon_ws_server(&mut self) {
        // Kill any existing daemon first
        self.kill_daemon();

        let daemon_bin = match &self.daemon_bin {
            Some(b) => b.clone(),
            None => {
                self.connection = ConnectionState::Error("Daemon not found".into());
                return;
            }
        };

        let ws_listen = format!("0.0.0.0:{}", self.daemon_ws_port);
        let _ = std::fs::create_dir_all(&self.data_dir);

        match DaemonProcess::spawn_ws_server(
            &daemon_bin,
            &ws_listen,
            &self.socket_path,
            &self.data_dir,
        ) {
            Ok(proc) => {
                self.daemon_proc = Some(proc);
                tracing::info!("[UI] daemon WS server started on {ws_listen}");
            }
            Err(e) => {
                self.connection = ConnectionState::Error(format!("Daemon spawn failed: {e}"));
                tracing::error!("[UI] daemon WS spawn failed: {e}");
            }
        }
    }

    /// Resolve the correct signaling URL for a peer based on its discovery plane.
    /// Used for manual pairing / rendezvous fallback path.
    #[allow(dead_code)]
    fn resolve_signal_url_for_peer(&self, peer_code: &str) -> String {
        let peer_plane = self
            .discovered_peers
            .iter()
            .find(|p| p.peer_code == peer_code)
            .map(|p| &p.plane);

        match peer_plane {
            Some(SignalingPlane::Cloud) => {
                // Peer discovered via cloud → daemon must use cloud signaling
                self.cloud_signal_url
                    .clone()
                    .unwrap_or_else(|| format!("ws://{}", rendezvous_addr()))
            }
            _ => {
                // Peer discovered locally → use local embedded server
                format!("ws://{}", rendezvous_addr())
            }
        }
    }

    /// Spawn daemon using peer's discovered signaling plane (rendezvous fallback).
    #[allow(dead_code)]
    fn spawn_daemon_for_peer(&mut self, peer_code: &str, peer_name: &str, is_offerer: bool) {
        let signal_url = self.resolve_signal_url_for_peer(peer_code);
        self.spawn_daemon_with_url(peer_code, peer_name, is_offerer, &signal_url);
    }

    /// Spawn daemon with an explicit signaling URL (rendezvous fallback).
    #[allow(dead_code)]
    fn spawn_daemon_with_url(
        &mut self,
        peer_code: &str,
        peer_name: &str,
        is_offerer: bool,
        signal_url: &str,
    ) {
        let daemon_bin = match &self.daemon_bin {
            Some(b) => b.clone(),
            None => {
                self.connection = ConnectionState::Error("Daemon not found".into());
                return;
            }
        };

        let room = daemon::generate_room_id();
        let session = daemon::generate_session_id();
        let _ = std::fs::create_dir_all(&self.data_dir);

        tracing::info!(
            "[UI] spawning daemon: signal_url={signal_url}, role={}, peer={peer_code}",
            if is_offerer { "offerer" } else { "answerer" }
        );

        let result = if is_offerer {
            DaemonProcess::spawn_join(
                &daemon_bin,
                &self.local_peer_code,
                peer_code,
                &room,
                &session,
                &self.socket_path,
                &self.data_dir,
                signal_url,
            )
        } else {
            DaemonProcess::spawn_host(
                &daemon_bin,
                &self.local_peer_code,
                peer_code,
                &room,
                &session,
                &self.socket_path,
                &self.data_dir,
                signal_url,
            )
        };

        match result {
            Ok(proc) => {
                self.daemon_proc = Some(proc);
                self.connection = ConnectionState::Establishing {
                    peer_code: peer_code.to_string(),
                    peer_name: peer_name.to_string(),
                    started_at: Instant::now(),
                };
                tracing::info!(
                    "[UI] daemon spawned as {} for peer {}",
                    if is_offerer { "offerer" } else { "answerer" },
                    peer_code
                );
            }
            Err(e) => {
                self.connection = ConnectionState::Error(format!("Daemon spawn failed: {e}"));
                tracing::error!("[UI] daemon spawn failed: {e}");
            }
        }
    }

    /// Accept an incoming connection request — spawn daemon WS server.
    pub fn accept_incoming(&mut self) {
        let req = match self.incoming_request.take() {
            Some(r) => r,
            None => return,
        };

        // Spawn daemon WS server (non-blocking)
        self.spawn_daemon_ws_server();

        // Store target — signal will be sent from poll_daemon once WS is ready
        self.pending_ws_signal = Some(PendingWsSignal {
            peer_code: req.peer_code.clone(),
            signal_type: "connection_accepted".into(),
        });

        self.connection = ConnectionState::Establishing {
            peer_code: req.peer_code.clone(),
            peer_name: req.device_name.clone(),
            started_at: Instant::now(),
        };

        self.connected_peer = Some(ConnectedPeer {
            peer_code: req.peer_code,
            device_name: req.device_name,
            device_type: req.device_type,
        });

        tracing::info!(
            "[UI] accepted — daemon spawning, will send connection_accepted when WS ready"
        );
    }

    /// Decline an incoming connection request.
    pub fn decline_incoming(&mut self) {
        if let Some(ref req) = self.incoming_request {
            self.signaling_handle.send_signal(
                &req.peer_code,
                "connection_declined",
                serde_json::json!({"reason": "user_declined"}),
                &self.local_peer_code,
            );
            tracing::info!("[UI] declined incoming connection from {}", req.peer_code);
        }
        self.incoming_request = None;
    }

    /// Cancel an outgoing connection request.
    pub fn cancel_request(&mut self) {
        if let ConnectionState::Requesting { ref peer_code, .. } = self.connection {
            self.signaling_handle.send_signal(
                peer_code,
                "connection_declined",
                serde_json::json!({"reason": "cancelled"}),
                &self.local_peer_code,
            );
        }
        self.kill_daemon();
        self.connection = ConnectionState::Idle;
    }

    // ── Manual pairing actions (fallback) ────────────────────

    pub fn start_host(&mut self) {
        let _daemon_bin = match &self.daemon_bin {
            Some(b) => b.clone(),
            None => {
                self.connection = ConnectionState::Error("Daemon not found".into());
                return;
            }
        };

        if !daemon::probe_rendezvous(&rendezvous_addr()) {
            self.connection = ConnectionState::Error(format!(
                "Signal server unreachable at {}",
                rendezvous_addr()
            ));
            return;
        }

        let room = daemon::generate_room_id();
        let session = daemon::generate_session_id();
        let peer_code = self.local_peer_code.clone();

        let _ = std::fs::create_dir_all(&self.data_dir);

        self.host_info = Some(HostInfo {
            peer_code,
            room,
            session,
        });
        self.connection = ConnectionState::Idle;
    }

    pub fn start_host_with_joiner(&mut self, joiner_code: &str) {
        let daemon_bin = match &self.daemon_bin {
            Some(b) => b.clone(),
            None => {
                self.connection = ConnectionState::Error("Daemon not found".into());
                return;
            }
        };

        let info = match &self.host_info {
            Some(i) => i.clone(),
            None => {
                self.connection = ConnectionState::Error("No host info".into());
                return;
            }
        };

        let _ = std::fs::create_dir_all(&self.data_dir);

        match DaemonProcess::spawn_host(
            &daemon_bin,
            &info.peer_code,
            joiner_code,
            &info.room,
            &info.session,
            &self.socket_path,
            &self.data_dir,
            &rendezvous_addr(),
        ) {
            Ok(proc) => {
                self.daemon_proc = Some(proc);
                self.connection = ConnectionState::Establishing {
                    peer_code: joiner_code.to_string(),
                    peer_name: joiner_code.to_string(),
                    started_at: Instant::now(),
                };
            }
            Err(e) => {
                self.connection = ConnectionState::Error(format!("Spawn failed: {e}"));
            }
        }
    }

    pub fn start_join(&mut self) {
        let daemon_bin = match &self.daemon_bin {
            Some(b) => b.clone(),
            None => {
                self.connection = ConnectionState::Error("Daemon not found".into());
                return;
            }
        };

        if !daemon::probe_rendezvous(&rendezvous_addr()) {
            self.connection = ConnectionState::Error(format!(
                "Signal server unreachable at {}",
                rendezvous_addr()
            ));
            return;
        }

        let _ = std::fs::create_dir_all(&self.data_dir);

        match DaemonProcess::spawn_join(
            &daemon_bin,
            &self.local_peer_code,
            &self.join_peer_code,
            &self.join_room,
            &self.join_session,
            &self.socket_path,
            &self.data_dir,
            &rendezvous_addr(),
        ) {
            Ok(proc) => {
                self.daemon_proc = Some(proc);
                self.connection = ConnectionState::Requesting {
                    peer_code: self.join_peer_code.clone(),
                    peer_name: self.join_peer_code.clone(),
                    started_at: Instant::now(),
                };
            }
            Err(e) => {
                self.connection = ConnectionState::Error(format!("Spawn failed: {e}"));
            }
        }
    }

    pub fn cancel_connect(&mut self) {
        self.kill_daemon();
        self.ipc_client = None;
        self.connection = ConnectionState::Idle;
        self.transfer = TransferState::Idle;
        self.verify = VerifyState::NotStarted;
        self.connected_peer = None;
    }

    pub fn disconnect(&mut self) {
        self.cancel_connect();
    }

    pub fn poll_daemon(&mut self) {
        if self.connection.is_timed_out() {
            let error_detail = self
                .daemon_proc
                .as_ref()
                .and_then(|p| p.last_error())
                .unwrap_or_else(|| "Connection timed out".into());
            self.kill_daemon();
            self.ipc_client = None;
            self.connection = ConnectionState::Error(error_detail);
            return;
        }

        if let Some(proc) = &mut self.daemon_proc {
            if !proc.is_running() {
                let error = proc
                    .last_error()
                    .unwrap_or_else(|| "Daemon exited unexpectedly".into());
                self.connection = ConnectionState::Error(error);
                self.daemon_proc = None;
                self.ipc_client = None;
                self.wt_cert_hash = None;
                self.wt_endpoint_ready = false;
                return;
            }

            let recent = proc.recent_stderr(30);

            // Capture WT cert hash from daemon stderr (SECURE-DIRECT-1 SD1).
            // Format: "[WT_CERT] hash=<64 hex chars>"
            if self.wt_cert_hash.is_none() {
                if let Some(hash) = recent.iter().find_map(|l| {
                    l.find("[WT_CERT] hash=")
                        .map(|idx| l[idx + 15..].trim().to_string())
                }) {
                    tracing::info!("[UI] captured WT cert hash: {hash}");
                    self.wt_cert_hash = Some(hash);
                }
            }

            // Detect WT endpoint ready (must see this before advertising wtUrl)
            if !self.wt_endpoint_ready
                && recent.iter().any(|l| l.contains("[WT_ENDPOINT] listening"))
            {
                self.wt_endpoint_ready = true;
                tracing::info!("[UI] WT endpoint ready");
            }

            // Check if daemon endpoints are ready — send pending signal.
            // Wait for WS to be listening. If WT cert hash was captured,
            // also wait for WT endpoint to be ready before sending signaling
            // (prevents browser from getting wtUrl before WT is listening).
            if let Some(ref pending) = self.pending_ws_signal {
                let ws_ready = recent.iter().any(|l| l.contains("[WS_ENDPOINT] listening"));
                let wt_needed = self.wt_cert_hash.is_some();
                let wt_ready = !wt_needed || self.wt_endpoint_ready;
                if ws_ready && wt_ready {
                    let ws_url = format!("ws://{}:{}", local_ip(), self.daemon_ws_port);
                    let device_name = hostname::get()
                        .map(|h| h.to_string_lossy().to_string())
                        .unwrap_or_else(|_| "Desktop".to_string());

                    // Build signaling payload with WS + optional WT endpoints
                    let wt_port = self.daemon_ws_port + 1;
                    let mut payload = serde_json::json!({
                        "deviceName": device_name,
                        "deviceType": "desktop",
                        "wsUrl": ws_url,
                    });
                    // Only advertise WT endpoint if it's confirmed listening
                    if self.wt_endpoint_ready {
                        if let Some(ref cert_hash) = self.wt_cert_hash {
                            let wt_url = format!("https://{}:{}", local_ip(), wt_port);
                            payload["wtUrl"] = serde_json::Value::String(wt_url);
                            payload["certHash"] = serde_json::Value::String(cert_hash.clone());
                        }
                    }

                    match pending.signal_type.as_str() {
                        "connection_request" => {
                            self.signaling_handle.send_signal(
                                &pending.peer_code,
                                "connection_request",
                                payload,
                                &self.local_peer_code,
                            );
                            tracing::info!("[UI] daemon WS ready — sent connection_request with wsUrl={ws_url}");
                        }
                        "connection_accepted" => {
                            // For accepted, strip deviceName/deviceType
                            let accept_payload = if self.wt_endpoint_ready {
                                if let Some(ref cert_hash) = self.wt_cert_hash {
                                    let wt_url = format!("https://{}:{}", local_ip(), wt_port);
                                    serde_json::json!({
                                        "wsUrl": ws_url,
                                        "wtUrl": wt_url,
                                        "certHash": cert_hash,
                                    })
                                } else {
                                    serde_json::json!({ "wsUrl": ws_url })
                                }
                            } else {
                                serde_json::json!({ "wsUrl": ws_url })
                            };
                            self.signaling_handle.send_signal(
                                &pending.peer_code,
                                "connection_accepted",
                                accept_payload,
                                &self.local_peer_code,
                            );
                            tracing::info!("[UI] daemon WS ready — sent connection_accepted with wsUrl={ws_url}");
                        }
                        _ => {}
                    }
                    self.pending_ws_signal = None;
                }
            }

            // Detect WS session establishment
            if self.connection.is_connecting()
                && recent.iter().any(|l| {
                    (l.contains("[WS_SESSION]") || l.contains("[WT_SESSION]"))
                        && l.contains("session established")
                })
            {
                tracing::info!("[UI] daemon session established — browser connected");
                self.connection = ConnectionState::Connected;
                // Determine verification mode from daemon HELLO logs.
                // Legacy HELLO (no identity) → transfer allowed immediately.
                // Identity HELLO → wait for user verification before transfer.
                let is_legacy = recent.iter().any(|l| l.contains("legacy HELLO"));
                if is_legacy {
                    self.verify = VerifyState::Legacy;
                    self.transfer = TransferState::Ready;
                } else {
                    // Identity mode: extract SAS from daemon if available via IPC,
                    // otherwise set pending with placeholder until IPC delivers it.
                    // For now, detect SAS from HELLO logs.
                    let sas = recent
                        .iter()
                        .find_map(|l| l.find("[SAS]").map(|idx| l[idx + 5..].trim().to_string()));
                    self.verify = VerifyState::Pending {
                        sas_code: sas.unwrap_or_else(|| "------".into()),
                    };
                    // Transfer stays Idle until user verifies
                    self.transfer = TransferState::Idle;
                }
            }

            // Detect session end (peer disconnected) from daemon stderr.
            // Daemon logs this when the WT/WS session ends and ACTIVE_SESSION is cleared.
            if matches!(self.connection, ConnectionState::Connected) {
                let session_ended = recent.iter().any(|l| {
                    l.contains("ACTIVE_SESSION cleared")
                        || l.contains("active session handle cleared")
                });
                if session_ended {
                    tracing::info!("[UI] daemon session ended — peer disconnected");
                    self.connection = ConnectionState::Idle;
                    self.transfer = TransferState::Idle;
                    self.verify = VerifyState::NotStarted;
                    self.connected_peer = None;
                    // Don't kill daemon — it can accept new connections
                    return;
                }
            }

            // Detect transfer activity from daemon stderr (WS or WT path)
            if matches!(self.connection, ConnectionState::Connected) {
                let is_transfer =
                    |l: &str| l.contains("[WS_TRANSFER]") || l.contains("[WT_TRANSFER]");
                for line in &recent {
                    // Detect incoming file transfer start
                    if is_transfer(line) && line.contains("receiving:") {
                        // Extract filename from log: "[WS/WT_TRANSFER] ... receiving: filename (size bytes, ...)"
                        let fname = line
                            .split("receiving:")
                            .nth(1)
                            .and_then(|s| s.trim().split('(').next())
                            .map(|s| s.trim().to_string())
                            .unwrap_or_else(|| "incoming file".into());
                        if matches!(self.transfer, TransferState::Ready | TransferState::Idle) {
                            self.transfer = TransferState::Receiving {
                                file_name: fname,
                                progress: 0.0,
                            };
                        }
                    }
                    // Update transfer progress
                    // Daemon format: "[WS_TRANSFER] ... progress: {done}/{total} chunks ({name})"
                    if is_transfer(line) && line.contains("progress:") {
                        if let Some(frac) = line.split("progress:").nth(1) {
                            let parts: Vec<&str> = frac.trim().splitn(2, '/').collect();
                            if parts.len() == 2 {
                                let done: f32 = parts[0].trim().parse().unwrap_or(0.0);
                                let total: f32 = parts[1]
                                    .split_whitespace()
                                    .next()
                                    .and_then(|s| s.parse().ok())
                                    .unwrap_or(1.0);
                                let pct = if total > 0.0 { done / total } else { 0.0 };
                                match &mut self.transfer {
                                    TransferState::Sending { progress, .. } => *progress = pct,
                                    TransferState::Receiving { progress, .. } => *progress = pct,
                                    _ => {}
                                }
                            }
                        }
                    }
                    // Detect file saved (receive complete)
                    // Daemon format: "[WS_TRANSFER] {peer} saved: {name} ({bytes} bytes) → {path}"
                    if is_transfer(line) && line.contains("saved:") {
                        let after_saved = line.split("saved:").nth(1).unwrap_or("");
                        let fname = after_saved
                            .trim()
                            .split('(')
                            .next()
                            .map(|s| s.trim().to_string())
                            .unwrap_or_else(|| "file".into());
                        let save_path = after_saved
                            .split('\u{2192}')
                            .nth(1) // → arrow
                            .map(|s| s.trim().to_string());
                        // Reveal in Finder on macOS
                        if let Some(ref path) = save_path {
                            #[cfg(target_os = "macos")]
                            {
                                let _ = std::process::Command::new("open")
                                    .arg("-R")
                                    .arg(path)
                                    .spawn();
                            }
                        }
                        self.transfer = TransferState::Complete {
                            file_name: fname,
                            save_path,
                        };
                    }
                    // Detect send complete
                    // Daemon format: "[WS_TRANSFER] all {N} chunks queued for {name}"
                    if is_transfer(line) && line.contains("chunks queued") {
                        if let TransferState::Sending { ref file_name, .. } = self.transfer {
                            let name = file_name.clone();
                            self.transfer = TransferState::Complete {
                                file_name: name,
                                save_path: None,
                            };
                        }
                    }
                }
            }

            // Try IPC connection for event forwarding
            if self.ipc_client.is_none()
                && proc
                    .recent_stderr(20)
                    .iter()
                    .any(|l| l.contains("[IPC] listening"))
            {
                if let Ok(client) = IpcClient::connect(&self.socket_path) {
                    self.ipc_client = Some(client);
                }
            }
        }

        if let Some(client) = &self.ipc_client {
            for event in client.drain_events() {
                match event.msg_type.as_str() {
                    "daemon.status" => {}
                    "session.connected" => {
                        let caps = event
                            .payload
                            .get("negotiated_capabilities")
                            .and_then(|v| v.as_array())
                            .map(|a| a.len())
                            .unwrap_or(0);
                        if caps > 0 {
                            self.connection = ConnectionState::Connected;
                            self.transfer = TransferState::Ready;
                        }
                    }
                    "session.sas" => {
                        if let Some(sas) = event.payload.get("sas").and_then(|v| v.as_str()) {
                            self.verify = VerifyState::Pending {
                                sas_code: sas.to_string(),
                            };
                        }
                    }
                    "session.error" => {
                        let reason = event
                            .payload
                            .get("reason")
                            .and_then(|v| v.as_str())
                            .unwrap_or("unknown error")
                            .to_string();
                        self.connection = ConnectionState::Error(reason);
                    }
                    "session.ended" => {
                        self.connection = ConnectionState::Idle;
                        self.transfer = TransferState::Idle;
                        self.verify = VerifyState::NotStarted;
                        self.connected_peer = None;
                    }
                    "transfer.started" => {
                        let file_name = event
                            .payload
                            .get("file_name")
                            .and_then(|v| v.as_str())
                            .unwrap_or("unknown")
                            .to_string();
                        let direction = event
                            .payload
                            .get("direction")
                            .and_then(|v| v.as_str())
                            .unwrap_or("receive");
                        if direction == "send" {
                            self.transfer = TransferState::Sending {
                                file_name,
                                progress: 0.0,
                            };
                        } else {
                            self.transfer = TransferState::Receiving {
                                file_name,
                                progress: 0.0,
                            };
                        }
                    }
                    "transfer.progress" => {
                        let progress = event
                            .payload
                            .get("progress")
                            .and_then(|v| v.as_f64())
                            .unwrap_or(0.0) as f32;
                        match &mut self.transfer {
                            TransferState::Sending { progress: p, .. }
                            | TransferState::Receiving { progress: p, .. } => {
                                *p = progress;
                            }
                            _ => {}
                        }
                    }
                    "transfer.complete" => {
                        let file_name = event
                            .payload
                            .get("file_name")
                            .and_then(|v| v.as_str())
                            .unwrap_or("unknown")
                            .to_string();
                        self.transfer = TransferState::Complete {
                            file_name,
                            save_path: None,
                        };
                    }
                    _ => {}
                }
            }
        }
    }
}

impl eframe::App for BoltApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.daemon_proc.is_some() {
            self.poll_daemon();
            ctx.request_repaint();
        }

        // ── Process signaling events ──────────────────────────
        while let Ok(event) = self.signaling_rx.try_recv() {
            match event {
                DiscoveryEvent::Connected(plane) => {
                    self.signal_healthy = true;
                    match plane {
                        Plane::Local => self.local_plane_connected = true,
                        Plane::Cloud => self.cloud_plane_connected = true,
                    }
                    self.discovery = DiscoveryStatus::Searching;
                    tracing::info!("[UI] signaling connected ({plane:?})");
                }
                DiscoveryEvent::Disconnected(_, plane) => {
                    let ui_plane = match plane {
                        Plane::Local => SignalingPlane::Local,
                        Plane::Cloud => SignalingPlane::Cloud,
                    };
                    match plane {
                        Plane::Local => self.local_plane_connected = false,
                        Plane::Cloud => self.cloud_plane_connected = false,
                    }
                    // Remove peers from the disconnected plane
                    self.discovered_peers.retain(|p| p.plane != ui_plane);
                    // Set truthful discovery status based on remaining connectivity
                    let any_plane = self.local_plane_connected || self.cloud_plane_connected;
                    if self.discovered_peers.is_empty() {
                        self.discovery = if any_plane {
                            DiscoveryStatus::Searching
                        } else {
                            self.signal_healthy = false;
                            DiscoveryStatus::Offline
                        };
                    }
                    tracing::warn!("[UI] signaling disconnected ({plane:?}), retained {} peers, planes: local={} cloud={}",
                        self.discovered_peers.len(), self.local_plane_connected, self.cloud_plane_connected);
                }
                DiscoveryEvent::PeerList(peers, plane) => {
                    let ui_plane = match plane {
                        Plane::Local => SignalingPlane::Local,
                        Plane::Cloud => SignalingPlane::Cloud,
                    };
                    // Filter self-discovery
                    for p in peers {
                        if p.peer_code == self.local_peer_code {
                            continue;
                        }
                        if !self
                            .discovered_peers
                            .iter()
                            .any(|dp| dp.peer_code == p.peer_code)
                        {
                            self.discovered_peers.push(DiscoveredPeer {
                                peer_code: p.peer_code,
                                device_name: p.device_name,
                                device_type: parse_device_type(&p.device_type),
                                plane: ui_plane.clone(),
                            });
                        }
                    }
                    self.discovery = if self.discovered_peers.is_empty() {
                        DiscoveryStatus::Searching
                    } else {
                        DiscoveryStatus::Active
                    };
                }
                DiscoveryEvent::PeerJoined(peer, plane) => {
                    if peer.peer_code == self.local_peer_code {
                        // skip self — can't use `continue` in match, so just don't add
                    } else {
                        let ui_plane = match plane {
                            Plane::Local => SignalingPlane::Local,
                            Plane::Cloud => SignalingPlane::Cloud,
                        };
                        if !self
                            .discovered_peers
                            .iter()
                            .any(|p| p.peer_code == peer.peer_code)
                        {
                            self.discovered_peers.push(DiscoveredPeer {
                                peer_code: peer.peer_code,
                                device_name: peer.device_name,
                                device_type: parse_device_type(&peer.device_type),
                                plane: ui_plane,
                            });
                        }
                        self.discovery = DiscoveryStatus::Active;
                    }
                }
                DiscoveryEvent::PeerLeft(code, _plane) => {
                    self.discovered_peers.retain(|p| p.peer_code != code);
                    if self.discovered_peers.is_empty() {
                        self.discovery = DiscoveryStatus::Searching;
                    }
                }
                DiscoveryEvent::Signal(sig, sig_plane) => {
                    let ui_plane = match sig_plane {
                        Plane::Local => SignalingPlane::Local,
                        Plane::Cloud => SignalingPlane::Cloud,
                    };
                    match sig.signal_type.as_str() {
                        "connection_request" => {
                            if self.connection != ConnectionState::Idle {
                                // Duplicate from same peer via other plane — ignore
                                let dominated = self
                                    .incoming_request
                                    .as_ref()
                                    .map(|r| r.peer_code == sig.from)
                                    .unwrap_or(false);
                                if !dominated {
                                    self.signaling_handle.send_signal(
                                        &sig.from,
                                        "connection_declined",
                                        serde_json::json!({"reason": "busy"}),
                                        &self.local_peer_code,
                                    );
                                }
                            } else {
                                let device_name = sig
                                    .data
                                    .get("deviceName")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("Unknown")
                                    .to_string();
                                let device_type_str = sig
                                    .data
                                    .get("deviceType")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("unknown");
                                self.incoming_request = Some(IncomingRequest {
                                    peer_code: sig.from.clone(),
                                    device_name,
                                    device_type: parse_device_type(device_type_str),
                                    plane: ui_plane,
                                });
                                tracing::info!(
                                    "[UI] incoming connection request from {} (plane: {:?})",
                                    sig.from,
                                    sig_plane
                                );
                            }
                        }
                        "connection_accepted" => {
                            if let ConnectionState::Requesting {
                                ref peer_code,
                                ref peer_name,
                                ..
                            } = self.connection
                            {
                                tracing::info!("[UI] connection accepted by {} — daemon WS server already running", sig.from);
                                // Daemon WS server was already spawned in connect_to_peer().
                                // Browser will now connect to it directly. Just update state.
                                let code = peer_code.clone();
                                let name = peer_name.clone();
                                self.connection = ConnectionState::Establishing {
                                    peer_code: code,
                                    peer_name: name,
                                    started_at: Instant::now(),
                                };
                            }
                        }
                        "connection_declined" => {
                            if let ConnectionState::Requesting { .. } = self.connection {
                                tracing::info!("[UI] connection declined by {}", sig.from);
                                self.connection =
                                    ConnectionState::Error("Connection declined".into());
                            }
                            if let Some(ref req) = self.incoming_request {
                                if req.peer_code == sig.from {
                                    self.incoming_request = None;
                                }
                            }
                        }
                        _ => {
                            tracing::debug!("[UI] unhandled signal type: {}", sig.signal_type);
                        }
                    }
                }
                DiscoveryEvent::Error(msg) => {
                    tracing::warn!("[UI] signaling error: {msg}");
                }
            }
        }

        // Always poll for signaling events. Without this, if the initial
        // probe_signal_health() fails (embedded server not yet bound),
        // signal_healthy stays false and Connected events sit unprocessed
        // in signaling_rx — the UI shows OFFLINE despite cloud signaling
        // being active with visible peers.
        ctx.request_repaint_after(std::time::Duration::from_millis(500));

        // ── Header strip (matches website: logo + NEARBY indicator) ──
        egui::TopBottomPanel::top("header")
            .frame(theme::header_frame())
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("\u{26A1} LocalBolt")
                            .size(theme::FONT_SIZE_HEADING)
                            .color(theme::ACCENT),
                    );

                    // Right-aligned discovery indicator (matches website header)
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let (color, pulse) = match self.discovery {
                            DiscoveryStatus::Active => (theme::ACCENT, false),
                            DiscoveryStatus::Searching => (theme::WARNING, true),
                            DiscoveryStatus::Offline => (theme::ERROR, true),
                        };
                        ui.label(
                            egui::RichText::new(self.discovery.label())
                                .size(theme::FONT_SIZE_SMALL)
                                .color(theme::TEXT_MUTED),
                        );
                        theme::status_dot(ui, color, pulse, self.discovery.label());
                    });
                });
            });

        // ── Main content: single card (matches website architecture) ──
        egui::CentralPanel::default()
            .frame(
                egui::Frame::NONE
                    .fill(theme::WINDOW_BG)
                    .inner_margin(theme::SPACING_XL),
            )
            .show(ctx, |ui| {
                screens::main_card::show(ui, self);
            });
    }
}

impl Drop for BoltApp {
    fn drop(&mut self) {
        if let Some(mut proc) = self.daemon_proc.take() {
            proc.kill();
        }
        let _ = std::fs::remove_file(&self.socket_path);
        let _ = std::fs::remove_dir_all(&self.data_dir);
    }
}
