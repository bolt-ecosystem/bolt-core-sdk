//! LocalBolt Desktop — standalone native application.
//!
//! NATIVE-DESKTOP-PKG-1: Self-contained desktop app with:
//! - Embedded rendezvous/signaling server (port 3001)
//! - Daemon binary resolution and per-session lifecycle
//! - egui native UI (no WebView)
//! - Signal health monitoring
//!
//! No Tauri dependency. No external signal server required.

pub mod app;
pub mod daemon;
pub mod ipc;
mod screens;
pub mod state;
mod theme;

use std::net::SocketAddr;

use bolt_rendezvous::SignalingServer;
use tracing_subscriber::EnvFilter;

/// Spawn the embedded signaling server on a background thread.
/// Runs on 0.0.0.0:3001 so other devices on the LAN can discover this app.
/// Panic-guarded: thread panic is caught and logged, does not crash the app.
fn start_embedded_signal_server() {
    std::thread::spawn(|| {
        let result = std::panic::catch_unwind(|| {
            let rt = tokio::runtime::Runtime::new().expect("failed to create tokio runtime");
            rt.block_on(async {
                let addr: SocketAddr = "0.0.0.0:3001".parse().unwrap();
                let server = SignalingServer::new(addr);
                tracing::info!("[SIGNAL] embedded server starting on {addr}");
                if let Err(e) = server.run().await {
                    tracing::error!("[SIGNAL] server error: {e}");
                }
            });
        });
        if let Err(e) = result {
            tracing::error!("[SIGNAL] PANIC in signaling server thread: {e:?}");
        }
    });
}

fn main() -> eframe::Result<()> {
    // Initialize structured logging
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    tracing::info!(
        "[BOLT_UI] starting LocalBolt Desktop v{}",
        env!("CARGO_PKG_VERSION")
    );

    // Start embedded signal server before UI
    start_embedded_signal_server();

    // Brief delay for signal server to bind
    std::thread::sleep(std::time::Duration::from_millis(200));

    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([520.0, 700.0])
            .with_min_inner_size([400.0, 550.0])
            .with_title("LocalBolt"),
        ..Default::default()
    };

    eframe::run_native(
        "LocalBolt",
        options,
        Box::new(|cc| Ok(Box::new(app::BoltApp::new(cc)))),
    )
}
