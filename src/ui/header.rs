use eframe::egui::{self, Align, CornerRadius, Layout, Margin, RichText, Vec2};

use crate::app::{ConnectionView, SoundcoreApp};
use crate::ui::theme::{BATTERY, HERO, MUTED, TEXT, WARNING};
use crate::ui::widgets::{TabIcon, battery, draw_buds, status_dot, tab_button};

impl SoundcoreApp {
    pub(crate) fn header(&mut self, ui: &mut egui::Ui) {
        egui::Frame::new()
            .fill(HERO)
            .corner_radius(CornerRadius::same(22))
            .inner_margin(Margin::same(24))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    if let Some(texture) = &self.soundcore_texture {
                        ui.add(
                            egui::Image::new(texture)
                                .fit_to_exact_size(Vec2::new(126.0, 24.0))
                                .maintain_aspect_ratio(true),
                        );
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if self.connection != ConnectionView::Connected
                            && ui.button("Try again").clicked()
                        {
                            self.reconnect();
                        }
                    });
                });
                ui.add_space(4.0);
                ui.vertical_centered(|ui| {
                    ui.heading(RichText::new(&self.device_name).size(27.0).color(TEXT));
                    ui.add_space(8.0);
                    let icon_texture = self
                        .current_icon_key
                        .as_ref()
                        .and_then(|key| self.icon_textures.get(key));
                    draw_buds(ui, icon_texture);
                    ui.add_space(14.0);
                    if let Some(level) = self.snapshot.battery_single {
                        ui.columns(1, |columns| {
                            battery(&mut columns[0], "Battery", Some(level));
                        });
                    } else {
                        ui.columns(3, |columns| {
                            battery(&mut columns[0], "L", self.snapshot.battery_left);
                            battery(&mut columns[1], "R", self.snapshot.battery_right);
                            battery(&mut columns[2], "Case", self.snapshot.battery_case);
                        });
                    }
                    ui.add_space(8.0);
                    let (color, text) = match self.connection {
                        ConnectionView::Searching => (WARNING, "Looking for earbuds…"),
                        ConnectionView::Connecting => (WARNING, "Opening controls…"),
                        ConnectionView::Connected => (BATTERY, "Connected"),
                        ConnectionView::Disconnected => (WARNING, "Disconnected"),
                        ConnectionView::Failed => (WARNING, "Needs attention"),
                    };
                    ui.horizontal(|ui| {
                        status_dot(ui, color);
                        ui.label(RichText::new(text).color(MUTED));
                    });
                });
            });
    }

    pub(crate) fn tab_bar(&mut self, ui: &mut egui::Ui) {
        use crate::app::ActiveTab;

        let tabs: Vec<(ActiveTab, TabIcon, &str)> = [
            (ActiveTab::Ambient, TabIcon::Ambient, "Ambient"),
            (ActiveTab::Equalizer, TabIcon::Equalizer, "Equalizer"),
            (ActiveTab::Controls, TabIcon::Controls, "Controls"),
        ]
        .into_iter()
        .filter(|(tab, _, _)| match tab {
            ActiveTab::Ambient => self.has_ambient_options(),
            ActiveTab::Equalizer => true,
            ActiveTab::Controls => self.has_controls(),
        })
        .collect();

        if !tabs.iter().any(|(tab, _, _)| *tab == self.active_tab) {
            if let Some((tab, _, _)) = tabs.first() {
                self.active_tab = *tab;
            }
        }

        ui.columns(tabs.len().max(1), |columns| {
            for (column, (tab, icon, label)) in columns.iter_mut().zip(tabs) {
                let response = tab_button(column, icon, label, self.active_tab == tab);
                if response.clicked() {
                    self.active_tab = tab;
                }
            }
        });
    }
}
