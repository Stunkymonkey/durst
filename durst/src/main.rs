mod app;
mod cli;
mod config;
mod core;
mod dbus;
mod ui;

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
    let config = match config::load(&path, explicit) {
        Ok(config) => config,
        Err(e) => {
            log::error!("{e}");
            std::process::exit(1);
        }
    };
    log::debug!("{config:?}");

    if let Err(e) = app::run(config) {
        log::error!("{e}");
        std::process::exit(1);
    }
}
