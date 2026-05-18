//! WebSocket signaling client for peer discovery and connection approval.
//!
//! Connects to bolt-rendezvous, registers, receives discovery events,
//! and relays connection approval signals (connection_request/accepted/declined).
//! Shell-agnostic: callbacks for events, channel for outbound signals.

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tungstenite::{connect, Message};

// ── Wire types ───────────────────────────────────────────────

#[derive(Serialize)]
struct RegisterMsg {
    #[serde(rename = "type")]
    msg_type: &'static str,
    peer_code: String,
    device_name: String,
    device_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    wt_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    wt_cert_hash: Option<String>,
}

#[derive(Serialize)]
struct SignalOutMsg {
    #[serde(rename = "type")]
    msg_type: &'static str,
    to: String,
    payload: serde_json::Value,
}

#[derive(Serialize)]
struct PingMsg {
    #[serde(rename = "type")]
    msg_type: &'static str,
}

#[derive(Deserialize, Debug)]
struct ServerMsg {
    #[serde(rename = "type")]
    msg_type: String,
    #[serde(default)]
    peers: Option<Vec<PeerInfo>>,
    #[serde(default)]
    peer: Option<PeerInfo>,
    #[serde(default)]
    peer_code: Option<String>,
    #[serde(default)]
    from: Option<String>,
    #[serde(default)]
    payload: Option<serde_json::Value>,
    #[serde(default)]
    message: Option<String>,
}

/// Peer info from signaling server.
#[derive(Deserialize, Serialize, Debug, Clone, PartialEq, Eq)]
pub struct PeerInfo {
    pub peer_code: String,
    pub device_name: String,
    pub device_type: String,
    /// WebTransport URL (optional, desktop peers with WT enabled).
    #[serde(default)]
    pub wt_url: Option<String>,
    /// WebTransport TLS cert SHA-256 hash hex (optional).
    #[serde(default)]
    pub wt_cert_hash: Option<String>,
}

// ── Events (shell-facing) ────────────────────────────────────

/// Inbound signal payload (from another peer via relay).
#[derive(Debug, Clone)]
pub struct InboundSignal {
    pub from: String,
    pub signal_type: String,
    pub data: serde_json::Value,
}

/// Which signaling plane produced this event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Plane {
    Local,
    Cloud,
}

/// Events emitted by the signaling client.
#[derive(Debug, Clone)]
pub enum DiscoveryEvent {
    PeerList(Vec<PeerInfo>, Plane),
    PeerJoined(PeerInfo, Plane),
    PeerLeft(String, Plane),
    Connected(Plane),
    Disconnected(String, Plane),
    /// A relayed signal from another peer.
    Signal(InboundSignal, Plane),
    Error(String),
}

pub type DiscoveryCallback = Box<dyn Fn(DiscoveryEvent) + Send + Sync + 'static>;

// ── Outbound signal command ──────────────────────────────────

/// Command to send a signal to another peer.
#[derive(Debug, Clone)]
pub struct OutboundSignal {
    pub to: String,
    pub signal_type: String,
    pub data: serde_json::Value,
    pub from: String,
}

// ── Configuration ────────────────────────────────────────────

pub struct SignalingConfig {
    /// Local signaling server URL (embedded rendezvous, e.g. ws://127.0.0.1:3001)
    pub server_url: String,
    /// Optional cloud signaling URL (e.g. wss://bolt-rendezvous.fly.dev)
    pub cloud_url: Option<String>,
    pub peer_code: String,
    pub device_name: String,
    pub device_type: String,
    /// WebTransport URL to advertise to peers (optional).
    pub wt_url: Option<String>,
    /// WebTransport TLS cert SHA-256 hash hex to advertise (optional).
    pub wt_cert_hash: Option<String>,
}

// ── Client handle ────────────────────────────────────────────

