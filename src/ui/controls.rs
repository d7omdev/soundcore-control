use eframe::egui::{self, Align, Layout, RichText};
use soundcore_control::domain::{ControlSetting, ControlValue, DeviceCommand};

use crate::app::SoundcoreApp;
use crate::ui::theme::TEXT;
use crate::ui::widgets::{card, section_title};

impl SoundcoreApp {
    /// Only ever shown when `SoundcoreApp::has_controls` is true (`tab_bar` hides the
    /// Controls tab, and reroutes `active_tab` away from it, otherwise), so at least one of
    /// `daily_controls`/`button_controls` is guaranteed non-empty here.
    pub(crate) fn controls_card(&mut self, ui: &mut egui::Ui) {
        let daily_controls = self.snapshot.daily_controls.clone();
        let button_controls = self.snapshot.button_controls.clone();
        let connected = self.is_connected();
        let mut commands = Vec::new();

        if !daily_controls.is_empty() {
            card(ui, |ui| {
                section_title(ui, "Daily Controls", "Listening and convenience features");
                ui.add_space(12.0);
                let last = daily_controls.len() - 1;
                for (index, control) in daily_controls.iter().enumerate() {
                    if let Some(command) = control_row(ui, control, connected, index < last) {
                        commands.push(command);
                    }
                }
            });
            ui.add_space(14.0);
        }

        if !button_controls.is_empty() {
            card(ui, |ui| {
                section_title(
                    ui,
                    "Button Controls",
                    "Choose an action for each button or gesture",
                );
                ui.add_space(12.0);
                let last = button_controls.len() - 1;
                for (index, control) in button_controls.iter().enumerate() {
                    if let Some(command) = control_row(ui, control, connected, index < last) {
                        commands.push(command);
                    }
                }
            });
        }

        for command in commands {
            self.send(command);
        }
    }
}

fn control_row(
    ui: &mut egui::Ui,
    control: &ControlSetting,
    enabled: bool,
    draw_separator: bool,
) -> Option<DeviceCommand> {
    let mut command = None;
    ui.horizontal(|ui| {
        ui.label(RichText::new(&control.label).color(TEXT));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            match &control.value {
                ControlValue::Toggle(current) => {
                    let mut value = *current;
                    if ui
                        .add_enabled(
                            enabled,
                            egui::Button::new(if value { "On" } else { "Off" }).selected(value),
                        )
                        .clicked()
                    {
                        value = !value;
                        command = Some(DeviceCommand::SetToggle(control.id, value));
                    }
                }
                ControlValue::Select {
                    value,
                    options,
                    optional,
                } => {
                    let selected_label = value
                        .as_deref()
                        .and_then(|selected| {
                            options
                                .iter()
                                .find(|option| option.value == selected)
                                .map(|option| option.label.as_str())
                        })
                        .unwrap_or("Disabled");
                    egui::ComboBox::from_id_salt(format!("control-{:?}", control.id))
                        .selected_text(selected_label)
                        .width(ui.available_width().min(210.0))
                        .show_ui(ui, |ui| {
                            if *optional
                                && ui.selectable_label(value.is_none(), "Disabled").clicked()
                            {
                                command = Some(DeviceCommand::SetSelect {
                                    id: control.id,
                                    value: None,
                                    optional: true,
                                });
                            }
                            for option in options {
                                if ui
                                    .selectable_label(
                                        value.as_deref() == Some(&option.value),
                                        &option.label,
                                    )
                                    .clicked()
                                {
                                    command = Some(DeviceCommand::SetSelect {
                                        id: control.id,
                                        value: Some(option.value.clone()),
                                        optional: *optional,
                                    });
                                }
                            }
                        });
                }
            }
        });
    });
    if draw_separator {
        ui.add_space(6.0);
        ui.separator();
        ui.add_space(6.0);
    }
    command
}
