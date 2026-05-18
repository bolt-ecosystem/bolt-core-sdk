// EW2 REUSE: Adapted from bolt-ui/src/screens/connect.rs.
//
// DECOUPLING REQUIRED: bolt-ui's connect.rs takes &mut BoltApp (desktop struct
// with daemon/IPC fields). This version takes state refs instead.
// All egui rendering code is preserved; only the function signatures changed.
// Daemon stderr display (lines 77-88 in original) removed — no daemon in WASM.
// app.start_host(), app.start_join(), app.cancel_connect() replaced with
// ConnectAction enum for the caller to handle.

use eframe::egui;

use crate::state::*;
use crate::theme;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectAction {
    None,
    SwitchMode(ConnectMode),
    CreateSession,
    StartHostWithJoiner(String),
    StartJoin {
        room: String,
        session: String,
        peer_code: String,
    },
    Cancel,
}

pub struct ConnectState<'a> {
    pub mode: ConnectMode,
    pub host_info: Option<&'a HostInfo>,
    pub local_peer_code: &'a str,
    pub join_room: &'a mut String,
    pub join_session: &'a mut String,
    pub join_peer_code: &'a mut String,
    pub connection: &'a ConnectionState,
    pub prereq_error: Option<&'a str>,
}

pub fn show(ui: &mut egui::Ui, state: &mut ConnectState<'_>) -> ConnectAction {
    let mut action = ConnectAction::None;

    ui.add_space(theme::SPACING_XL);

    // Mode selector (Host / Join tabs)
    ui.horizontal(|ui| {
        let host_active = state.mode == ConnectMode::Host;
        let join_active = state.mode == ConnectMode::Join;
        let can_switch = state.connection.can_retry();

        if ui
            .selectable_label(
                host_active,
                egui::RichText::new("Host")
                    .size(theme::FONT_SIZE_HEADING)
                    .color(if host_active {
                        theme::ACCENT
                    } else {
                        theme::TEXT_SECONDARY
                    }),
            )
            .clicked()
            && can_switch
        {
            action = ConnectAction::SwitchMode(ConnectMode::Host);
        }

        ui.add_space(theme::SPACING_LG);

        if ui
            .selectable_label(
                join_active,
                egui::RichText::new("Join")
                    .size(theme::FONT_SIZE_HEADING)
                    .color(if join_active {
                        theme::ACCENT
                    } else {
                        theme::TEXT_SECONDARY
                    }),
            )
            .clicked()
            && can_switch
        {
            action = ConnectAction::SwitchMode(ConnectMode::Join);
        }
    });

    ui.add_space(theme::SPACING_LG);
    ui.separator();
    ui.add_space(theme::SPACING_LG);

    match state.mode {
        ConnectMode::Host => {
            let host_action = show_host(ui, state);
            if host_action != ConnectAction::None {
                action = host_action;
            }
        }
        ConnectMode::Join => {
            let join_action = show_join(ui, state);
            if join_action != ConnectAction::None {
                action = join_action;
            }
        }
    }

    // Status bar
    ui.add_space(theme::SPACING_XL);
    let status_color = match state.connection {
        ConnectionState::Idle => theme::TEXT_MUTED,
        ConnectionState::Connecting { .. } => theme::WARNING,
        ConnectionState::Connected => theme::SUCCESS,
        ConnectionState::TimedOut => theme::ERROR,
        ConnectionState::Error(_) => theme::ERROR,
    };
    let status_text = match state.connection {
        ConnectionState::Error(msg) => format!("Error: {msg}"),
        other => other.status_text().to_string(),
    };
    ui.colored_label(status_color, &status_text);

    action
}

