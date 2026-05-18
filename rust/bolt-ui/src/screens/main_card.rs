use eframe::egui;

use crate::app::BoltApp;
use crate::state::*;
use crate::theme;

/// Single-card progressive flow — matches website's one-card architecture.
/// State drives what's shown. No tabs.
pub fn show(ui: &mut egui::Ui, app: &mut BoltApp) {
    ui.vertical_centered(|ui| {
        // ── Encryption badge (always visible, matches website) ──
        ui.label(
            egui::RichText::new("\u{25CB} End-to-End Encrypted")
                .size(theme::FONT_SIZE_SMALL)
                .color(theme::ACCENT_DIM),
        );

        ui.add_space(theme::SPACING_LG);

        // ── Main card ────────────────────────────────────────
        theme::panel_frame().show(ui, |ui| {
            ui.set_min_width(360.0);
            ui.set_max_width(420.0);

            // Prerequisite error
            if let Some(err) = &app.prereq_error {
                ui.label(
                    egui::RichText::new(format!("! {err}"))
                        .size(theme::FONT_SIZE_SMALL)
                        .color(theme::ERROR),
                );
                ui.add_space(theme::SPACING_MD);
            }

            // ── State-driven card content ─────────────────────
            match &app.connection {
                ConnectionState::Connected => {
                    show_connected(ui, app);
                }
                ConnectionState::Requesting { .. } => {
                    show_requesting(ui, app);
                }
                ConnectionState::Establishing { .. } => {
                    show_establishing(ui, app);
                }
                ConnectionState::Error(msg) => {
                    let msg = msg.clone();
                    show_error(ui, &msg);
                    ui.add_space(theme::SPACING_LG);
                    show_device_list(ui, app);
                }
                ConnectionState::TimedOut => {
                    show_error(ui, "Connection timed out");
                    ui.add_space(theme::SPACING_LG);
                    show_device_list(ui, app);
                }
                ConnectionState::Idle => {
                    // Check for incoming request first
                    if app.incoming_request.is_some() {
                        show_incoming_request(ui, app);
                    } else {
                        show_device_list(ui, app);
                    }
                }
            }
        });

        // ── Manual pair fallback (below main card) ───────────
        ui.add_space(theme::SPACING_LG);
        if ui
            .small_button(
                egui::RichText::new(if app.show_manual_pair {
                    "\u{25BC} Manual Pair"
                } else {
                    "\u{25B6} Manual Pair"
                })
                .size(theme::FONT_SIZE_SMALL)
                .color(theme::TEXT_MUTED),
            )
            .clicked()
        {
            app.show_manual_pair = !app.show_manual_pair;
        }

        if app.show_manual_pair {
            ui.add_space(theme::SPACING_SM);
            show_manual_pair(ui, app);
        }
    });
}

// ── Device list (discovery) ──────────────────────────────────

fn show_device_list(ui: &mut egui::Ui, app: &mut BoltApp) {
    theme::section_label(ui, "select a device");
    ui.add_space(theme::SPACING_SM);

    if app.discovered_peers.is_empty() {
        ui.horizontal(|ui| {
            theme::status_dot(ui, theme::WARNING, true, "Searching...");
            ui.label(
                egui::RichText::new("Searching for nearby devices...")
                    .size(theme::FONT_SIZE_BODY)
                    .color(theme::TEXT_SECONDARY),
            );
        });
    } else {
        let mut selected_peer = None;
        for peer in &app.discovered_peers {
            let resp = ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(peer.device_type.icon())
                        .size(theme::FONT_SIZE_BODY)
                        .color(theme::ACCENT_DIM),
                );
                ui.label(
                    egui::RichText::new(&peer.device_name)
                        .size(theme::FONT_SIZE_BODY)
                        .color(theme::TEXT_PRIMARY),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new("\u{25B6}")
                            .size(theme::FONT_SIZE_SMALL)
                            .color(theme::ACCENT_DIM),
                    );
                });
            });
            if resp.response.interact(egui::Sense::click()).clicked() {
                selected_peer = Some(peer.clone());
            }
            ui.separator();
        }
        if let Some(peer) = selected_peer {
            app.connect_to_peer(&peer);
        }
    }
}

// ── Requesting (waiting for acceptance) ──────────────────────

