mod cli;

use std::process::ExitCode;

use clap::Parser;
use durst_proto::INTERFACE_HASH;
use durst_proto::control::{
    BUS_NAME, DurstProxy, INTERFACE, INTERFACE_HASH_PROPERTY, MediaInfo, NotificationInfo,
    OBJECT_PATH, VolumeInfo, error,
};

use cli::{Cli, Cmd, HistoryCmd, MediaCmd, ModeCmd, NotifCmd, OsdCmd, VolumeCmd};

/// Exit codes, see the help text in cli.rs.
const NOT_RUNNING: u8 = 1;
const INVALID_ARGUMENT: u8 = 2;
const NOT_FOUND: u8 = 3;
const INVALID_CONFIG: u8 = 4;
const MISMATCH: u8 = 5;

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli.command).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            let (code, message) = match mismatch().await {
                Some(message) => (MISMATCH, message),
                None => describe(&e),
            };
            eprintln!("durstctl: {message}");
            ExitCode::from(code)
        }
    }
}

/// After an error: is the running durst built from other interface
/// definitions than durstctl? Then that is the actual problem, typically a
/// daemon still running from before an update.
async fn mismatch() -> Option<String> {
    let conn = zbus::Connection::session().await.ok()?;
    let properties = zbus::fdo::PropertiesProxy::builder(&conn)
        .destination(BUS_NAME)
        .ok()?
        .path(OBJECT_PATH)
        .ok()?
        .build()
        .await
        .ok()?;
    let interface = zbus::names::InterfaceName::try_from(INTERFACE).ok()?;
    let theirs = match properties.get(interface, INTERFACE_HASH_PROPERTY).await {
        Ok(value) => String::try_from(value).ok(),
        // not running: no mismatch, the normal error says so
        Err(zbus::fdo::Error::ServiceUnknown(_) | zbus::fdo::Error::NameHasNoOwner(_)) => {
            return None;
        }
        // durst from before the fingerprint existed
        Err(_) => None,
    };
    if theirs.as_deref() == Some(INTERFACE_HASH) {
        return None;
    }
    Some(match theirs {
        Some(theirs) => format!(
            "the running durst speaks another interface version ({theirs}, durstctl: \
             {INTERFACE_HASH}); restart durst, e.g. `systemctl --user restart durst`"
        ),
        None => "the running durst is older than durstctl; restart durst, e.g. \
                 `systemctl --user restart durst`"
            .into(),
    })
}

fn describe(e: &zbus::Error) -> (u8, String) {
    match e {
        zbus::Error::MethodError(name, msg, _) => {
            let msg = msg.clone().unwrap_or_default();
            match name.as_str() {
                error::NOT_FOUND => (NOT_FOUND, msg),
                error::INVALID_CONFIG => (INVALID_CONFIG, format!("invalid config: {msg}")),
                error::INVALID_ARGUMENT => (INVALID_ARGUMENT, msg),
                "org.freedesktop.DBus.Error.ServiceUnknown"
                | "org.freedesktop.DBus.Error.NameHasNoOwner" => {
                    (NOT_RUNNING, "durst is not running".into())
                }
                _ => (NOT_RUNNING, format!("{name}: {msg}")),
            }
        }
        e => (NOT_RUNNING, e.to_string()),
    }
}

