mod cli;

use std::process::ExitCode;

use clap::Parser;
use durst_proto::control::{DurstProxy, NotificationInfo, error};

use cli::{Cli, Cmd, HistoryCmd, NotifCmd};

/// Exit codes, see the help text in cli.rs.
const NOT_RUNNING: u8 = 1;
const NOT_FOUND: u8 = 3;
const INVALID_CONFIG: u8 = 4;

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli.command).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            let (code, message) = describe(&e);
            eprintln!("durstctl: {message}");
            ExitCode::from(code)
        }
    }
}

fn describe(e: &zbus::Error) -> (u8, String) {
    match e {
        zbus::Error::MethodError(name, msg, _) => {
            let msg = msg.clone().unwrap_or_default();
            match name.as_str() {
                error::NOT_FOUND => (NOT_FOUND, msg),
                error::INVALID_CONFIG => (INVALID_CONFIG, format!("invalid config: {msg}")),
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
        Cmd::Notif(NotifCmd::Count { waiting: false }) => {
            println!("{}", durst.displayed_count().await?)
        }
        Cmd::Notif(NotifCmd::Count { waiting: true }) => {
            println!("{}", durst.waiting_count().await?)
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
                println!("history:   {}", info.history);
            }
        }
    }
    Ok(())
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