/// Handle for the running signaling client.
/// Used to send signals and shut down.
pub struct SignalingHandle {
    pub shutdown: Arc<AtomicBool>,
    pub send_tx: std::sync::mpsc::Sender<OutboundSignal>,
    pub cloud_send_tx: Option<std::sync::mpsc::Sender<OutboundSignal>>,
}

impl SignalingHandle {
    /// Send a signal to another peer via all connected signaling servers.
    pub fn send_signal(&self, to: &str, signal_type: &str, data: serde_json::Value, from: &str) {
        let sig = OutboundSignal {
            to: to.to_string(),
            signal_type: signal_type.to_string(),
            data,
            from: from.to_string(),
        };
        let _ = self.send_tx.send(sig.clone());
        if let Some(ref cloud_tx) = self.cloud_send_tx {
            let _ = cloud_tx.send(sig);
        }
    }

    pub fn shutdown(&self) {
        self.shutdown.store(true, Ordering::Relaxed);
    }
}

// ── Client loop ──────────────────────────────────────────────

/// Spawn dual signaling clients (local + optional cloud) on background threads.
/// Both feed events into the same callback. Outbound signals are sent to both.
pub fn spawn_signaling_client(
    config: SignalingConfig,
    on_event: DiscoveryCallback,
) -> SignalingHandle {
    let shutdown = Arc::new(AtomicBool::new(false));
    let (send_tx, send_rx) = std::sync::mpsc::channel::<OutboundSignal>();

    let on_event = Arc::new(on_event);

    // Local signaling thread
    let shutdown_local = Arc::clone(&shutdown);
    let on_event_local = Arc::clone(&on_event);
    let local_config = SignalingConfig {
        server_url: config.server_url.clone(),
        cloud_url: None,
        peer_code: config.peer_code.clone(),
        device_name: config.device_name.clone(),
        device_type: config.device_type.clone(),
        wt_url: config.wt_url.clone(),
        wt_cert_hash: config.wt_cert_hash.clone(),
    };
    std::thread::spawn(move || {
        run_signaling_client(
            local_config,
            shutdown_local,
            Box::new(move |e| on_event_local(e)),
            send_rx,
            Plane::Local,
        );
    });

    // Cloud signaling thread (if configured)
    if let Some(cloud_url) = config.cloud_url {
        if !cloud_url.is_empty() {
            let shutdown_cloud = Arc::clone(&shutdown);
            let on_event_cloud = Arc::clone(&on_event);
            let (cloud_tx, cloud_rx) = std::sync::mpsc::channel::<OutboundSignal>();

            let cloud_config = SignalingConfig {
                server_url: cloud_url,
                cloud_url: None,
                peer_code: config.peer_code.clone(),
                device_name: config.device_name.clone(),
                device_type: config.device_type.clone(),
                wt_url: config.wt_url.clone(),
                wt_cert_hash: config.wt_cert_hash.clone(),
            };
            std::thread::spawn(move || {
                run_signaling_client(
                    cloud_config,
                    shutdown_cloud,
                    Box::new(move |e| on_event_cloud(e)),
                    cloud_rx,
                    Plane::Cloud,
                );
            });

            // Return handle that sends to both local and cloud
            return SignalingHandle {
                shutdown,
                send_tx, // local gets original send_rx
                cloud_send_tx: Some(cloud_tx),
            };
        }
    }

    SignalingHandle {
        shutdown,
        send_tx,
        cloud_send_tx: None,
    }
}

/// Set read timeout on a WebSocket stream regardless of TLS wrapping.
fn set_stream_timeout(
    socket: &tungstenite::WebSocket<tungstenite::stream::MaybeTlsStream<std::net::TcpStream>>,
    timeout: Duration,
) {
    match socket.get_ref() {
        tungstenite::stream::MaybeTlsStream::Plain(s) => {
            let _ = s.set_read_timeout(Some(timeout));
        }
        tungstenite::stream::MaybeTlsStream::NativeTls(tls) => {
            let _ = tls.get_ref().set_read_timeout(Some(timeout));
        }
        _ => {} // other TLS backends
    }
}

