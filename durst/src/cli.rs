use clap::{Arg, Command};
use clap::builder::PossibleValuesParser;

pub fn build_cli() -> Command {
    Command::new(env!("CARGO_PKG_NAME"))
        .about(env!("CARGO_PKG_DESCRIPTION"))
        .author(env!("CARGO_PKG_AUTHORS"))
        .version(env!("CARGO_PKG_VERSION"))
        .arg(
            Arg::new("verbose")
                .help("turn on debugging information")
                .long("verbose")
                .short('v'),
        )
        .arg(
            Arg::new("config-path")
                .value_name("FILE")
                .help("Use alternative config file")
                .long("config")
                .short('c'),
                // .takes_value(true),
        )
        .arg(
            Arg::new("mode")
                .help("Overwrite the automatic output-mode")
                .long("force-output")
                .short('o')
                .value_name("MODE")
                .value_parser(PossibleValuesParser::new(["wayland", "xorg", "stdout"]))
        )
}
