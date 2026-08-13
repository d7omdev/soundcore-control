use std::{sync::mpsc, thread};

use ksni::{TrayMethods, menu::StandardItem};
use tokio::sync::mpsc as tokio_mpsc;

use crate::domain::{DeviceSnapshot, ListeningMode};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrayAction {
    ShowWindow,
    Disconnect,
    SetListeningMode(ListeningMode),
    Quit,
}

/// Shown in the tray title/tooltip/menu when no specific device is known yet
/// (searching, disconnected, failed).
const APP_NAME: &str = "Liberty Control";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrayState {
    device_name: Option<String>,
    status: String,
    connected: bool,
    battery_left: Option<u8>,
    battery_right: Option<u8>,
    battery_case: Option<u8>,
    listening_mode: Option<ListeningMode>,
}

impl Default for TrayState {
    fn default() -> Self {
        Self::disconnected()
    }
}

impl TrayState {
    #[must_use]
    pub fn searching() -> Self {
        Self {
            status: "Looking for earbuds…".into(),
            ..Self::disconnected()
        }
    }

    #[must_use]
    pub fn connecting(device_name: &str) -> Self {
        Self {
            device_name: Some(device_name.to_owned()),
            status: "Connecting…".into(),
            ..Self::disconnected()
        }
    }

    #[must_use]
    pub fn disconnected() -> Self {
        Self {
            device_name: None,
            status: "Disconnected".into(),
            connected: false,
            battery_left: None,
            battery_right: None,
            battery_case: None,
            listening_mode: None,
        }
    }

    #[must_use]
    pub fn failed() -> Self {
        Self {
            status: "Connection needs attention".into(),
            ..Self::disconnected()
        }
    }

    #[must_use]
    pub fn connected(device_name: &str, snapshot: &DeviceSnapshot) -> Self {
        Self {
            device_name: Some(device_name.to_owned()),
            status: "Connected".into(),
            connected: true,
            battery_left: snapshot.battery_left,
            battery_right: snapshot.battery_right,
            battery_case: snapshot.battery_case,
            listening_mode: Some(snapshot.listening_mode),
        }
    }

    fn device_label(&self) -> &str {
        self.device_name.as_deref().unwrap_or(APP_NAME)
    }
}

pub struct TrayController {
    state_sender: tokio_mpsc::UnboundedSender<TrayState>,
    action_receiver: mpsc::Receiver<TrayAction>,
}

impl TrayController {
    #[must_use]
    pub fn spawn_for_ui(egui_context: eframe::egui::Context) -> Self {
        Self::spawn(Some(egui_context))
    }

    #[must_use]
    pub fn spawn_background() -> Self {
        Self::spawn(None)
    }

    fn spawn(egui_context: Option<eframe::egui::Context>) -> Self {
        let (state_sender, state_receiver) = tokio_mpsc::unbounded_channel();
        let (action_sender, action_receiver) = mpsc::channel();

        if let Err(error) = thread::Builder::new()
            .name("liberty-tray".into())
            .spawn(move || run_tray(state_receiver, action_sender, egui_context))
        {
            tracing::error!(%error, "could not start tray thread");
        }

        Self {
            state_sender,
            action_receiver,
        }
    }

    pub fn update(&self, state: TrayState) {
        let _ = self.state_sender.send(state);
    }

    pub fn try_recv(&self) -> Option<TrayAction> {
        self.action_receiver.try_recv().ok()
    }
}

fn run_tray(
    mut states: tokio_mpsc::UnboundedReceiver<TrayState>,
    actions: mpsc::Sender<TrayAction>,
    egui_context: Option<eframe::egui::Context>,
) {
    let runtime = match tokio::runtime::Runtime::new() {
        Ok(runtime) => runtime,
        Err(error) => {
            tracing::error!(%error, "could not start tray runtime");
            return;
        }
    };

    runtime.block_on(async move {
        let tray = LibertyTray {
            state: TrayState::default(),
            actions,
            egui_context,
            icons: tray_icons(),
        };
        let handle = match tray.assume_sni_available(true).spawn().await {
            Ok(handle) => handle,
            Err(error) => {
                tracing::error!(%error, "could not create system tray icon");
                return;
            }
        };

        while let Some(state) = states.recv().await {
            if handle
                .update(move |tray: &mut LibertyTray| tray.state = state)
                .await
                .is_none()
            {
                return;
            }
        }
    });
}

struct LibertyTray {
    state: TrayState,
    actions: mpsc::Sender<TrayAction>,
    egui_context: Option<eframe::egui::Context>,
    icons: Vec<ksni::Icon>,
}

impl LibertyTray {
    fn send_action(&self, action: TrayAction) {
        let _ = self.actions.send(action);
        if let Some(context) = &self.egui_context {
            context.request_repaint();
        }
    }
}

impl ksni::Tray for LibertyTray {
    const MENU_ON_ACTIVATE: bool = true;

