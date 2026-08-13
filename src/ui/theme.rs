use eframe::egui::{self, Color32, Vec2};

pub(crate) const BG: Color32 = Color32::BLACK;
pub(crate) const HERO: Color32 = Color32::from_rgb(27, 28, 30);
pub(crate) const CARD: Color32 = Color32::from_rgb(27, 28, 30);
pub(crate) const CARD_ALT: Color32 = Color32::from_rgb(45, 45, 48);
pub(crate) const TEXT: Color32 = Color32::from_rgb(247, 247, 249);
pub(crate) const MUTED: Color32 = Color32::from_rgb(166, 166, 174);
pub(crate) const ACCENT: Color32 = Color32::from_rgb(25, 190, 235);
pub(crate) const EQ_ACCENT: Color32 = Color32::from_rgb(248, 174, 55);
pub(crate) const BATTERY: Color32 = Color32::from_rgb(61, 205, 85);
pub(crate) const WARNING: Color32 = Color32::from_rgb(255, 174, 105);

pub(crate) fn configure_style(context: &egui::Context) {
    let mut style = (*context.style()).clone();
    style.visuals = egui::Visuals::dark();
    style.visuals.panel_fill = BG;
    style.visuals.window_fill = CARD;
    style.visuals.widgets.inactive.bg_fill = CARD_ALT;
    style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(43, 50, 58);
    style.visuals.widgets.active.bg_fill = ACCENT;
    style.visuals.selection.bg_fill = ACCENT;
    style.visuals.hyperlink_color = ACCENT;
    style.spacing.item_spacing = Vec2::new(10.0, 10.0);
    style.spacing.button_padding = Vec2::new(14.0, 9.0);
    context.set_style(style);
}