fn run_signaling_client(
    config: SignalingConfig,
    shutdown: Arc<AtomicBool>,
    on_event: DiscoveryCallback,
    send_rx: std::sync::mpsc::Receiver<OutboundSignal>,
    plane: Plane,
) {
    let ws_url =
        if config.server_url.starts_with("ws://") || config.server_url.starts_with("wss://") {
            config.server_url.clone()
        } else {
            format!("ws://{}", config.server_url)
        };

    loop {
        if shutdown.load(Ordering::Relaxed) {
            return;
        }

        tracing::info!("[SIGNALING] connecting to {ws_url}");

        match connect(&ws_url) {
            Ok((mut socket, _response)) => {
                tracing::info!("[SIGNALING] connected");
                on_event(DiscoveryEvent::Connected(plane));

                // Set read timeout so we can check outbound queue + shutdown
                set_stream_timeout(&socket, Duration::from_millis(100));

                // Register
                let register = RegisterMsg {
                    msg_type: "register",
                    peer_code: config.peer_code.clone(),
                    device_name: config.device_name.clone(),
                    device_type: config.device_type.clone(),
                    wt_url: config.wt_url.clone(),
                    wt_cert_hash: config.wt_cert_hash.clone(),
                };
                if let Err(e) =
                    socket.send(Message::Text(serde_json::to_string(&register).unwrap()))
                {
                    tracing::warn!("[SIGNALING] register failed: {e}");
                    on_event(DiscoveryEvent::Disconnected(
                        format!("register: {e}"),
                        plane,
                    ));
                    reconnect_delay(&shutdown);
                    continue;
                }

                // Set read timeout for main loop
                set_stream_timeout(&socket, Duration::from_millis(200));

                let mut ping_ticks = 0u32;

                // Main loop: read inbound, flush outbound
                loop {
                    if shutdown.load(Ordering::Relaxed) {
                        let _ = socket.close(None);
                        return;
                    }

                    // Read inbound messages
                    match socket.read() {
                        Ok(Message::Text(text)) => {
                            if let Ok(msg) = serde_json::from_str::<ServerMsg>(&text) {
                                dispatch_message(&msg, &on_event, plane);
                            }
                            ping_ticks = 0;
                        }
                        Ok(Message::Ping(data)) => {
                            let _ = socket.send(Message::Pong(data));
                        }
                        Ok(Message::Close(_)) => {
                            on_event(DiscoveryEvent::Disconnected("server closed".into(), plane));
                            break;
                        }
                        Ok(_) => {}
                        Err(tungstenite::Error::Io(ref e))
                            if e.kind() == std::io::ErrorKind::WouldBlock
                                || e.kind() == std::io::ErrorKind::TimedOut => {}
                        Err(e) => {
                            tracing::warn!("[SIGNALING] read error: {e}");
                            on_event(DiscoveryEvent::Disconnected(format!("{e}"), plane));
                            break;
                        }
                    }

                    // Flush outbound signals
                    while let Ok(cmd) = send_rx.try_recv() {
                        let payload = serde_json::json!({
                            "type": cmd.signal_type,
                            "data": cmd.data,
                            "from": cmd.from,
                            "to": cmd.to,
                        });
                        let msg = SignalOutMsg {
                            msg_type: "signal",
                            to: cmd.to.clone(),
                            payload,
                        };
                        if let Err(e) =
                            socket.send(Message::Text(serde_json::to_string(&msg).unwrap()))
                        {
                            tracing::warn!("[SIGNALING] send signal failed: {e}");
                            break;
                        }
                        tracing::info!("[SIGNALING] sent {} to {}", cmd.signal_type, cmd.to);
                    }

                    // Keepalive ping every ~30s (150 ticks at 200ms)
                    ping_ticks += 1;
                    if ping_ticks >= 150 {
                        let _ = socket.send(Message::Text(
                            serde_json::to_string(&PingMsg { msg_type: "ping" }).unwrap(),
                        ));
                        ping_ticks = 0;
                    }
                }
            }
            Err(e) => {
                tracing::warn!("[SIGNALING] connect failed: {e}");
                on_event(DiscoveryEvent::Disconnected(format!("connect: {e}"), plane));
            }
        }

        reconnect_delay(&shutdown);
    }
}