fn show_requesting(ui: &mut egui::Ui, app: &mut BoltApp) {
    let text = app.connection.status_text();
    ui.label(
        egui::RichText::new(&text)
            .size(theme::FONT_SIZE_BODY)
            .color(theme::WARNING),
    );
    ui.add_space(theme::SPACING_MD);
    theme::status_dot(ui, theme::WARNING, true, "Waiting...");
    ui.add_space(theme::SPACING_LG);
    if ui.add(theme::danger_button("CANCEL")).clicked() {
        app.cancel_request();
    }
}

// ── Establishing (daemon spawning / WebRTC handshake) ────────

fn show_establishing(ui: &mut egui::Ui, app: &mut BoltApp) {
    let text = app.connection.status_text();
    ui.label(
        egui::RichText::new(&text)
            .size(theme::FONT_SIZE_BODY)
            .color(theme::WARNING),
    );
    ui.add_space(theme::SPACING_MD);
    theme::status_dot(ui, theme::ACCENT, true, "Establishing...");
    ui.add_space(theme::SPACING_LG);
    if ui.add(theme::danger_button("CANCEL")).clicked() {
        app.cancel_connect();
    }
}

// ── Incoming request ─────────────────────────────────────────

fn show_incoming_request(ui: &mut egui::Ui, app: &mut BoltApp) {
    let (name, icon) = {
        let req = app.incoming_request.as_ref().unwrap();
        (req.device_name.clone(), req.device_type.icon().to_string())
    };

    ui.label(
        egui::RichText::new(&icon)
            .size(theme::FONT_SIZE_DISPLAY)
            .color(theme::ACCENT),
    );
    ui.add_space(theme::SPACING_MD);
    ui.label(
        egui::RichText::new(format!("{name} wants to connect"))
            .size(theme::FONT_SIZE_HEADING)
            .color(theme::TEXT_PRIMARY),
    );
    ui.label(
        egui::RichText::new("Accept to start sharing files")
            .size(theme::FONT_SIZE_SMALL)
            .color(theme::TEXT_SECONDARY),
    );
    ui.add_space(theme::SPACING_LG);
    ui.horizontal(|ui| {
        if ui.add(theme::danger_button("DECLINE")).clicked() {
            app.decline_incoming();
        }
        ui.add_space(theme::SPACING_MD);
        if ui.add(theme::primary_button("ACCEPT")).clicked() {
            app.accept_incoming();
        }
    });
}

// ── Connected ────────────────────────────────────────────────

