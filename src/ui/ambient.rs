use eframe::egui::{self, Color32, CornerRadius, FontId, Stroke, Vec2};
use soundcore_control::domain::{DeviceCommand, ListeningMode, parse_listening_mode};

use crate::app::SoundcoreApp;
use crate::ui::theme::{CARD_ALT, HERO, MUTED, TEXT};
use crate::ui::widgets::{card, section_title};

impl SoundcoreApp {
    /// Only ever shown when `SoundcoreApp::has_ambient_options` is true (`tab_bar` hides the
    /// Ambient tab, and reroutes `active_tab` away from it, otherwise), so this can assume
    /// there's always something to render.
    pub(crate) fn sound_mode_card(&mut self, ui: &mut egui::Ui) {
        card(ui, |ui| {
            if self.supports_manual_ambient_ranges {
                self.ambient_slider_section(ui);
            } else {
                self.ambient_mode_picker_section(ui);
            }
        });
    }

    /// Continuous 1-10 ambient slider, for devices that expose manual noise-canceling and
    /// transparency ranges (see `DeviceProfile::supports_manual_ambient_ranges`).
    fn ambient_slider_section(&mut self, ui: &mut egui::Ui) {
        section_title(
            ui,
            "Ambient Sound",
            "Slide from transparency to noise canceling",
        );
        ui.add_space(18.0);
        match ambient_slider(
            ui,
            self.snapshot.ambient_level,
            self.snapshot.listening_mode,
            self.is_connected(),
        ) {
            Some(AmbientSelection::Level(level)) => {
                self.snapshot.ambient_level = Some(level);
                self.snapshot.listening_mode = if level <= 5 {
                    ListeningMode::Transparency
                } else {
                    ListeningMode::NoiseCanceling
                };
                self.send(DeviceCommand::SetAmbientLevel(level));
            }
            Some(AmbientSelection::Normal) => {
                self.snapshot.listening_mode = ListeningMode::Normal;
                self.snapshot.ambient_level = None;
                self.send(DeviceCommand::SetListeningMode(ListeningMode::Normal));
            }
            None => {}
        }
        ui.add_space(8.0);
        let (labels, _) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), 18.0), egui::Sense::hover());
        let font = FontId::proportional(12.0);
        ui.painter().text(
            labels.left_center(),
            egui::Align2::LEFT_CENTER,
            "Transparency",
            font.clone(),
            MUTED,
        );
        ui.painter().text(
            labels.center(),
            egui::Align2::CENTER_CENTER,
            "Normal",
            font.clone(),
            MUTED,
        );
        ui.painter().text(
            labels.right_center(),
            egui::Align2::RIGHT_CENTER,
            "Noise Canceling",
            font,
            MUTED,
        );
    }

    /// Mode-only picker for devices with discrete ambient modes but no manual intensity
    /// slider (e.g. the R60i NC only exposes Normal/Transparency/Noise Canceling as fixed
    /// modes, not a continuous 1-10 range).
    fn ambient_mode_picker_section(&mut self, ui: &mut egui::Ui) {
        section_title(ui, "Ambient Sound", "Choose a listening mode");
        ui.add_space(18.0);
        let connected = self.is_connected();
        let options = self.snapshot.mode_options.clone();
        let current = self.snapshot.listening_mode;
        let mut selected = None;
        ui.columns(options.len().max(1), |columns| {
            for (column, option) in columns.iter_mut().zip(&options) {
                let mode = parse_listening_mode(&option.value);
                if column
                    .add_enabled(
                        connected,
                        egui::Button::new(&option.label).selected(mode == current),
                    )
                    .clicked()
                {
                    selected = Some(mode);
                }
            }
        });
        if let Some(mode) = selected {
            self.snapshot.listening_mode = mode;
            self.send(DeviceCommand::SetListeningMode(mode));
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AmbientSelection {
    Level(u8),
    Normal,
}

fn ambient_slider(
    ui: &mut egui::Ui,
    current: Option<u8>,
    mode: ListeningMode,
    enabled: bool,
) -> Option<AmbientSelection> {
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), 64.0),
        if enabled {
            egui::Sense::click_and_drag()
        } else {
            egui::Sense::hover()
        },
    );
    let painter = ui.painter();
    let slot_width = rect.width() / 11.0;
    let normal_selected = mode == ListeningMode::Normal;
    let normal_active = normal_selected || current.is_some_and(|level| level >= 6);

    for slot in 0..11 {
        let level = match slot {
            0..=4 => Some(slot + 1),
            5 => None,
            _ => Some(slot),
        };
        let segment = egui::Rect::from_min_max(
            egui::pos2(rect.left() + slot as f32 * slot_width, rect.top()),
            egui::pos2(rect.left() + (slot + 1) as f32 * slot_width, rect.bottom()),
        );
        let rounding = if slot == 0 {
            CornerRadius {
                nw: 14,
                sw: 14,
                ..Default::default()
            }
        } else if slot == 10 {
            CornerRadius {
                ne: 14,
                se: 14,
                ..Default::default()
            }
        } else {
            CornerRadius::ZERO
        };
        let is_colored = level.is_some_and(|level| {
            if normal_selected {
                level <= 5
            } else {
                current.is_some_and(|selected| level <= usize::from(selected))
            }
        });
        let fill = match level {
            Some(level) if is_colored => ambient_gradient((level as f32 - 0.5) / 10.0),
            None if normal_active => ambient_gradient(0.5),
            _ => CARD_ALT,
        };
        painter.rect_filled(segment, rounding, fill);
        if slot > 0 {
            painter.line_segment(
                [segment.left_top(), segment.left_bottom()],
                Stroke::new(1.0_f32, Color32::from_white_alpha(52)),
            );
        }
        if let Some(level) = level {
            painter.text(
                segment.center(),
                egui::Align2::CENTER_CENTER,
                level.to_string(),
                FontId::proportional(16.0),
                if is_colored { Color32::WHITE } else { MUTED },
            );
        }
    }

    let normal_center = egui::pos2(rect.left() + slot_width * 5.5, rect.center().y);
    painter.circle_filled(normal_center, 19.0, HERO);
    painter.circle_filled(
        normal_center,
        16.0,
        if normal_active {
            ambient_gradient(0.5)
        } else {
            CARD_ALT
        },
    );
    painter.circle_stroke(
        normal_center,
        16.0,
        Stroke::new(
            1.5_f32,
            if normal_selected {
                Color32::WHITE
            } else if normal_active {
                Color32::from_white_alpha(180)
            } else {
                Color32::from_rgb(115, 139, 153)
            },
        ),
    );
    draw_normal_icon(
        painter,
        normal_center,
        if normal_active { Color32::WHITE } else { TEXT },
    );

    if enabled && (response.clicked() || response.dragged()) {
        let pointer = response.interact_pointer_pos()?;
        let slot = (((pointer.x - rect.left()) / slot_width).floor() as i32).clamp(0, 10);
        if slot == 5 {
            return (mode != ListeningMode::Normal).then_some(AmbientSelection::Normal);
        }
        let level = if slot < 5 { slot + 1 } else { slot } as u8;
        if current != Some(level) || mode == ListeningMode::Normal {
            return Some(AmbientSelection::Level(level));
        }
    }
    None
}

