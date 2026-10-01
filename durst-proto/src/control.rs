//! `org.durst_notification.Durst1`: durst's own control interface, used by
//! durstctl and usable with `busctl --user` from scripts.

use serde::{Deserialize, Serialize};
use zbus::zvariant::Type;

pub const BUS_NAME: &str = "org.durst_notification.Durst";
pub const OBJECT_PATH: &str = "/org/durst_notification/Durst";
pub const INTERFACE: &str = "org.durst_notification.Durst1";

/// Error names returned by the daemon, `org.durst_notification.Error.<name>`.
pub mod error {
    /// there is nothing to act on, e.g. no notification or empty history
    pub const NOT_FOUND: &str = "org.durst_notification.Error.NotFound";
    /// the config file could not be loaded; the old config stays active
    pub const INVALID_CONFIG: &str = "org.durst_notification.Error.InvalidConfig";
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct NotificationInfo {
    pub id: u32,
    pub app_name: String,
    pub summary: String,
    pub body: String,
    /// "low", "normal" or "critical"
    pub urgency: String,
    pub category: String,
    /// (key, label)
    pub actions: Vec<(String, String)>,
    /// unix time in seconds
    pub received: u64,
    /// "displayed", "waiting" or "history"
    pub state: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct DaemonInfo {
    pub version: String,
    /// the config file; it may not exist (defaults are used then)
    pub config_path: String,
    pub displayed: u32,
    pub waiting: u32,
    pub history: u32,
}

#[zbus::proxy(
    interface = "org.durst_notification.Durst1",
    default_service = "org.durst_notification.Durst",
    default_path = "/org/durst_notification/Durst"
)]
pub trait Durst {
    /// displayed and waiting notifications
    fn list(&self) -> zbus::Result<Vec<NotificationInfo>>;

    /// closes `id`, or the newest displayed notification if `id` is 0
    fn close(&self, id: u32) -> zbus::Result<()>;

    fn close_all(&self) -> zbus::Result<()>;

    /// invokes `key` of `id` (0: newest displayed); an empty key means the
    /// default action, like a middle click
    fn action(&self, id: u32, key: &str) -> zbus::Result<()>;

    /// closed notifications, newest first
    fn history(&self) -> zbus::Result<Vec<NotificationInfo>>;

    /// shows the newest notification from the history again, returns its id
    fn history_pop(&self) -> zbus::Result<u32>;

    fn history_clear(&self) -> zbus::Result<()>;

    /// reloads the config file
    fn reload(&self) -> zbus::Result<()>;

    fn info(&self) -> zbus::Result<DaemonInfo>;

    #[zbus(property)]
    fn displayed_count(&self) -> zbus::Result<u32>;

    #[zbus(property)]
    fn waiting_count(&self) -> zbus::Result<u32>;

    #[zbus(property)]
    fn history_count(&self) -> zbus::Result<u32>;
}
