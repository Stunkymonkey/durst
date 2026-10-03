use clap::{Parser, Subcommand};

/// Control the durst notification daemon.
///
/// Exit codes: 0 ok, 1 durst is not running, 2 invalid arguments,
/// 3 nothing to act on (or no audio device), 4 invalid config (on reload),
/// 5 the running durst is from another version (restart it).
#[derive(Parser, Debug)]
#[command(name = "durstctl", version, author)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Cmd,
}

#[derive(Subcommand, Debug)]
pub enum Cmd {
    /// Displayed and waiting notifications
    #[command(subcommand)]
    Notif(NotifCmd),
    /// Closed notifications
    #[command(subcommand)]
    History(HistoryCmd),
    /// Modes switch rules on and off, e.g. a do-not-disturb mode
    #[command(subcommand)]
    Mode(ModeCmd),
    /// Speaker and microphone volume (shows the volume OSD)
    #[command(subcommand)]
    Volume(VolumeCmd),
    /// The media player that most recently started playing
    #[command(subcommand)]
    Media(MediaCmd),
    /// On-screen displays
    #[command(subcommand)]
    Osd(OsdCmd),
    /// Reload the config file; on errors the old config stays active
    Reload,
    /// Version, config path and counts
    Info {
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand, Debug)]
pub enum NotifCmd {
    /// List displayed and waiting notifications
    List {
        #[arg(long)]
        json: bool,
    },
    /// Number of displayed notifications
    Count {
        /// count the waiting ones instead
        #[arg(long, conflicts_with = "held")]
        waiting: bool,
        /// count the ones held back by rules instead
        #[arg(long)]
        held: bool,
    },
    /// Close a notification, by default the newest displayed one
    Close { id: Option<u32> },
    /// Close all displayed notifications
    CloseAll,
    /// Invoke an action, by default the default action of the newest one
    Action {
        /// notification id, 0 = newest displayed
        #[arg(default_value_t = 0)]
        id: u32,
        /// action key; default: the "default" action or the only one
        key: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
pub enum ModeCmd {
    /// Print the active modes, one per line
    List {
        /// all modes used by rules, active ones marked with "*"
        #[arg(long)]
        all: bool,
    },
    /// Replace the active modes (none given: deactivate all)
    Set { modes: Vec<String> },
    /// Activate a mode
    Enable { mode: String },
    /// Deactivate a mode
    Disable { mode: String },
    /// Activate a mode if it is inactive, deactivate it otherwise
    Toggle { mode: String },
}

#[derive(Subcommand, Debug)]
pub enum VolumeCmd {
    /// Print the volume in percent, "muted" and the device
    Get {
        /// the microphone instead of the speakers
        #[arg(long)]
        mic: bool,
        #[arg(long)]
        json: bool,
    },
    /// Set the volume: 50, +5, -5 (a trailing % is fine)
    Set {
        #[arg(allow_hyphen_values = true)]
        change: String,
        #[arg(long)]
        mic: bool,
    },
    /// Raise the volume by the configured step
    Up {
        #[arg(long)]
        mic: bool,
    },
    /// Lower the volume by the configured step
    Down {
        #[arg(long)]
        mic: bool,
    },
    /// Mute: on, off or toggle
    Mute {
        #[arg(value_parser = ["on", "off", "toggle"], default_value = "toggle")]
        state: String,
        #[arg(long)]
        mic: bool,
    },
}

#[derive(Subcommand, Debug)]
pub enum MediaCmd {
    /// Player, status, title, artist
    Status {
        #[arg(long)]
        json: bool,
    },
    Play,
    Pause,
    /// Play or pause
    Toggle,
    Next,
    Prev,
}

#[derive(Subcommand, Debug)]
pub enum OsdCmd {
    /// Show an OSD: volume, mic or media
    Show {
        #[arg(value_parser = ["volume", "mic", "media"])]
        kind: String,
    },
}

#[derive(Subcommand, Debug)]
pub enum HistoryCmd {
    /// List closed notifications, newest first
    List {
        #[arg(long)]
        json: bool,
    },
    /// Show the newest closed notification again
    Pop,
    /// Forget all closed notifications
    Clear,
    /// Number of notifications in the history
    Count,
}