fn ambient_gradient(position: f32) -> Color32 {
    let transparency = Color32::from_rgb(75, 214, 246);
    let neutral = Color32::from_rgb(65, 113, 143);
    let noise_canceling = Color32::from_rgb(42, 101, 235);
    if position <= 0.5 {
        lerp_color(transparency, neutral, position * 2.0)
    } else {
        lerp_color(neutral, noise_canceling, (position - 0.5) * 2.0)
    }
}

fn lerp_color(from: Color32, to: Color32, amount: f32) -> Color32 {
    let amount = amount.clamp(0.0, 1.0);
    let mix = |start: u8, end: u8| {
        (f32::from(start) + (f32::from(end) - f32::from(start)) * amount).round() as u8
    };
    Color32::from_rgb(
        mix(from.r(), to.r()),
        mix(from.g(), to.g()),
        mix(from.b(), to.b()),
    )
}

fn draw_normal_icon(painter: &egui::Painter, center: egui::Pos2, color: Color32) {
    let stroke = Stroke::new(2.0_f32, color);
    painter.circle_stroke(center + Vec2::new(0.0, -5.0), 4.0, stroke);
    painter.add(egui::Shape::line(
        vec![
            center + Vec2::new(-8.0, 8.0),
            center + Vec2::new(-6.0, 4.0),
            center + Vec2::new(-3.0, 2.0),
            center + Vec2::new(3.0, 2.0),
            center + Vec2::new(6.0, 4.0),
            center + Vec2::new(8.0, 8.0),
        ],
        stroke,
    ));
}
