use eframe::egui::{self, Color32, CornerRadius, FontId, Margin, RichText, Stroke, Vec2};

use crate::ui::theme::{ACCENT, BATTERY, CARD, CARD_ALT, MUTED, TEXT};

pub(crate) fn card(ui: &mut egui::Ui, content: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .fill(CARD)
        .corner_radius(CornerRadius::same(18))
        .inner_margin(Margin::same(22))
        .show(ui, content);
}

pub(crate) fn section_title(ui: &mut egui::Ui, title: &str, subtitle: &str) {
    ui.vertical(|ui| {
        ui.label(RichText::new(title).size(19.0).color(TEXT).strong());
        ui.label(RichText::new(subtitle).size(12.0).color(MUTED));
    });
}

pub(crate) fn battery(ui: &mut egui::Ui, label: &str, value: Option<u8>) {
    ui.vertical_centered(|ui| {
        ui.label(RichText::new(label).size(12.0).color(MUTED));
        ui.add_space(5.0);
        ui.label(
            RichText::new(value.map_or_else(|| "—".into(), |value| format!("{value}%")))
                .font(FontId::proportional(25.0))
                .color(TEXT)
                .strong(),
        );
        ui.add_space(6.0);
        ui.add(
            egui::ProgressBar::new(f32::from(value.unwrap_or(0)) / 100.0)
                .fill(BATTERY)
                .desired_width(92.0),
        );
    });
}

pub(crate) fn load_png_texture(
    context: &egui::Context,
    name: &str,
    bytes: &[u8],
) -> Option<egui::TextureHandle> {
    let image = image::load_from_memory(bytes).ok()?.to_rgba8();
    load_rgba_texture(context, name, &image)
}

pub(crate) fn load_soundcore_texture(
    context: &egui::Context,
    bytes: &[u8],
) -> Option<egui::TextureHandle> {
    let image = image::load_from_memory(bytes).ok()?.to_rgba8();
    let width = 378;
    let height = (image.height() * width / image.width()).max(1);
    let image =
        image::imageops::resize(&image, width, height, image::imageops::FilterType::Lanczos3);
    load_rgba_texture(context, "soundcore-logo", &image)
}

fn load_rgba_texture(
    context: &egui::Context,
    name: &str,
    image: &image::RgbaImage,
) -> Option<egui::TextureHandle> {
    let size = [image.width() as usize, image.height() as usize];
    let color_image = egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw());
    let texture_options = egui::TextureOptions {
        mipmap_mode: Some(egui::TextureFilter::Linear),
        ..egui::TextureOptions::LINEAR
    };
    Some(context.load_texture(name, color_image, texture_options))
}

pub(crate) fn draw_buds(ui: &mut egui::Ui, texture: Option<&egui::TextureHandle>) {
    if let Some(texture) = texture {
        ui.add(
            egui::Image::new(texture)
                .fit_to_exact_size(Vec2::new(236.0, 136.0))
                .maintain_aspect_ratio(true),
        );
        return;
    }

    let (rect, _) = ui.allocate_exact_size(Vec2::new(154.0, 108.0), egui::Sense::hover());
    let painter = ui.painter();
    let left = rect.left_center() + Vec2::new(34.0, -8.0);
    let right = rect.right_center() + Vec2::new(-34.0, -8.0);
    for (center, direction) in [(left, -1.0), (right, 1.0)] {
        painter.circle_filled(center, 19.0, CARD_ALT);
        let stem = egui::Rect::from_center_size(
            center + Vec2::new(direction * 8.0, 25.0),
            Vec2::new(14.0, 38.0),
        );
        painter.rect_filled(stem, CornerRadius::same(7), CARD_ALT);
    }
}

#[derive(Clone, Copy)]
pub(crate) enum TabIcon {
    Ambient,
    Equalizer,
    Controls,
}

pub(crate) fn tab_button(
    ui: &mut egui::Ui,
    icon: TabIcon,
    label: &str,
    active: bool,
) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 52.0), egui::Sense::click());
    let padded_rect = rect.shrink2(Vec2::new(4.0, 3.0));
    if response.hovered() {
        ui.painter().rect_filled(padded_rect, 10.0, CARD_ALT);
    }

    let color = if active { TEXT } else { MUTED };
    let font = FontId::proportional(15.0);
    let galley = ui.painter().layout_no_wrap(label.to_owned(), font, color);
    let icon_width = 28.0;
    let gap = 8.0;
    let content_width = icon_width + gap + galley.size().x;
    let content_left = rect.center().x - content_width / 2.0;
    let icon_center = egui::pos2(content_left + icon_width / 2.0, rect.center().y);
    draw_tab_icon(ui.painter(), icon, icon_center, color);
    ui.painter().galley(
        egui::pos2(
            content_left + icon_width + gap,
            rect.center().y - galley.size().y / 2.0,
        ),
        galley,
        color,
    );

    if active {
        let underline = egui::Rect::from_center_size(
            egui::pos2(rect.center().x, padded_rect.bottom() - 1.5),
            Vec2::new((content_width + 16.0).min(padded_rect.width()), 3.0),
        );
        ui.painter().rect_filled(underline, 2.0, ACCENT);
    }
    response
}

fn draw_tab_icon(painter: &egui::Painter, icon: TabIcon, center: egui::Pos2, color: Color32) {
    match icon {
        TabIcon::Ambient => {
            for (offset, half_height) in [(-9.0, 5.0), (-3.0, 10.0), (3.0, 14.0), (9.0, 7.0)] {
                painter.line_segment(
                    [
                        center + Vec2::new(offset, -half_height),
                        center + Vec2::new(offset, half_height),
                    ],
                    Stroke::new(2.2_f32, color),
                );
            }
        }
        TabIcon::Equalizer => {
            for (offset, knob_y) in [(-8.0, -5.0), (0.0, 6.0), (8.0, -1.0)] {
                painter.line_segment(
                    [
                        center + Vec2::new(offset, -12.0),
                        center + Vec2::new(offset, 12.0),
                    ],
                    Stroke::new(1.8_f32, color),
                );
                painter.circle_filled(center + Vec2::new(offset, knob_y), 3.0, color);
            }
        }
        TabIcon::Controls => {
            painter.circle_stroke(center, 10.0, Stroke::new(2.0_f32, color));
            painter.circle_filled(center, 3.5, color);
            for angle in [
                0.0_f32,
                std::f32::consts::FRAC_PI_2,
                std::f32::consts::PI,
                3.0 * std::f32::consts::FRAC_PI_2,
            ] {
                let direction = Vec2::angled(angle);
                painter.line_segment(
                    [center + direction * 10.0, center + direction * 14.0],
                    Stroke::new(2.0_f32, color),
                );
            }
        }
    }
}

pub(crate) fn status_dot(ui: &mut egui::Ui, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(10.0), egui::Sense::hover());
    ui.painter().circle_filled(rect.center(), 4.0, color);
}

pub(crate) fn preset_label(
    options: &[soundcore_control::domain::SelectOption],
    value: &str,
) -> String {
    options
        .iter()
        .find(|option| option.value == value)
        .map(|option| option.label.clone())
        .unwrap_or_else(|| value.to_owned())
}

pub(crate) fn format_frequency(frequency: u16) -> String {
    if frequency >= 1_000 {
        format!("{}k", frequency / 1_000)
    } else {
        frequency.to_string()
    }
}
