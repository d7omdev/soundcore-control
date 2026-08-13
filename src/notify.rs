use notify_rust::{Notification, Timeout};

use crate::domain::DeviceSnapshot;

pub fn buds_connected(name: &str, snapshot: &DeviceSnapshot) {
    let body = format!(
        "L {} · R {} · Case {}",
        battery(snapshot.battery_left),
        battery(snapshot.battery_right),
        battery(snapshot.battery_case),
    );
    if let Err(error) = Notification::new()
        .appname("Soundcore Control")
        .summary(&format!("{name} connected"))
        .body(&body)
        .icon("soundcore-control")
        .timeout(Timeout::Milliseconds(6000))
        .show()
    {
        tracing::warn!(%error, "could not show desktop notification");
    }
}

fn battery(value: Option<u8>) -> String {
    value.map_or_else(|| "—".into(), |value| format!("{value}%"))
}
