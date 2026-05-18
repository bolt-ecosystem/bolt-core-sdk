use eframe::egui;

// BOLT-UI-PRODUCT-1: 16-bit retro console / dark terminal visual system.
//
// Aesthetic: SNES/Genesis system menu meets phosphor terminal.
// Dark base, bright phosphor green, chunky beveled panels, monospace.
// Think: retro console boot screen, CRT monitor energy.

// ── Background / Surface ─────────────────────────────────────
// Pure dark with faint green CRT warmth.
pub const WINDOW_BG: egui::Color32 = egui::Color32::from_rgb(0x08, 0x0C, 0x06);
pub const PANEL_BG: egui::Color32 = egui::Color32::from_rgb(0x10, 0x16, 0x0C);
pub const SURFACE_RAISED: egui::Color32 = egui::Color32::from_rgb(0x18, 0x20, 0x14);
pub const PANEL_INSET: egui::Color32 = egui::Color32::from_rgb(0x04, 0x06, 0x03);

// ── Primary Accent (bright phosphor green — 16-bit CRT) ─────
pub const ACCENT: egui::Color32 = egui::Color32::from_rgb(0x33, 0xFF, 0x33);
#[allow(dead_code)]
pub const ACCENT_HOVER: egui::Color32 = egui::Color32::from_rgb(0x66, 0xFF, 0x66);
pub const ACCENT_DIM: egui::Color32 = egui::Color32::from_rgb(0x1A, 0x88, 0x1A);
pub const ACCENT_FG: egui::Color32 = egui::Color32::from_rgb(0x00, 0x00, 0x00);
#[allow(dead_code)]
pub const ACCENT_GLOW: egui::Color32 = egui::Color32::from_rgb(0x22, 0x66, 0x22);

// ── Text ─────────────────────────────────────────────────────
// Green-tinted phosphor text for retro CRT feel.
pub const TEXT_PRIMARY: egui::Color32 = egui::Color32::from_rgb(0xCC, 0xEE, 0xBB);
pub const TEXT_SECONDARY: egui::Color32 = egui::Color32::from_rgb(0x66, 0x88, 0x55);
pub const TEXT_MUTED: egui::Color32 = egui::Color32::from_rgb(0x33, 0x44, 0x2A);

// ── Semantic ─────────────────────────────────────────────────
pub const SUCCESS: egui::Color32 = ACCENT;
pub const ERROR: egui::Color32 = egui::Color32::from_rgb(0xFF, 0x33, 0x33);
pub const WARNING: egui::Color32 = egui::Color32::from_rgb(0xFF, 0xCC, 0x00);

// ── Borders ──────────────────────────────────────────────────
// Retro beveled feel: bright top-left, dark bottom-right.
pub const BORDER: egui::Color32 = egui::Color32::from_rgb(0x1A, 0x22, 0x16);
pub const BORDER_BRIGHT: egui::Color32 = egui::Color32::from_rgb(0x2A, 0x3A, 0x22);
#[allow(dead_code)]
pub const BORDER_ACCENT: egui::Color32 = ACCENT_DIM;

// ── Spacing ──────────────────────────────────────────────────
pub const SPACING_SM: f32 = 4.0;
pub const SPACING_MD: f32 = 8.0;
pub const SPACING_LG: f32 = 16.0;
pub const SPACING_XL: f32 = 24.0;

// ── Typography ───────────────────────────────────────────────
pub const FONT_SIZE_SMALL: f32 = 11.0;
pub const FONT_SIZE_BODY: f32 = 13.0;
pub const FONT_SIZE_HEADING: f32 = 16.0;
pub const FONT_SIZE_TITLE: f32 = 22.0;
pub const FONT_SIZE_DISPLAY: f32 = 32.0;
pub const FONT_SIZE_MONO: f32 = 15.0;

// ── Corner radius (0 = pixel-sharp for retro) ────────────────
pub const ROUNDING: u8 = 0;

// ── Frame helpers ────────────────────────────────────────────

/// Standard panel: beveled retro frame.
pub fn panel_frame() -> egui::Frame {
    egui::Frame::NONE
        .fill(PANEL_BG)
        .corner_radius(ROUNDING)
        .inner_margin(SPACING_LG)
        .stroke(egui::Stroke::new(2.0, BORDER_BRIGHT))
}

/// Accent-bordered panel (primary content / active state).
#[allow(dead_code)]
pub fn accent_panel() -> egui::Frame {
    egui::Frame::NONE
        .fill(PANEL_BG)
        .corner_radius(ROUNDING)
        .inner_margin(SPACING_LG)
        .stroke(egui::Stroke::new(2.0, ACCENT_DIM))
}

/// Inset panel (recessed input areas, readout displays).
pub fn inset_frame() -> egui::Frame {
    egui::Frame::NONE
        .fill(PANEL_INSET)
        .corner_radius(ROUNDING)
        .inner_margin(SPACING_LG)
        .stroke(egui::Stroke::new(1.0, BORDER))
}

/// Header strip.
pub fn header_frame() -> egui::Frame {
    egui::Frame::NONE
        .fill(PANEL_BG)
        .inner_margin(egui::Margin::symmetric(SPACING_LG as i8, SPACING_MD as i8))
        .stroke(egui::Stroke::new(1.0, BORDER_BRIGHT))
}

// ── Component helpers ────────────────────────────────────────

