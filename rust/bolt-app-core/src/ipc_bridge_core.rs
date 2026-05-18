//! Shell-agnostic IPC bridge for daemon event forwarding.
//!
//! Persistent connection to daemon: version handshake, event dispatch
//! via callback, decision relay. No Tauri dependency.
//!
//! Extracted from localbolt-app ipc_bridge.rs (NATIVE-APP-CORE-1).

use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::ipc_transport::IpcStream;
use crate::ipc_types::{
    DaemonStatusPayload, IpcKind, IpcMessage, PairingRequestPayload,
    TransferIncomingRequestPayload, VersionHandshakePayload, VersionStatusPayload,
};

/// Read timeout during handshake phase.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);

/// Read timeout during event loop.
const EVENT_LOOP_TIMEOUT: Duration = Duration::from_secs(5);

/// Callback for dispatching parsed IPC events to the shell.
/// Arguments: (event_name, payload_json).
pub type EventCallback = Box<dyn Fn(&str, serde_json::Value) + Send + Sync + 'static>;

/// Shell-agnostic IPC bridge.
pub struct IpcBridgeCore {
    writer: Arc<Mutex<Option<IpcStream>>>,
    shutdown: Arc<AtomicBool>,
    event_callback: Arc<Mutex<Option<EventCallback>>>,
}

impl IpcBridgeCore {
    pub fn new() -> Self {
        Self {
            writer: Arc::new(Mutex::new(None)),
            shutdown: Arc::new(AtomicBool::new(false)),
            event_callback: Arc::new(Mutex::new(None)),
        }
    }

    /// Set the event dispatch callback.
    /// Shell implementations wire this to their event system.
    pub fn set_event_callback(&self, cb: EventCallback) {
        *self.event_callback.lock().unwrap() = Some(cb);
    }

    /// Establish persistent connection and start event forwarding.
    pub fn start(&self, socket_path: &Path, app_version: &str) -> Result<(), String> {
        let stream = IpcStream::connect(socket_path).map_err(|e| format!("bridge connect: {e}"))?;
        stream
            .set_read_timeout(Some(HANDSHAKE_TIMEOUT))
            .map_err(|e| format!("set_read_timeout: {e}"))?;
        stream
            .set_write_timeout(Some(HANDSHAKE_TIMEOUT))
            .map_err(|e| format!("set_write_timeout: {e}"))?;

        let write_stream = stream
            .try_clone()
            .map_err(|e| format!("clone writer: {e}"))?;
        let mut writer = stream
            .try_clone()
            .map_err(|e| format!("clone for handshake: {e}"))?;
        let reader = BufReader::new(stream);

        // Step 1: Send version.handshake
        let handshake = IpcMessage::new_decision(
            "version.handshake",
            serde_json::to_value(VersionHandshakePayload {
                app_version: app_version.to_string(),
            })
            .unwrap(),
        );
        let line = handshake
            .to_ndjson()
            .map_err(|e| format!("serialize handshake: {e}"))?;
        writer
            .write_all(line.as_bytes())
            .map_err(|e| format!("write handshake: {e}"))?;
        writer
            .flush()
            .map_err(|e| format!("flush handshake: {e}"))?;

        // Step 2: Read version.status
        let mut reader = reader;
        let mut buf = String::new();
        reader
            .read_line(&mut buf)
            .map_err(|e| format!("read version.status: {e}"))?;
        let vs_msg: IpcMessage =
            serde_json::from_str(buf.trim()).map_err(|e| format!("parse version.status: {e}"))?;
        if vs_msg.msg_type != "version.status" || vs_msg.kind != IpcKind::Event {
            return Err(format!(
                "expected version.status event, got {}:{:?}",
                vs_msg.msg_type, vs_msg.kind
            ));
        }
        let vs: VersionStatusPayload = serde_json::from_value(vs_msg.payload)
            .map_err(|e| format!("parse version.status payload: {e}"))?;
        if !vs.compatible {
            return Err(format!(
                "bridge version incompatible: daemon={}",
                vs.daemon_version
            ));
        }
        tracing::info!("[IPC_BRIDGE] handshake ok: daemon v{}", vs.daemon_version);

        // Step 3: Read initial daemon.status
        buf.clear();
        reader
            .read_line(&mut buf)
            .map_err(|e| format!("read daemon.status: {e}"))?;
        let ds_msg: IpcMessage =
            serde_json::from_str(buf.trim()).map_err(|e| format!("parse daemon.status: {e}"))?;
        if ds_msg.msg_type == "daemon.status" {
            if let Ok(payload) = serde_json::from_value::<DaemonStatusPayload>(ds_msg.payload) {
                self.emit_event(
                    "daemon://status-update",
                    serde_json::to_value(&payload).unwrap_or_default(),
                );
                tracing::info!(
                    "[IPC_BRIDGE] initial status: peers={}",
                    payload.connected_peers
                );
            }
        }

        let _ = write_stream.set_read_timeout(Some(EVENT_LOOP_TIMEOUT));

        *self.writer.lock().unwrap() = Some(write_stream);

        let shutdown = Arc::clone(&self.shutdown);
        let event_cb = Arc::clone(&self.event_callback);
        std::thread::spawn(move || {
            Self::event_loop(reader, &event_cb, &shutdown);
            tracing::info!("[IPC_BRIDGE] reader thread exiting");
        });

        Ok(())
    }

