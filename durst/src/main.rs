mod app;
mod audio;
mod cli;
mod config;
mod config_watch;
mod core;
mod dbus;
mod effects;
mod logind;
mod ui;
mod wayland;

use std::path::PathBuf;

fn main() {
    let matches = cli::build_cli().get_matches();

    let level = match matches.get_count("verbose") {
        0 => "info",
        1 => "debug",
        _ => "trace",
    };
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or(format!("warn,durst={level}")),
    )
    .init();

    let (path, explicit) = match matches.get_one::<String>("config") {
        Some(path) => (PathBuf::from(path), true),
        None => (config::default_path(), false),
    };
    // a broken config must not leave the user without notifications: start
    // with the defaults and show the error as a notification
    let (config, text, error) = match config::load(&path, explicit) {
        Ok((config, text)) => (config, text, None),
        Err(e) => {
            log::error!("{e}");
            (config::Config::default(), None, Some(e))
        }
    };
    log::debug!("{config:?}");

    let source = app::ConfigSource {
        path,
        explicit,
        text,
        error,
    };
    if let Err(e) = app::run(config, source) {
        log::error!("{e}");
        std::process::exit(1);
    }
}
