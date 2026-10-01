//! `org.freedesktop.Notifications`, see
//! <https://specifications.freedesktop.org/notification-spec/latest/>.

use std::collections::HashMap;
use zbus::zvariant::Value;

pub const BUS_NAME: &str = "org.freedesktop.Notifications";
pub const OBJECT_PATH: &str = "/org/freedesktop/Notifications";
pub const INTERFACE: &str = "org.freedesktop.Notifications";
pub const SPEC_VERSION: &str = "1.2";

/// Reason sent with the `NotificationClosed` signal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum CloseReason {
    Expired = 1,
    Dismissed = 2,
    /// closed by a `CloseNotification` call
    Closed = 3,
    Undefined = 4,
}

/// Client side of the interface, used by durstctl and the tests.
#[zbus::proxy(
    interface = "org.freedesktop.Notifications",
    default_service = "org.freedesktop.Notifications",
    default_path = "/org/freedesktop/Notifications"
)]
pub trait Notifications {
    #[allow(clippy::too_many_arguments)]
    fn notify(
        &self,
        app_name: &str,
        replaces_id: u32,
        app_icon: &str,
        summary: &str,
        body: &str,
        actions: &[&str],
        hints: HashMap<&str, &Value<'_>>,
        expire_timeout: i32,
    ) -> zbus::Result<u32>;

    fn close_notification(&self, id: u32) -> zbus::Result<()>;

    fn get_capabilities(&self) -> zbus::Result<Vec<String>>;

    fn get_server_information(&self) -> zbus::Result<(String, String, String, String)>;

    #[zbus(signal)]
    fn notification_closed(&self, id: u32, reason: u32) -> zbus::Result<()>;

    #[zbus(signal)]
    fn action_invoked(&self, id: u32, action_key: &str) -> zbus::Result<()>;
}