fn show_connected(ui: &mut egui::Ui, app: &mut BoltApp) {
    let (name, icon) = {
        let cp = app.connected_peer.as_ref();
        (
            cp.map(|p| p.device_name.clone())
                .unwrap_or_else(|| "Peer".into()),
            cp.map(|p| p.device_type.icon().to_string())
                .unwrap_or_else(|| "\u{25CB}".into()),
        )
    };

    // Connected peer row
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(&icon)
                .size(theme::FONT_SIZE_HEADING)
                .color(theme::ACCENT),
        );
        ui.label(
            egui::RichText::new(&name)
                .size(theme::FONT_SIZE_HEADING)
                .color(theme::TEXT_PRIMARY),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.add(theme::danger_button("DISCONNECT")).clicked() {
                app.disconnect();
            }
        });
    });

    ui.add_space(theme::SPACING_LG);

    // Verification state (inline, matches website verification-gated flow)
    match &app.verify {
        VerifyState::Pending { sas_code } => {
            let formatted = sas_code
                .chars()
                .collect::<Vec<_>>()
                .chunks(2)
                .map(|c| c.iter().collect::<String>())
                .collect::<Vec<_>>()
                .join("  ");

            theme::section_label(ui, "verify connection");
            ui.add_space(theme::SPACING_SM);
            ui.label(theme::mono_value(&formatted, theme::FONT_SIZE_TITLE));
            ui.label(
                egui::RichText::new("Compare this code with the other device.")
                    .size(theme::FONT_SIZE_SMALL)
                    .color(theme::TEXT_SECONDARY),
            );
            ui.add_space(theme::SPACING_SM);
            ui.horizontal(|ui| {
                if ui.add(theme::primary_button("Mark Verified")).clicked() {
                    tracing::info!("[UI] user marked peer as verified");
                    app.verify = VerifyState::Confirmed;
                    app.transfer = TransferState::Ready;
                }
                ui.add_space(theme::SPACING_SM);
                if ui
                    .add(egui::Button::new(
                        egui::RichText::new("Reject")
                            .size(theme::FONT_SIZE_BODY)
                            .color(theme::ERROR),
                    ))
                    .clicked()
                {
                    tracing::info!("[UI] user rejected peer verification");
                    app.verify = VerifyState::Rejected;
                    // Disconnect — rejected peer should not remain connected
                    if let Some(mut proc) = app.daemon_proc.take() {
                        proc.kill();
                    }
                    app.ipc_client = None;
                    app.connection = ConnectionState::Idle;
                    app.transfer = TransferState::Idle;
                }
            });
        }
        VerifyState::Confirmed => {
            ui.horizontal(|ui| {
                theme::status_dot(ui, theme::SUCCESS, false, "Verified");
                ui.label(
                    egui::RichText::new("Verified")
                        .size(theme::FONT_SIZE_SMALL)
                        .color(theme::SUCCESS),
                );
            });
        }
        VerifyState::Legacy => {
            ui.horizontal(|ui| {
                theme::status_dot(ui, theme::TEXT_SECONDARY, false, "Legacy");
                ui.label(
                    egui::RichText::new("Legacy Peer")
                        .size(theme::FONT_SIZE_SMALL)
                        .color(theme::TEXT_SECONDARY),
                );
            });
        }
        _ => {}
    }

    ui.add_space(theme::SPACING_LG);

    // Transfer state — only shown when verification allows it (matches web policy)
    if app.verify.is_transfer_allowed() {
        match &app.transfer {
            TransferState::Ready => {
                theme::section_label(ui, "transfer");
                ui.add_space(theme::SPACING_SM);
                if ui.add(theme::primary_button("SEND FILE")).clicked() {
                    if let Some(path) = rfd::FileDialog::new().pick_file() {
                        let file_name = path
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_else(|| "file".into());
                        let path_str = path.display().to_string();
                        tracing::info!("[UI] file selected: {} ({})", file_name, path_str);

                        // Signal daemon to send file via signal file
                        let signal_path = format!("{}/send_file.signal", app.data_dir());
                        match std::fs::write(&signal_path, &path_str) {
                            Ok(()) => {
                                tracing::info!("[UI] wrote send signal: {signal_path}");
                                app.transfer = TransferState::Sending {
                                    file_name,
                                    progress: 0.0,
                                };
                            }
                            Err(e) => {
                                tracing::error!("[UI] failed to write send signal: {e}");
                                app.transfer = TransferState::Failed {
                                    file_name,
                                    reason: format!("Signal error: {e}"),
                                };
                            }
                        }
                    }
                }
            }
            TransferState::Sending {
                file_name,
                progress,
            } => {
                theme::section_label(ui, "sending");
                theme::field_row(ui, "File", file_name);
                ui.add(
                    egui::ProgressBar::new(*progress)
                        .text(format!("{:.0}%", progress * 100.0))
                        .desired_width(320.0),
                );
            }
            TransferState::Receiving {
                file_name,
                progress,
            } => {
                theme::section_label(ui, "receiving");
                theme::field_row(ui, "File", file_name);
                ui.add(
                    egui::ProgressBar::new(*progress)
                        .text(format!("{:.0}%", progress * 100.0))
                        .desired_width(320.0),
                );
            }
            TransferState::Complete {
                file_name,
                save_path,
            } => {
                ui.label(
                    egui::RichText::new(format!("\u{2713} {} — complete", file_name))
                        .size(theme::FONT_SIZE_BODY)
                        .color(theme::SUCCESS),
                );
                if let Some(path) = save_path {
                    ui.label(
                        egui::RichText::new(path)
                            .size(theme::FONT_SIZE_SMALL)
                            .color(theme::TEXT_SECONDARY),
                    );
                    ui.add_space(theme::SPACING_SM);
                    #[cfg(target_os = "macos")]
                    {
                        let path_clone = path.clone();
                        if ui.add(theme::primary_button("SHOW IN FINDER")).clicked() {
                            let _ = std::process::Command::new("open")
                                .arg("-R")
                                .arg(&path_clone)
                                .spawn();
                        }
                    }
                }
                ui.add_space(theme::SPACING_SM);
                if ui.add(theme::primary_button("SEND ANOTHER")).clicked() {
                    app.transfer = TransferState::Ready;
                }
            }
            TransferState::Failed { file_name, reason } => {
                ui.label(
                    egui::RichText::new(format!("\u{2717} {} — {}", file_name, reason))
                        .size(theme::FONT_SIZE_BODY)
                        .color(theme::ERROR),
                );
                ui.add_space(theme::SPACING_SM);
                if ui.add(theme::primary_button("TRY AGAIN")).clicked() {
                    app.transfer = TransferState::Ready;
                }
            }
            TransferState::Idle => {}
        }
    }
}