fn dispatch_message(msg: &ServerMsg, on_event: &DiscoveryCallback, plane: Plane) {
    match msg.msg_type.as_str() {
        "peers" => {
            if let Some(ref peers) = msg.peers {
                on_event(DiscoveryEvent::PeerList(peers.clone(), plane));
            }
        }
        "peer_joined" => {
            if let Some(ref peer) = msg.peer {
                on_event(DiscoveryEvent::PeerJoined(peer.clone(), plane));
            }
        }
        "peer_left" => {
            if let Some(ref code) = msg.peer_code {
                on_event(DiscoveryEvent::PeerLeft(code.clone(), plane));
            }
        }
        "signal" => {
            if let (Some(ref from), Some(ref payload)) = (&msg.from, &msg.payload) {
                let signal_type = payload
                    .get("type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                let data = payload
                    .get("data")
                    .cloned()
                    .unwrap_or(serde_json::Value::Null);

                on_event(DiscoveryEvent::Signal(
                    InboundSignal {
                        from: from.clone(),
                        signal_type,
                        data,
                    },
                    plane,
                ));
            }
        }
        "error" => {
            let message = msg.message.as_deref().unwrap_or("unknown error");
            tracing::warn!("[SIGNALING] server error: {message}");
            on_event(DiscoveryEvent::Error(message.to_string()));
        }
        _ => {}
    }
}

fn reconnect_delay(shutdown: &AtomicBool) {
    for _ in 0..10 {
        if shutdown.load(Ordering::Relaxed) {
            return;
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_msg_serializes() {
        let msg = RegisterMsg {
            msg_type: "register",
            peer_code: "ABC123".into(),
            device_name: "Test".into(),
            device_type: "desktop".into(),
            wt_url: None,
            wt_cert_hash: None,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"register\""));
        assert!(json.contains("\"peer_code\":\"ABC123\""));
    }

    #[test]
    fn signal_out_serializes() {
        let msg = SignalOutMsg {
            msg_type: "signal",
            to: "PEER1".into(),
            payload: serde_json::json!({"type": "connection_request", "from": "ME"}),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"signal\""));
        assert!(json.contains("\"to\":\"PEER1\""));
    }

    #[test]
    fn peers_msg_deserializes() {
        let json = r#"{"type":"peers","peers":[{"peer_code":"X","device_name":"Phone","device_type":"phone"}]}"#;
        let msg: ServerMsg = serde_json::from_str(json).unwrap();
        assert_eq!(msg.msg_type, "peers");
        assert_eq!(msg.peers.unwrap().len(), 1);
    }

    #[test]
    fn signal_relay_deserializes() {
        let json = r#"{"type":"signal","from":"ALICE","payload":{"type":"connection_request","data":{"deviceName":"Phone","deviceType":"phone"},"from":"ALICE","to":"BOB"}}"#;
        let msg: ServerMsg = serde_json::from_str(json).unwrap();
        assert_eq!(msg.msg_type, "signal");
        assert_eq!(msg.from.unwrap(), "ALICE");
        let payload = msg.payload.unwrap();
        assert_eq!(payload["type"], "connection_request");
    }

    #[test]
    fn peer_left_deserializes() {
        let json = r#"{"type":"peer_left","peer_code":"Z"}"#;
        let msg: ServerMsg = serde_json::from_str(json).unwrap();
        assert_eq!(msg.peer_code.unwrap(), "Z");
    }
}