async fn run(command: Cmd) -> zbus::Result<()> {
    let conn = zbus::Connection::session().await?;
    let durst = DurstProxy::new(&conn).await?;
    match command {
        Cmd::Notif(NotifCmd::List { json }) => print_list(&durst.list().await?, json),
        Cmd::Notif(NotifCmd::Count { waiting, held }) => {
            let count = match (waiting, held) {
                (true, _) => durst.waiting_count().await?,
                (_, true) => durst.held_count().await?,
                _ => durst.displayed_count().await?,
            };
            println!("{count}")
        }
        Cmd::Notif(NotifCmd::Close { id }) => durst.close(id.unwrap_or(0)).await?,
        Cmd::Notif(NotifCmd::CloseAll) => durst.close_all().await?,
        Cmd::Notif(NotifCmd::Action { id, key }) => {
            durst.action(id, key.as_deref().unwrap_or("")).await?
        }
        Cmd::History(HistoryCmd::List { json }) => print_list(&durst.history().await?, json),
        Cmd::History(HistoryCmd::Pop) => println!("{}", durst.history_pop().await?),
        Cmd::History(HistoryCmd::Clear) => durst.history_clear().await?,
        Cmd::History(HistoryCmd::Count) => println!("{}", durst.history_count().await?),
        Cmd::Mode(ModeCmd::List { all: false }) => print_modes(&durst.active_modes().await?),
        Cmd::Mode(ModeCmd::List { all: true }) => {
            let active = durst.active_modes().await?;
            let mut known = durst.known_modes().await?;
            known.extend(
                active
                    .iter()
                    .filter(|m| !known.contains(m))
                    .cloned()
                    .collect::<Vec<_>>(),
            );
            known.sort();
            for mode in known {
                let mark = if active.contains(&mode) { "*" } else { " " };
                println!("{mark} {mode}");
            }
        }
        Cmd::Mode(ModeCmd::Set { modes }) => {
            let modes: Vec<&str> = modes.iter().map(String::as_str).collect();
            print_modes(&durst.set_modes(&modes).await?)
        }
        Cmd::Mode(ModeCmd::Enable { mode }) => print_modes(&durst.enable_mode(&mode).await?),
        Cmd::Mode(ModeCmd::Disable { mode }) => print_modes(&durst.disable_mode(&mode).await?),
        Cmd::Mode(ModeCmd::Toggle { mode }) => print_modes(&durst.toggle_mode(&mode).await?),
        Cmd::Volume(VolumeCmd::Get { mic, json }) => print_volume(&durst.volume(mic).await?, json),
        Cmd::Volume(VolumeCmd::Set { change, mic }) => {
            print_volume(&durst.set_volume(mic, &change).await?, false)
        }
        Cmd::Volume(VolumeCmd::Up { mic }) => {
            print_volume(&durst.set_volume(mic, "up").await?, false)
        }
        Cmd::Volume(VolumeCmd::Down { mic }) => {
            print_volume(&durst.set_volume(mic, "down").await?, false)
        }
        Cmd::Volume(VolumeCmd::Mute { state, mic }) => {
            print_volume(&durst.set_mute(mic, &state).await?, false)
        }
        Cmd::Media(MediaCmd::Status { json }) => print_media(&durst.media_status().await?, json),
        Cmd::Media(action) => {
            let action = match action {
                MediaCmd::Play => "play",
                MediaCmd::Pause => "pause",
                MediaCmd::Toggle => "toggle",
                MediaCmd::Next => "next",
                MediaCmd::Prev => "prev",
                MediaCmd::Status { .. } => unreachable!(),
            };
            durst.media_action(action).await?;
        }
        Cmd::Osd(OsdCmd::Show { kind }) if kind == "media" => durst.show_media_osd().await?,
        Cmd::Osd(OsdCmd::Show { kind }) => durst.show_volume_osd(kind == "mic").await?,
        Cmd::Reload => durst.reload().await?,
        Cmd::Info { json } => {
            let info = durst.info().await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&info).unwrap());
            } else {
                println!("version:   {}", info.version);
                println!("config:    {}", info.config_path);
                println!("displayed: {}", info.displayed);
                println!("waiting:   {}", info.waiting);
                println!("held:      {}", info.held);
                println!("history:   {}", info.history);
                println!("modes:     {}", info.modes.join(" "));
                println!("idle:      {}", info.idle);
                println!("locked:    {}", info.locked);
                println!("fullscreen: {}", info.fullscreen);
                println!("outputs:   {}", info.outputs.join(" "));
                println!("focused output: {}", info.focused_output);
            }
        }
    }
    Ok(())
}

fn print_volume(v: &VolumeInfo, json: bool) {
    if json {
        println!("{}", serde_json::to_string_pretty(v).unwrap());
    } else if v.muted {
        println!("{} muted  {}", v.percent, v.description);
    } else {
        println!("{}  {}", v.percent, v.description);
    }
}

fn print_media(m: &MediaInfo, json: bool) {
    if json {
        println!("{}", serde_json::to_string_pretty(m).unwrap());
    } else {
        println!("{} ({})", m.player, m.status);
        if !m.title.is_empty() {
            println!("{}", m.title);
        }
        if !m.artist.is_empty() {
            println!("{}", m.artist);
        }
    }
}

fn print_modes(modes: &[String]) {
    for mode in modes {
        println!("{mode}");
    }
}

fn print_list(list: &[NotificationInfo], json: bool) {
    if json {
        println!("{}", serde_json::to_string_pretty(list).unwrap());
        return;
    }
    for n in list {
        let summary = n.summary.replace('\n', " ");
        println!(
            "{:>5}  {:<8}  {:<9}  {}: {}",
            n.id, n.urgency, n.state, n.app_name, summary
        );
    }
}
