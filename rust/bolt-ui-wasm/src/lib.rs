#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]

// EW2 measurement PoC — browser egui shell via WASM.
//
// EW-G9: measurement-only, no consumer app integration.
// No daemon, no IPC, no transport, no signaling.
// Reuses theme, state, and screen rendering from bolt-ui where possible.

mod screens;
mod state;
mod theme;

use eframe::egui;
use screens::connect::{ConnectAction, ConnectState};
use screens::verify::VerifyAction;
use screens::Screen;
use state::*;

/// Browser-specific app struct. No daemon/IPC/filesystem fields.
/// Compare with bolt-ui's BoltApp which has daemon_proc, ipc_client,
/// data_dir, socket_path — all excluded here.
struct BoltWebApp {
    current_screen: Screen,
    mode: ConnectMode,
    host_info: Option<HostInfo>,
    join_room: String,
    join_session: String,
    join_peer_code: String,
    local_peer_code: String,
    connection: ConnectionState,
    transfer: TransferState,
    verify: VerifyState,
}

impl BoltWebApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::apply_theme(&cc.egui_ctx);

        // bolt-core peer code generation — already WASM-proven
        let local_peer_code = bolt_core::peer_code::generate_secure_peer_code();

        Self {
            current_screen: Screen::Connect,
            mode: ConnectMode::Host,
            host_info: None,
            join_room: String::new(),
            join_session: String::new(),
            join_peer_code: String::new(),
            local_peer_code,
            connection: ConnectionState::Idle,
            transfer: TransferState::Idle,
            verify: VerifyState::NotStarted,
        }
    }

    fn handle_connect_action(&mut self, action: ConnectAction) {
        match action {
            ConnectAction::None => {}
            ConnectAction::SwitchMode(mode) => {
                self.mode = mode;
            }
            ConnectAction::CreateSession => {
                // In desktop bolt-ui this spawns a daemon.
                // In browser PoC we just generate mock host info.
                let peer_code = bolt_core::peer_code::generate_secure_peer_code();
                self.host_info = Some(HostInfo {
                    peer_code,
                    room: format!("r{}", &self.local_peer_code[..4].to_lowercase()),
                    session: format!(
                        "s{:08x}",
                        web_time::Instant::now().elapsed().as_millis() as u32
                    ),
                });
            }
            ConnectAction::StartHostWithJoiner(_code) => {
                // No daemon to start — simulate connecting state
                self.connection = ConnectionState::Connecting {
                    started_at: web_time::Instant::now(),
                };
            }
            ConnectAction::StartJoin { .. } => {
                self.connection = ConnectionState::Connecting {
                    started_at: web_time::Instant::now(),
                };
            }
            ConnectAction::Cancel => {
                self.connection = ConnectionState::Idle;
                self.host_info = None;
            }
        }
    }
}

impl eframe::App for BoltWebApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Navigation bar
        egui::TopBottomPanel::top("nav").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Bolt")
                        .size(theme::FONT_SIZE_HEADING)
                        .color(theme::ACCENT),
                );
                ui.label(
                    egui::RichText::new("EW2 PoC")
                        .size(theme::FONT_SIZE_SMALL)
                        .color(theme::TEXT_MUTED),
                );

                ui.add_space(theme::SPACING_XL);

                for (screen, label) in [
                    (Screen::Connect, "Connect"),
                    (Screen::Transfer, "Transfer"),
                    (Screen::Verify, "Verify"),
                ] {
                    let active = self.current_screen == screen;
                    if ui
                        .selectable_label(
                            active,
                            egui::RichText::new(label).color(if active {
                                theme::ACCENT
                            } else {
                                theme::TEXT_SECONDARY
                            }),
                        )
                        .clicked()
                    {
                        self.current_screen = screen;
                    }
                }
            });
        });

        // Main content
        egui::CentralPanel::default().show(ctx, |ui| match self.current_screen {
            Screen::Connect => {
                let mut connect_state = ConnectState {
                    mode: self.mode,
                    host_info: self.host_info.as_ref(),
                    local_peer_code: &self.local_peer_code,
                    join_room: &mut self.join_room,
                    join_session: &mut self.join_session,
                    join_peer_code: &mut self.join_peer_code,
                    connection: &self.connection,
                    prereq_error: None,
                };
                let action = screens::connect::show(ui, &mut connect_state);
                self.handle_connect_action(action);
            }
            Screen::Transfer => {
                screens::transfer::show(ui, &self.transfer);
            }
            Screen::Verify => {
                let mut action = VerifyAction::None;
                screens::verify::show(ui, &self.verify, &mut action);
                match action {
                    VerifyAction::None => {}
                    VerifyAction::Confirm => self.verify = VerifyState::Confirmed,
                    VerifyAction::Reject => self.verify = VerifyState::Rejected,
                }
            }
        });

        // Check timeout
        if self.connection.is_timed_out() {
            self.connection = ConnectionState::TimedOut;
        }
    }
}

// WASM entry point
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub async fn start(canvas_id: &str) -> Result<(), JsValue> {
    console_error_panic_hook::set_once();

    let document = web_sys::window()
        .expect("no window")
        .document()
        .expect("no document");
    let canvas = document
        .get_element_by_id(canvas_id)
        .expect("canvas not found");
    let canvas: web_sys::HtmlCanvasElement = canvas.dyn_into().expect("element is not a canvas");

    let web_options = eframe::WebOptions::default();

    eframe::WebRunner::new()
        .start(
            canvas,
            web_options,
            Box::new(|cc| Ok(Box::new(BoltWebApp::new(cc)))),
        )
        .await
}
