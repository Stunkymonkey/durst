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
    /// an argument has the wrong form, e.g. a volume of "loud"
    pub const INVALID_ARGUMENT: &str = "org.durst_notification.Error.InvalidArgument";
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
    /// "displayed", "waiting", "held" (by a rule) or "history"
    pub state: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct DaemonInfo {
    pub version: String,
    /// the config file; it may not exist (defaults are used then)
    pub config_path: String,
    pub displayed: u32,
    pub waiting: u32,
    /// held back by rules, e.g. while do-not-disturb is active
    pub held: u32,
    pub history: u32,
    pub modes: Vec<String>,
    /// no input for `idle_threshold`: timeouts are paused
    pub idle: bool,
    /// the session is locked: timeouts are paused
    pub locked: bool,
    /// a focused window is fullscreen
    pub fullscreen: bool,
    pub outputs: Vec<String>,
    /// the output of the focused window, "" if unknown
    pub focused_output: String,
}

/// Volume of the default sink or source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct VolumeInfo {
    /// cubic scale like pavucontrol and wpctl; may exceed 100
    pub percent: u32,
    pub muted: bool,
    /// the device, e.g. "Built-in Audio Analog Stereo"
    pub description: String,
}

/// The media player `durstctl media` acts on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct MediaInfo {
    /// e.g. "Spotify"
    pub player: String,
    /// "playing", "paused" or "stopped"
    pub status: String,
    pub title: String,
    pub artist: String,
    pub album: String,
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

    /// replaces the active modes, returns them
    fn set_modes(&self, modes: &[&str]) -> zbus::Result<Vec<String>>;

    fn enable_mode(&self, mode: &str) -> zbus::Result<Vec<String>>;

    fn disable_mode(&self, mode: &str) -> zbus::Result<Vec<String>>;

    fn toggle_mode(&self, mode: &str) -> zbus::Result<Vec<String>>;

    /// the modes used by any rule, active or not
    fn known_modes(&self) -> zbus::Result<Vec<String>>;

    /// the volume of the default sink, or of the default source if `mic`
    fn volume(&self, mic: bool) -> zbus::Result<VolumeInfo>;

    /// `change`: "50" sets, "+5" / "-5" changes (a trailing "%" is allowed),
    /// "up" / "down" change by the configured step; shows the OSD and
    /// returns the new volume (as sent, before PipeWire applied it)
    fn set_volume(&self, mic: bool, change: &str) -> zbus::Result<VolumeInfo>;

    /// `state`: "on", "off" or "toggle"; shows the OSD
    fn set_mute(&self, mic: bool, state: &str) -> zbus::Result<VolumeInfo>;

    fn show_volume_osd(&self, mic: bool) -> zbus::Result<()>;

    /// the player that most recently started playing, else the last active
    fn media_status(&self) -> zbus::Result<MediaInfo>;

    /// `action`: "play", "pause", "toggle", "next" or "prev", on the player
    /// of `media_status`; shows the media OSD
    fn media_action(&self, action: &str) -> zbus::Result<MediaInfo>;

    fn show_media_osd(&self) -> zbus::Result<()>;

    #[zbus(property)]
    fn active_modes(&self) -> zbus::Result<Vec<String>>;

    #[zbus(property)]
    fn displayed_count(&self) -> zbus::Result<u32>;

    #[zbus(property)]
    fn waiting_count(&self) -> zbus::Result<u32>;

    #[zbus(property)]
    fn held_count(&self) -> zbus::Result<u32>;

    #[zbus(property)]
    fn history_count(&self) -> zbus::Result<u32>;
}