    fn event_loop(
        mut reader: BufReader<IpcStream>,
        event_cb: &Arc<Mutex<Option<EventCallback>>>,
        shutdown: &AtomicBool,
    ) {
        let mut buf = String::new();
        loop {
            if shutdown.load(Ordering::Relaxed) {
                break;
            }
            buf.clear();
            match reader.read_line(&mut buf) {
                Ok(0) => {
                    tracing::warn!("[IPC_BRIDGE] daemon disconnected (EOF)");
                    if let Some(ref cb) = *event_cb.lock().unwrap() {
                        cb("daemon://bridge-disconnected", serde_json::Value::Null);
                    }
                    break;
                }
                Ok(_) => {
                    Self::dispatch_event(event_cb, buf.trim());
                }
                Err(ref e)
                    if e.kind() == std::io::ErrorKind::WouldBlock
                        || e.kind() == std::io::ErrorKind::TimedOut =>
                {
                    continue;
                }
                Err(e) => {
                    tracing::warn!("[IPC_BRIDGE] read error: {e}");
                    if let Some(ref cb) = *event_cb.lock().unwrap() {
                        cb("daemon://bridge-disconnected", serde_json::Value::Null);
                    }
                    break;
                }
            }
        }
    }

    fn dispatch_event(event_cb: &Arc<Mutex<Option<EventCallback>>>, line: &str) {
        let msg: IpcMessage = match serde_json::from_str(line) {
            Ok(m) => m,
            Err(e) => {
                tracing::debug!("[IPC_BRIDGE] parse error: {e}");
                return;
            }
        };

        if msg.kind != IpcKind::Event {
            tracing::debug!("[IPC_BRIDGE] ignoring non-event: {:?}", msg.kind);
            return;
        }

        let cb_guard = event_cb.lock().unwrap();
        let Some(ref cb) = *cb_guard else { return };

        match msg.msg_type.as_str() {
            "daemon.status" => {
                if let Ok(payload) = serde_json::from_value::<DaemonStatusPayload>(msg.payload) {
                    cb(
                        "daemon://status-update",
                        serde_json::to_value(&payload).unwrap_or_default(),
                    );
                }
            }
            "pairing.request" => {
                if let Ok(payload) = serde_json::from_value::<PairingRequestPayload>(msg.payload) {
                    tracing::info!(
                        "[IPC_BRIDGE] pairing request from {}",
                        payload.remote_device_name
                    );
                    cb(
                        "daemon://pairing-request",
                        serde_json::to_value(&payload).unwrap_or_default(),
                    );
                }
            }
            "transfer.incoming.request" => {
                if let Ok(payload) =
                    serde_json::from_value::<TransferIncomingRequestPayload>(msg.payload)
                {
                    tracing::info!(
                        "[IPC_BRIDGE] transfer request: {} ({} bytes)",
                        payload.file_name,
                        payload.file_size_bytes
                    );
                    cb(
                        "daemon://transfer-request",
                        serde_json::to_value(&payload).unwrap_or_default(),
                    );
                }
            }
            // Session lifecycle events
            "session.connected" | "session.sas" | "session.ended" | "session.error" => {
                let event_name = format!("daemon://{}", msg.msg_type.replace('.', "-"));
                tracing::info!("[IPC_BRIDGE] session event: {}", msg.msg_type);
                cb(&event_name, msg.payload);
            }
            // Transfer lifecycle events
            "transfer.started" | "transfer.progress" | "transfer.complete" | "transfer.error"
            | "transfer.paused" | "transfer.resumed" => {
                let event_name = format!("daemon://{}", msg.msg_type.replace('.', "-"));
                tracing::info!("[IPC_BRIDGE] transfer event: {}", msg.msg_type);
                cb(&event_name, msg.payload);
            }
            other => {
                tracing::debug!("[IPC_BRIDGE] unhandled event type: {other}");
            }
        }
    }

    fn emit_event(&self, name: &str, payload: serde_json::Value) {
        if let Some(ref cb) = *self.event_callback.lock().unwrap() {
            cb(name, payload);
        }
    }

    /// Send a decision message to the daemon.
    pub fn send_decision(&self, msg: IpcMessage) -> Result<(), String> {
        let mut guard = self.writer.lock().unwrap();
        let writer = guard.as_mut().ok_or("bridge not connected")?;
        let line = msg
            .to_ndjson()
            .map_err(|e| format!("serialize decision: {e}"))?;
        writer
            .write_all(line.as_bytes())
            .map_err(|e| format!("write decision: {e}"))?;
        writer.flush().map_err(|e| format!("flush decision: {e}"))?;
        Ok(())
    }

    pub fn is_connected(&self) -> bool {
        self.writer.lock().unwrap().is_some()
    }

    pub fn shutdown(&self) {
        self.shutdown.store(true, Ordering::Relaxed);
        *self.writer.lock().unwrap() = None;
    }
}

impl Default for IpcBridgeCore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bridge_initially_not_connected() {
        let bridge = IpcBridgeCore::new();
        assert!(!bridge.is_connected());
    }

    #[test]
    fn bridge_shutdown_idempotent() {
        let bridge = IpcBridgeCore::new();
        bridge.shutdown();
        bridge.shutdown();
        assert!(!bridge.is_connected());
    }

    #[test]
    fn send_decision_fails_when_not_connected() {
        let bridge = IpcBridgeCore::new();
        let msg = IpcMessage::new_decision(
            "pairing.decision",
            serde_json::json!({"request_id": "evt-0", "decision": "deny_once"}),
        );
        let result = bridge.send_decision(msg);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not connected"));
    }
}
