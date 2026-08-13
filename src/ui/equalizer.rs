use eframe::egui::{self, Align, Layout, RichText, Vec2};
use soundcore_control::domain::DeviceCommand;

use crate::app::SoundcoreApp;
use crate::ui::theme::{EQ_ACCENT, MUTED};
use crate::ui::widgets::{card, format_frequency, preset_label, section_title};

impl SoundcoreApp {
    pub(crate) fn equalizer_card(&mut self, ui: &mut egui::Ui) {
        card(ui, |ui| {
            ui.visuals_mut().selection.bg_fill = EQ_ACCENT;
            section_title(ui, "Equalizer", "Tune the sound to you");
            ui.add_space(12.0);
            let current = self
                .snapshot
                .selected_preset
                .clone()
                .unwrap_or_else(|| "Custom".into());
            let options = self.snapshot.preset_options.clone();
            egui::ComboBox::from_id_salt("equalizer-preset")
                .selected_text(preset_label(&options, &current))
                .width(ui.available_width().min(260.0))
                .show_ui(ui, |ui| {
                    ui.set_min_width(240.0);
                    for option in options {
                        if ui
                            .selectable_label(option.value == current, &option.label)
                            .clicked()
                        {
                            self.snapshot.selected_preset = Some(option.value.clone());
                            self.send(DeviceCommand::SetPreset(option.value));
                        }
                    }
                });
            ui.add_space(18.0);

            let connected = self.is_connected();
            let mut changed_gains = None;
            if let Some(equalizer) = &mut self.snapshot.equalizer {
                let mut changed = false;
                let band_count = equalizer.gains.len().max(1) as f32;
                let band_width = (ui.available_width() / band_count).max(34.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    for (index, gain) in equalizer.gains.iter_mut().enumerate() {
                        ui.allocate_ui_with_layout(
                            Vec2::new(band_width, 190.0),
                            Layout::top_down(Align::Center),
                            |ui| {
                                ui.spacing_mut().slider_width = 120.0;
                                let db = *gain as f32
                                    / 10_f32.powi(i32::from(equalizer.fraction_digits));
                                ui.label(
                                    RichText::new(format!("{db:+.1}")).size(11.0).color(MUTED),
                                );
                                let response = ui.add_enabled(
                                    connected,
                                    egui::Slider::new(gain, equalizer.min..=equalizer.max)
                                        .vertical()
                                        .show_value(false),
                                );
                                changed |= response.changed();
                                let frequency =
                                    equalizer.frequencies_hz.get(index).copied().unwrap_or(0);
                                ui.label(
                                    RichText::new(format_frequency(frequency))
                                        .size(11.0)
                                        .color(MUTED),
                                );
                            },
                        );
                    }
                });
                if changed {
                    changed_gains = Some(equalizer.gains.clone());
                }
            } else {
                ui.label(
                    RichText::new("Equalizer data will appear after connection.").color(MUTED),
                );
                ui.add_space(72.0);
            }
            if let Some(gains) = changed_gains {
                self.snapshot.selected_preset = None;
                self.send(DeviceCommand::SetEqualizer(gains));
            }
        });
    }
}
