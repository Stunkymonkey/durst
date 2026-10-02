use clap::{Parser, Subcommand};

/// Control the durst notification daemon.
///
/// Exit codes: 0 ok, 1 durst is not running, 2 invalid arguments,
/// 3 nothing to act on, 4 invalid config (on reload).
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