/// Primary action button.
pub fn primary_button(label: &str) -> egui::Button<'_> {
    egui::Button::new(
        egui::RichText::new(label)
            .size(FONT_SIZE_BODY)
            .color(ACCENT_FG),
    )
    .fill(ACCENT)
    .corner_radius(ROUNDING)
    .stroke(egui::Stroke::new(1.0, ACCENT_DIM))
}

/// Danger button.
pub fn danger_button(label: &str) -> egui::Button<'_> {
    egui::Button::new(
        egui::RichText::new(label)
            .size(FONT_SIZE_BODY)
            .color(TEXT_PRIMARY),
    )
    .fill(ERROR)
    .corner_radius(ROUNDING)
}

/// Pulsing status indicator dot.
pub fn status_dot(ui: &mut egui::Ui, color: egui::Color32, pulse: bool, tooltip: &str) {
    let alpha = if pulse {
        let t = ui.ctx().input(|i| i.time);
        ((t * 2.0).sin().abs() as f32 * 0.5 + 0.5) * 255.0
    } else {
        255.0
    };
    let c = egui::Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), alpha as u8);
    ui.label(egui::RichText::new("\u{25A0}").color(c).size(8.0)) // ■ small square
        .on_hover_text(tooltip);
}

/// Section label (uppercase, muted).
pub fn section_label(ui: &mut egui::Ui, text: &str) {
    ui.label(
        egui::RichText::new(format!("[ {} ]", text.to_uppercase()))
            .size(FONT_SIZE_SMALL)
            .color(TEXT_SECONDARY),
    );
}

/// Monospace value display (codes, IDs).
pub fn mono_value(text: &str, size: f32) -> egui::RichText {
    egui::RichText::new(text)
        .size(size)
        .color(ACCENT)
        .monospace()
}

/// Field label + value pair.
pub fn field_row(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(format!("{label}:"))
                .size(FONT_SIZE_BODY)
                .color(TEXT_SECONDARY),
        );
        ui.label(mono_value(value, FONT_SIZE_BODY));
    });
}

pub fn apply_theme(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();

    visuals.window_fill = WINDOW_BG;
    visuals.extreme_bg_color = PANEL_INSET;

    visuals.widgets.noninteractive.bg_fill = PANEL_BG;
    visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0, TEXT_PRIMARY);
    visuals.widgets.noninteractive.corner_radius = egui::CornerRadius::same(ROUNDING);

    visuals.widgets.inactive.bg_fill = PANEL_INSET;
    visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0, TEXT_SECONDARY);
    visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, BORDER);
    visuals.widgets.inactive.corner_radius = egui::CornerRadius::same(ROUNDING);

    visuals.widgets.hovered.bg_fill = SURFACE_RAISED;
    visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.0, ACCENT);
    visuals.widgets.hovered.corner_radius = egui::CornerRadius::same(ROUNDING);

    visuals.widgets.active.bg_fill = ACCENT;
    visuals.widgets.active.fg_stroke = egui::Stroke::new(1.0, ACCENT_FG);
    visuals.widgets.active.corner_radius = egui::CornerRadius::same(ROUNDING);

    visuals.window_corner_radius = egui::CornerRadius::same(0);
    visuals.selection.bg_fill = ACCENT.gamma_multiply(0.15);
    visuals.selection.stroke = egui::Stroke::new(1.0, ACCENT_DIM);

    let mut style = (*ctx.style()).clone();
    style.visuals = visuals;
    style.spacing.item_spacing = egui::vec2(SPACING_MD, SPACING_MD);
    style.spacing.button_padding = egui::vec2(SPACING_LG, SPACING_MD);

    // Monospace everywhere — retro terminal feel.
    use egui::FontId;
    use egui::TextStyle;
    style
        .text_styles
        .insert(TextStyle::Body, FontId::monospace(FONT_SIZE_BODY));
    style
        .text_styles
        .insert(TextStyle::Button, FontId::monospace(FONT_SIZE_BODY));
    style
        .text_styles
        .insert(TextStyle::Heading, FontId::monospace(FONT_SIZE_HEADING));
    style
        .text_styles
        .insert(TextStyle::Monospace, FontId::monospace(FONT_SIZE_MONO));
    style
        .text_styles
        .insert(TextStyle::Small, FontId::monospace(FONT_SIZE_SMALL));

    ctx.set_style(style);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accent_is_phosphor_green() {
        assert_eq!(ACCENT, egui::Color32::from_rgb(0x33, 0xFF, 0x33));
    }

    #[test]
    fn background_is_near_black_green_tint() {
        assert!(WINDOW_BG.r() < 0x10);
        assert!(WINDOW_BG.g() > WINDOW_BG.r());
        assert!(WINDOW_BG.g() > WINDOW_BG.b());
    }

    #[test]
    fn rounding_is_zero_pixel_sharp() {
        assert_eq!(ROUNDING, 0);
    }

    #[test]
    fn text_has_green_tint() {
        // Primary text should have green > red and green > blue
        assert!(TEXT_PRIMARY.g() > TEXT_PRIMARY.r());
    }

    #[test]
    fn spacing_ordered() {
        assert!(SPACING_SM < SPACING_MD);
        assert!(SPACING_MD < SPACING_LG);
        assert!(SPACING_LG < SPACING_XL);
    }

    #[test]
    fn font_sizes_ordered() {
        assert!(FONT_SIZE_SMALL < FONT_SIZE_BODY);
        assert!(FONT_SIZE_BODY < FONT_SIZE_HEADING);
        assert!(FONT_SIZE_HEADING < FONT_SIZE_TITLE);
        assert!(FONT_SIZE_TITLE < FONT_SIZE_DISPLAY);
    }
}