// ── Error state ──────────────────────────────────────────────

fn show_error(ui: &mut egui::Ui, msg: &str) {
    ui.label(
        egui::RichText::new(format!("! {msg}"))
            .size(theme::FONT_SIZE_SMALL)
            .color(theme::ERROR),
    );
}

// ── Manual pair fallback ─────────────────────────────────────

fn show_manual_pair(ui: &mut egui::Ui, app: &mut BoltApp) {
    theme::inset_frame().show(ui, |ui| {
        ui.set_min_width(340.0);

        ui.horizontal(|ui| {
            for (mode, label) in [(ConnectMode::Host, "HOST"), (ConnectMode::Join, "JOIN")] {
                let active = app.mode == mode;
                let color = if active {
                    theme::ACCENT
                } else {
                    theme::TEXT_MUTED
                };
                if ui
                    .selectable_label(
                        active,
                        egui::RichText::new(label)
                            .size(theme::FONT_SIZE_SMALL)
                            .color(color),
                    )
                    .clicked()
                {
                    app.mode = mode;
                }
            }
        });
        ui.add_space(theme::SPACING_SM);

        match app.mode {
            ConnectMode::Host => {
                if app.host_info.is_none()
                    && app.connection == ConnectionState::Idle
                    && ui
                        .add_enabled(
                            app.prereq_error.is_none(),
                            theme::primary_button("CREATE SESSION"),
                        )
                        .clicked()
                {
                    app.start_host();
                }
                if let Some(info) = &app.host_info {
                    theme::field_row(ui, "Room", &info.room);
                    theme::field_row(ui, "Session", &info.session);
                    theme::field_row(ui, "Code", &info.peer_code);

                    if matches!(app.connection, ConnectionState::Idle) {
                        ui.add_space(theme::SPACING_SM);
                        let mut joiner = app.join_peer_code.clone();
                        ui.add(
                            egui::TextEdit::singleline(&mut joiner)
                                .hint_text("peer code")
                                .font(egui::TextStyle::Monospace)
                                .desired_width(150.0)
                                .char_limit(bolt_core::constants::PEER_CODE_LENGTH),
                        );
                        app.join_peer_code = joiner;
                        let ready =
                            app.join_peer_code.len() == bolt_core::constants::PEER_CODE_LENGTH;
                        if ui
                            .add_enabled(ready, theme::primary_button("START"))
                            .clicked()
                        {
                            let code = app.join_peer_code.clone();
                            app.start_host_with_joiner(&code);
                        }
                    }
                }
            }
            ConnectMode::Join => {
                ui.label(
                    egui::RichText::new(format!("Your code: {}", app.local_peer_code))
                        .size(theme::FONT_SIZE_SMALL)
                        .color(theme::ACCENT),
                );
                ui.add_space(theme::SPACING_SM);

                let mut room = app.join_room.clone();
                let mut sess = app.join_session.clone();
                let mut code = app.join_peer_code.clone();

                for (label, value, hint) in [
                    ("Room", &mut room, "r1a2b3"),
                    ("Session", &mut sess, "s4d5e6f7"),
                    ("Code", &mut code, "host code"),
                ] {
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(format!("{label}:"))
                                .size(theme::FONT_SIZE_SMALL)
                                .color(theme::TEXT_SECONDARY),
                        );
                        ui.add(
                            egui::TextEdit::singleline(value)
                                .hint_text(hint)
                                .font(egui::TextStyle::Monospace)
                                .desired_width(140.0),
                        );
                    });
                }
                app.join_room = room;
                app.join_session = sess;
                app.join_peer_code = code;

                ui.add_space(theme::SPACING_SM);
                let ready = !app.join_room.is_empty()
                    && !app.join_session.is_empty()
                    && !app.join_peer_code.is_empty()
                    && app.prereq_error.is_none();
                if ui
                    .add_enabled(ready, theme::primary_button("JOIN"))
                    .clicked()
                {
                    app.start_join();
                }
            }
        }
    });
}