fn show_host(ui: &mut egui::Ui, state: &mut ConnectState<'_>) -> ConnectAction {
    let mut action = ConnectAction::None;

    ui.vertical_centered(|ui| {
        if let Some(err) = state.prereq_error {
            ui.colored_label(theme::ERROR, format!("Prerequisite: {err}"));
            ui.add_space(theme::SPACING_MD);
        }

        if state.host_info.is_none() && *state.connection == ConnectionState::Idle {
            let btn = egui::Button::new(
                egui::RichText::new("Create Session")
                    .size(theme::FONT_SIZE_BODY)
                    .color(theme::ACCENT_FG),
            )
            .fill(theme::ACCENT)
            .corner_radius(theme::ROUNDING);

            if ui.add_enabled(state.prereq_error.is_none(), btn).clicked() {
                action = ConnectAction::CreateSession;
            }
        }

        if let Some(info) = state.host_info {
            ui.label(
                egui::RichText::new("Share these with your peer:")
                    .size(theme::FONT_SIZE_BODY)
                    .color(theme::TEXT_SECONDARY),
            );
            ui.add_space(theme::SPACING_MD);

            egui::Frame::NONE
                .fill(theme::PANEL_BG)
                .corner_radius(theme::ROUNDING)
                .inner_margin(theme::SPACING_LG)
                .stroke(egui::Stroke::new(1.0, theme::ACCENT))
                .show(ui, |ui| {
                    ui.set_min_width(280.0);
                    show_field(ui, "Room", &info.room);
                    show_field(ui, "Session", &info.session);
                    show_field(ui, "Code", &info.peer_code);
                });

            if matches!(state.connection, ConnectionState::Idle) {
                ui.add_space(theme::SPACING_LG);
                ui.label(
                    egui::RichText::new("Enter joiner's peer code to start:")
                        .size(theme::FONT_SIZE_BODY)
                        .color(theme::TEXT_SECONDARY),
                );
                ui.add_space(theme::SPACING_SM);

                let input = egui::TextEdit::singleline(state.join_peer_code)
                    .hint_text("Joiner's code")
                    .font(egui::TextStyle::Monospace)
                    .desired_width(150.0)
                    .char_limit(bolt_core::constants::PEER_CODE_LENGTH);
                ui.add(input);

                ui.add_space(theme::SPACING_MD);

                let start_btn = egui::Button::new(
                    egui::RichText::new("Start Listening")
                        .size(theme::FONT_SIZE_BODY)
                        .color(theme::ACCENT_FG),
                )
                .fill(theme::ACCENT)
                .corner_radius(theme::ROUNDING);

                let ready = state.join_peer_code.len() == bolt_core::constants::PEER_CODE_LENGTH;
                if ui.add_enabled(ready, start_btn).clicked() {
                    action = ConnectAction::StartHostWithJoiner(state.join_peer_code.clone());
                }
            }
        }

        if state.connection.can_cancel() {
            ui.add_space(theme::SPACING_MD);
            let cancel = egui::Button::new(
                egui::RichText::new("Cancel")
                    .size(theme::FONT_SIZE_BODY)
                    .color(theme::TEXT_PRIMARY),
            )
            .fill(theme::ERROR)
            .corner_radius(theme::ROUNDING);
            if ui.add(cancel).clicked() {
                action = ConnectAction::Cancel;
            }
        }
    });

    action
}

fn show_join(ui: &mut egui::Ui, state: &mut ConnectState<'_>) -> ConnectAction {
    let mut action = ConnectAction::None;

    ui.vertical_centered(|ui| {
        ui.label(
            egui::RichText::new("Your Peer Code")
                .size(theme::FONT_SIZE_SMALL)
                .color(theme::TEXT_SECONDARY),
        );
        ui.label(
            egui::RichText::new(state.local_peer_code)
                .size(theme::FONT_SIZE_TITLE)
                .color(theme::ACCENT)
                .monospace(),
        );

        ui.add_space(theme::SPACING_XL);

        ui.label(
            egui::RichText::new("Enter host's connection details:")
                .size(theme::FONT_SIZE_BODY)
                .color(theme::TEXT_SECONDARY),
        );
        ui.add_space(theme::SPACING_MD);

        let is_connecting = matches!(state.connection, ConnectionState::Connecting { .. });

        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Room:").color(theme::TEXT_SECONDARY));
            let r = egui::TextEdit::singleline(state.join_room)
                .hint_text("e.g. r1a2b3")
                .font(egui::TextStyle::Monospace)
                .desired_width(150.0)
                .interactive(!is_connecting);
            ui.add(r);
        });

        ui.add_space(theme::SPACING_SM);

        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Session:").color(theme::TEXT_SECONDARY));
            let s = egui::TextEdit::singleline(state.join_session)
                .hint_text("e.g. s4d5e6f7")
                .font(egui::TextStyle::Monospace)
                .desired_width(150.0)
                .interactive(!is_connecting);
            ui.add(s);
        });

        ui.add_space(theme::SPACING_SM);

        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Code:").color(theme::TEXT_SECONDARY));
            let c = egui::TextEdit::singleline(state.join_peer_code)
                .hint_text("Host's code")
                .font(egui::TextStyle::Monospace)
                .desired_width(150.0)
                .interactive(!is_connecting);
            ui.add(c);
        });

        ui.add_space(theme::SPACING_LG);

        if is_connecting {
            let cancel = egui::Button::new(
                egui::RichText::new("Cancel")
                    .size(theme::FONT_SIZE_BODY)
                    .color(theme::TEXT_PRIMARY),
            )
            .fill(theme::ERROR)
            .corner_radius(theme::ROUNDING);
            if ui.add(cancel).clicked() {
                action = ConnectAction::Cancel;
            }
        } else {
            let label = if state.connection.can_retry()
                && !matches!(state.connection, ConnectionState::Idle)
            {
                "Retry"
            } else {
                "Join"
            };
            let join_btn = egui::Button::new(
                egui::RichText::new(label)
                    .size(theme::FONT_SIZE_BODY)
                    .color(theme::ACCENT_FG),
            )
            .fill(theme::ACCENT)
            .corner_radius(theme::ROUNDING);

            let ready = !state.join_room.is_empty()
                && !state.join_session.is_empty()
                && !state.join_peer_code.is_empty()
                && state.prereq_error.is_none();

            if ui.add_enabled(ready, join_btn).clicked() {
                action = ConnectAction::StartJoin {
                    room: state.join_room.clone(),
                    session: state.join_session.clone(),
                    peer_code: state.join_peer_code.clone(),
                };
            }
        }
    });

    action
}

fn show_field(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(format!("{label}:"))
                .size(theme::FONT_SIZE_BODY)
                .color(theme::TEXT_SECONDARY),
        );
        ui.label(
            egui::RichText::new(value)
                .size(theme::FONT_SIZE_BODY)
                .color(theme::ACCENT)
                .monospace(),
        );
    });
}