    fn id(&self) -> String {
        "liberty-control".into()
    }

    fn category(&self) -> ksni::Category {
        ksni::Category::Hardware
    }

    fn title(&self) -> String {
        format!("{} · {}", self.state.device_label(), self.state.status)
    }

    fn status(&self) -> ksni::Status {
        if self.state.connected {
            ksni::Status::Active
        } else {
            ksni::Status::NeedsAttention
        }
    }

    fn icon_name(&self) -> String {
        "liberty-control".into()
    }

    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        self.icons.clone()
    }

    fn tool_tip(&self) -> ksni::ToolTip {
        ksni::ToolTip {
            icon_name: self.icon_name(),
            icon_pixmap: self.icon_pixmap(),
            title: self.state.device_label().to_owned(),
            description: format!(
                "{} · L {} · R {} · Case {}",
                self.state.status,
                battery_value(self.state.battery_left),
                battery_value(self.state.battery_right),
                battery_value(self.state.battery_case),
            ),
        }
    }

    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        use ksni::menu::MenuItem;

        let selected_mode = self.state.listening_mode;
        vec![
            StandardItem {
                label: "Open Liberty Control".into(),
                activate: Box::new(|tray: &mut LibertyTray| {
                    tray.send_action(TrayAction::ShowWindow);
                }),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            info_item(format!(
                "{} — {}",
                self.state.device_label(),
                self.state.status
            )),
            info_item(format!(
                "L {}   R {}   Case {}",
                battery_value(self.state.battery_left),
                battery_value(self.state.battery_right),
                battery_value(self.state.battery_case),
            )),
            MenuItem::Separator,
            ksni::menu::SubMenu {
                label: format!(
                    "Ambient mode: {}",
                    selected_mode.map_or("—", listening_mode_label)
                ),
                enabled: self.state.connected,
                submenu: vec![
                    ksni::menu::RadioGroup {
                        selected: match selected_mode {
                            Some(ListeningMode::Transparency) | None => 0,
                            Some(ListeningMode::Normal) => 1,
                            Some(ListeningMode::NoiseCanceling) => 2,
                        },
                        select: Box::new(|tray: &mut LibertyTray, selected| {
                            let mode = match selected {
                                0 => ListeningMode::Transparency,
                                1 => ListeningMode::Normal,
                                _ => ListeningMode::NoiseCanceling,
                            };
                            tray.send_action(TrayAction::SetListeningMode(mode));
                        }),
                        options: ["Transparency", "Normal", "Noise Canceling"]
                            .into_iter()
                            .map(|label| ksni::menu::RadioItem {
                                label: label.into(),
                                ..Default::default()
                            })
                            .collect(),
                    }
                    .into(),
                ],
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: "Disconnect".into(),
                enabled: self.state.connected,
                activate: Box::new(|tray: &mut LibertyTray| {
                    tray.send_action(TrayAction::Disconnect);
                }),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: "Quit Liberty Control".into(),
                activate: Box::new(|tray: &mut LibertyTray| {
                    tray.send_action(TrayAction::Quit);
                }),
                ..Default::default()
            }
            .into(),
        ]
    }
}

fn listening_mode_label(mode: ListeningMode) -> &'static str {
    match mode {
        ListeningMode::NoiseCanceling => "Noise Canceling",
        ListeningMode::Transparency => "Transparency",
        ListeningMode::Normal => "Normal",
    }
}

fn info_item(label: String) -> ksni::MenuItem<LibertyTray> {
    StandardItem {
        label,
        enabled: false,
        ..Default::default()
    }
    .into()
}

fn battery_value(value: Option<u8>) -> String {
    value.map_or_else(|| "—".into(), |value| format!("{value}%"))
}

fn tray_icons() -> Vec<ksni::Icon> {
    let Ok(source) = image::load_from_memory(include_bytes!("../assets/launcher-logo.png")) else {
        return Vec::new();
    };
    let source = source.to_rgba8();

    [32_u32, 64_u32]
        .into_iter()
        .map(|size| {
            let mut data =
                image::imageops::resize(&source, size, size, image::imageops::FilterType::Lanczos3)
                    .into_raw();
            for pixel in data.chunks_exact_mut(4) {
                pixel.rotate_right(1);
            }
            ksni::Icon {
                width: size as i32,
                height: size as i32,
                data,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connected_state_formats_battery_and_mode() {
        let snapshot = DeviceSnapshot {
            battery_left: Some(94),
            battery_right: Some(90),
            battery_case: Some(80),
            listening_mode: ListeningMode::Normal,
            ..DeviceSnapshot::default()
        };

        let state = TrayState::connected("Test Earbuds", &snapshot);

        assert!(state.connected);
        assert_eq!(state.device_label(), "Test Earbuds");
        assert_eq!(state.listening_mode, Some(ListeningMode::Normal));
        assert_eq!(battery_value(state.battery_left), "94%");
    }
}
