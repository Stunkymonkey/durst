use clap::{Arg, ArgAction, Command};

pub fn build_cli() -> Command {
    Command::new(env!("CARGO_PKG_NAME"))
        .about(env!("CARGO_PKG_DESCRIPTION"))
        .author(env!("CARGO_PKG_AUTHORS"))
        .version(env!("CARGO_PKG_VERSION"))
        .arg(
            Arg::new("verbose")
                .help("more log output, repeat for more (RUST_LOG overrides)")
                .long("verbose")
                .short('v')
                .action(ArgAction::Count),
        )
        .arg(
            Arg::new("config")
                .value_name("FILE")
                .help("use an alternative config file")
                .long("config")
                .short('c'),
        )
}
