// EW2 REUSE: Copied from bolt-ui/src/screens/transfer.rs.
// Change: crate path only (crate::state, crate::theme).
// Zero logic changes — this screen was already state-ref based.

use eframe::egui;

use crate::state::TransferState;
use crate::theme;

pub fn show(ui: &mut egui::Ui, transfer: &TransferState) {
    ui.add_space(theme::SPACING_XL);

    ui.vertical_centered(|ui| {
        ui.label(
            egui::RichText::new("File Transfer")
                .size(theme::FONT_SIZE_HEADING)
                .color(theme::TEXT_PRIMARY),
        );

        ui.add_space(theme::SPACING_XL);

        let status_text = transfer.status_text();
        let status_color = match transfer {
            TransferState::Idle => theme::TEXT_MUTED,
            TransferState::Ready => theme::SUCCESS,
            TransferState::Sending { .. } | TransferState::Receiving { .. } => theme::ACCENT,
            TransferState::Complete { .. } => theme::SUCCESS,
            TransferState::Failed { .. } => theme::ERROR,
        };

        ui.label(
            egui::RichText::new(&status_text)
                .size(theme::FONT_SIZE_BODY)
                .color(status_color),
        );
        ui.add_space(theme::SPACING_SM);

        let progress = transfer.progress();
        let progress_text = format!("{:.0}%", progress * 100.0);
        let progress_bar = egui::ProgressBar::new(progress)
            .text(&progress_text)
            .desired_width(300.0);
        ui.add(progress_bar);

        ui.add_space(theme::SPACING_XL);

        egui::Frame::NONE
            .fill(theme::PANEL_BG)
            .corner_radius(theme::ROUNDING)
            .inner_margin(theme::SPACING_LG)
            .stroke(egui::Stroke::new(1.0, theme::BORDER))
            .show(ui, |ui| {
                ui.set_min_width(300.0);
                ui.label(
                    egui::RichText::new("Transfer Details")
                        .size(theme::FONT_SIZE_BODY)
                        .color(theme::TEXT_SECONDARY),
                );
                ui.separator();

                match transfer {
                    TransferState::Idle => {
                        ui.label(
                            egui::RichText::new("No files queued")
                                .size(theme::FONT_SIZE_BODY)
                                .color(theme::TEXT_MUTED),
                        );
                    }
                    TransferState::Ready => {
                        ui.label(
                            egui::RichText::new("Connected — ready for transfer")
                                .size(theme::FONT_SIZE_BODY)
                                .color(theme::SUCCESS),
                        );
                    }
                    TransferState::Sending {
                        file_name,
                        progress,
                    } => {
                        ui.label(
                            egui::RichText::new(format!("\u{2191} {}", file_name))
                                .size(theme::FONT_SIZE_BODY)
                                .color(theme::ACCENT),
                        );
                        ui.label(
                            egui::RichText::new(format!("{:.1}% sent", progress * 100.0))
                                .size(theme::FONT_SIZE_SMALL)
                                .color(theme::TEXT_SECONDARY),
                        );
                    }
                    TransferState::Receiving {
                        file_name,
                        progress,
                    } => {
                        ui.label(
                            egui::RichText::new(format!("\u{2193} {}", file_name))
                                .size(theme::FONT_SIZE_BODY)
                                .color(theme::ACCENT),
                        );
                        ui.label(
                            egui::RichText::new(format!("{:.1}% received", progress * 100.0))
                                .size(theme::FONT_SIZE_SMALL)
                                .color(theme::TEXT_SECONDARY),
                        );
                    }
                    TransferState::Complete { file_name } => {
                        ui.label(
                            egui::RichText::new(format!("\u{2713} {}", file_name))
                                .size(theme::FONT_SIZE_BODY)
                                .color(theme::SUCCESS),
                        );
                    }
                    TransferState::Failed { file_name, reason } => {
                        ui.label(
                            egui::RichText::new(format!("\u{2717} {}", file_name))
                                .size(theme::FONT_SIZE_BODY)
                                .color(theme::ERROR),
                        );
                        ui.label(
                            egui::RichText::new(reason)
                                .size(theme::FONT_SIZE_SMALL)
                                .color(theme::TEXT_MUTED),
                        );
                    }
                }
            });

        ui.add_space(theme::SPACING_XL);

        let send_btn = egui::Button::new(
            egui::RichText::new("Send File")
                .size(theme::FONT_SIZE_BODY)
                .color(theme::ACCENT_FG),
        )
        .fill(theme::ACCENT)
        .corner_radius(theme::ROUNDING);

        if ui.add_enabled(false, send_btn).clicked() {
            // No transport in EW2 PoC
        }

        ui.add_space(theme::SPACING_SM);
        ui.label(
            egui::RichText::new("Connect to a peer first")
                .size(theme::FONT_SIZE_SMALL)
                .color(theme::TEXT_MUTED),
        );
    });
}
