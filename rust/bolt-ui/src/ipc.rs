//! IPC client for bolt-ui → daemon communication.
//!
//! Uses bolt-app-core IPC types and transport for cross-platform socket
//! communication. Performs NDJSON handshake, receives daemon events.

use std::io::{BufRead, BufReader, Write};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use bolt_app_core::ipc_transport::IpcStream;
use bolt_app_core::ipc_types::{IpcMessage, VersionHandshakePayload, VersionStatusPayload};

const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);
const APP_VERSION: &str = "0.0.1";

/// IPC event received from daemon (re-export shared type shape).
pub use bolt_app_core::ipc_types::IpcMessage as IpcEvent;

/// IPC client handle with bidirectional communication.
pub struct IpcClient {
    event_rx: mpsc::Receiver<IpcMessage>,
    writer: std::sync::Mutex<Box<dyn Write + Send>>,
    _reader_thread: thread::JoinHandle<()>,
}

impl IpcClient {
    /// Connect to daemon IPC socket and perform version handshake.
    /// Uses bolt-app-core cross-platform IPC transport (Unix socket / Windows pipe).
    pub fn connect(socket_path: &str) -> Result<Self, String> {
        let path = std::path::Path::new(socket_path);
        let stream = IpcStream::connect(path).map_err(|e| format!("IPC connect failed: {e}"))?;

        stream
            .set_read_timeout(Some(HANDSHAKE_TIMEOUT))
            .map_err(|e| format!("set timeout: {e}"))?;

        let mut writer = stream.try_clone().map_err(|e| format!("clone: {e}"))?;

        // Send version.handshake using shared IPC types
        let handshake = IpcMessage::new_decision(
            "version.handshake",
            serde_json::to_value(VersionHandshakePayload {
                app_version: APP_VERSION.to_string(),
            })
            .unwrap(),
        );
        let line = handshake
            .to_ndjson()
            .map_err(|e| format!("serialize: {e}"))?;
        writer
            .write_all(line.as_bytes())
            .map_err(|e| format!("write handshake: {e}"))?;
        writer.flush().map_err(|e| format!("flush: {e}"))?;

        // Read version.status
        let mut reader = BufReader::new(stream);
        let mut buf = String::new();
        reader
            .read_line(&mut buf)
            .map_err(|e| format!("read version.status: {e}"))?;

        let status: IpcMessage =
            serde_json::from_str(buf.trim()).map_err(|e| format!("parse version.status: {e}"))?;

        if status.msg_type != "version.status" {
            return Err(format!("expected version.status, got {}", status.msg_type));
        }

        let vs: VersionStatusPayload =
            serde_json::from_value(status.payload).map_err(|e| format!("parse payload: {e}"))?;
        if !vs.compatible {
            return Err("daemon version incompatible".into());
        }

        // Clear read timeout for event loop
        reader
            .get_ref()
            .set_read_timeout(None)
            .map_err(|e| format!("clear timeout: {e}"))?;

        // Start event reader thread
        let (tx, rx) = mpsc::channel();
        let reader_thread = thread::spawn(move || loop {
            let mut line = String::new();
            match reader.read_line(&mut line) {
                Ok(0) => break,
                Ok(_) => {
                    if let Ok(event) = serde_json::from_str::<IpcMessage>(line.trim()) {
                        if tx.send(event).is_err() {
                            break;
                        }
                    }
                }
                Err(_) => break,
            }
        });

        Ok(Self {
            event_rx: rx,
            writer: std::sync::Mutex::new(Box::new(writer)),
            _reader_thread: reader_thread,
        })
    }

    /// Try to receive the next event (non-blocking).
    pub fn try_recv(&self) -> Option<IpcMessage> {
        self.event_rx.try_recv().ok()
    }

    /// Send a command to the daemon via IPC.
    pub fn send_command(&self, msg_type: &str, payload: serde_json::Value) -> Result<(), String> {
        let msg = IpcMessage::new_decision(msg_type, payload);
        let line = msg.to_ndjson().map_err(|e| format!("serialize: {e}"))?;
        let mut w = self.writer.lock().map_err(|e| format!("lock: {e}"))?;
        w.write_all(line.as_bytes())
            .map_err(|e| format!("write: {e}"))?;
        w.flush().map_err(|e| format!("flush: {e}"))?;
        Ok(())
    }

    /// Receive all pending events.
    pub fn drain_events(&self) -> Vec<IpcMessage> {
        let mut events = Vec::new();
        while let Some(e) = self.try_recv() {
            events.push(e);
        }
        events
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ipc_message_deserialize() {
        let json = r#"{"id":"evt-0","kind":"event","type":"session.sas","ts_ms":123,"payload":{"sas":"123456","remote_identity_pk_b64":"abc"}}"#;
        let event: IpcMessage = serde_json::from_str(json).unwrap();
        assert_eq!(event.msg_type, "session.sas");
        assert_eq!(event.payload["sas"], "123456");
    }

    #[test]
    fn connect_to_nonexistent_socket_fails() {
        let result = IpcClient::connect("/tmp/nonexistent-bolt-ui-test.sock");
        assert!(result.is_err());
    }
}
