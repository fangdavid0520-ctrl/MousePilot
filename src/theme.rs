use std::fs;

use eframe::egui::{
    self, Color32, FontData, FontDefinitions, FontFamily, FontId, Margin, Stroke, StrokeKind,
    TextStyle,
};

pub const BG: Color32 = Color32::from_rgb(0x0A, 0x0E, 0x13);
pub const CARD: Color32 = Color32::from_rgb(0x11, 0x17, 0x1F);
pub const CARD_SOFT: Color32 = Color32::from_rgb(0x15, 0x1C, 0x25);
pub const BORDER: Color32 = Color32::from_rgb(0x24, 0x2E, 0x3A);
pub const TEXT: Color32 = Color32::from_rgb(0xED, 0xF2, 0xF6);
pub const MUTED: Color32 = Color32::from_rgb(0x87, 0x96, 0xA7);
pub const FAINT: Color32 = Color32::from_rgb(0x55, 0x64, 0x75);
pub const ACCENT: Color32 = Color32::from_rgb(0xC4, 0xF0, 0x58);
pub const ACCENT_DARK: Color32 = Color32::from_rgb(0x18, 0x22, 0x12);
pub const CYAN: Color32 = Color32::from_rgb(0x5B, 0xD9, 0xF2);
pub const AMBER: Color32 = Color32::from_rgb(0xF4, 0xC2, 0x51);
pub const RED: Color32 = Color32::from_rgb(0xFF, 0x6E, 0x6E);

pub fn install(ctx: &egui::Context) {
    install_fonts(ctx);

    let mut style = (*ctx.style()).clone();
    style.visuals = egui::Visuals::dark();
    style.visuals.override_text_color = Some(TEXT);
    style.visuals.panel_fill = BG;
    style.visuals.window_fill = BG;
    style.visuals.extreme_bg_color = Color32::from_rgb(0x08, 0x0C, 0x10);
    style.visuals.faint_bg_color = CARD_SOFT;
    style.visuals.selection.bg_fill = Color32::from_rgba_unmultiplied(0xC4, 0xF0, 0x58, 48);
    style.visuals.selection.stroke = Stroke::new(1.0_f32, ACCENT);
    style.visuals.widgets.noninteractive.bg_fill = CARD;
    style.visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, BORDER);
    style.visuals.widgets.inactive.bg_fill = CARD_SOFT;
    style.visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, BORDER);
    style.visuals.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, TEXT);
    style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(0x1B, 0x25, 0x30);
    style.visuals.widgets.hovered.bg_stroke =
        Stroke::new(1.0_f32, Color32::from_rgb(0x4C, 0x60, 0x70));
    style.visuals.widgets.active.bg_fill = Color32::from_rgb(0x24, 0x30, 0x3D);
    style.visuals.widgets.active.bg_stroke = Stroke::new(1.0_f32, ACCENT);
    style.visuals.widgets.open.bg_fill = CARD_SOFT;
    style.visuals.slider_trailing_fill = true;
    style.spacing.item_spacing = egui::vec2(10.0, 10.0);
    style.spacing.button_padding = egui::vec2(14.0, 8.0);
    style.spacing.interact_size.y = 34.0;
    style
        .text_styles
        .insert(TextStyle::Body, FontId::proportional(14.0));
    style
        .text_styles
        .insert(TextStyle::Button, FontId::proportional(14.0));
    style
        .text_styles
        .insert(TextStyle::Heading, FontId::proportional(22.0));
    style
        .text_styles
        .insert(TextStyle::Small, FontId::proportional(12.0));
    style
        .text_styles
        .insert(TextStyle::Monospace, FontId::monospace(14.0));
    ctx.set_style(style);
}

fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let mut cjk_loaded = false;
    let mut mono_loaded = false;
    let mut ui_loaded = false;

    for path in [
        r"C:\Windows\Fonts\simhei.ttf",
        r"C:\Windows\Fonts\Deng.ttf",
        r"C:\Windows\Fonts\msyh.ttc",
    ] {
        let Ok(bytes) = fs::read(path) else {
            continue;
        };
        fonts.font_data.insert(
            "mousepilot-cjk".to_owned(),
            FontData::from_owned(bytes).into(),
        );
        if let Some(family) = fonts.families.get_mut(&FontFamily::Proportional) {
            family.insert(0, "mousepilot-cjk".to_owned());
        }
        if let Some(family) = fonts.families.get_mut(&FontFamily::Monospace) {
            family.push("mousepilot-cjk".to_owned());
        }
        cjk_loaded = true;
        break;
    }

    if let Ok(bytes) = fs::read(r"C:\Windows\Fonts\segoeui.ttf") {
        fonts.font_data.insert(
            "mousepilot-ui".to_owned(),
            FontData::from_owned(bytes).into(),
        );
        if let Some(family) = fonts.families.get_mut(&FontFamily::Proportional) {
            family.insert(0, "mousepilot-ui".to_owned());
        }
        ui_loaded = true;
    }

    if let Ok(bytes) = fs::read(r"C:\Windows\Fonts\consola.ttf") {
        fonts.font_data.insert(
            "mousepilot-mono".to_owned(),
            FontData::from_owned(bytes).into(),
        );
        if let Some(family) = fonts.families.get_mut(&FontFamily::Monospace) {
            family.insert(0, "mousepilot-mono".to_owned());
        }
        mono_loaded = true;
    }

    if cjk_loaded || mono_loaded || ui_loaded {
        ctx.set_fonts(fonts);
    }
}

pub fn card() -> egui::Frame {
    egui::Frame::new()
        .fill(CARD)
        .stroke(Stroke::new(1.0_f32, BORDER))
        .corner_radius(egui::CornerRadius::same(18))
        .inner_margin(Margin::same(18))
}

pub fn level_color(level: Option<u8>, charging: bool, low: u8) -> Color32 {
    match (level, charging) {
        (_, true) => CYAN,
        (Some(value), false) if value <= low => RED,
        (Some(value), false) if value <= 60 => AMBER,
        (Some(_), false) => ACCENT,
        (None, false) => FAINT,
    }
}

pub fn pill(ui: &mut egui::Ui, text: &str, color: Color32) {
    let font = FontId::proportional(10.5);
    let galley = ui.fonts(|fonts| fonts.layout_no_wrap(text.to_owned(), font, color));
    let width = galley.size().x + 18.0;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 20.0), egui::Sense::hover());
    let painter = ui.painter();
    let fill = Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 26);
    painter.rect_filled(rect, egui::CornerRadius::same(10), fill);
    painter.rect_stroke(
        rect,
        egui::CornerRadius::same(10),
        Stroke::new(
            1.0_f32,
            Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 80),
        ),
        StrokeKind::Inside,
    );
    painter.galley(rect.center() - galley.size() * 0.5, galley, color);
}
