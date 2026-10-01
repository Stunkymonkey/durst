use clap::CommandFactory;
use clap_complete::{Shell, generate_to};
use std::{fs, process::exit};

include!("src/cli.rs");

fn main() {
    println!("cargo:rerun-if-changed=src/cli.rs");

    let scripts = concat!(env!("CARGO_MANIFEST_DIR"), "/scripts");
    let completions = format!("{scripts}/completion");
    let man = format!("{scripts}/man");
    for dir in [&completions, &man] {
        if let Err(e) = fs::create_dir_all(dir) {
            eprintln!("Error creating output directory '{}': {}", dir, e);
            exit(1);
        }
    }

    let mut cmd = Cli::command();
    for shell in [Shell::Fish, Shell::Zsh, Shell::Bash, Shell::Elvish] {
        if let Err(e) = generate_to(shell, &mut cmd, "durstctl", &completions) {
            eprintln!("Error generating completions for {:?}: {}", shell, e);
            exit(1);
        }
    }
    if let Err(e) = clap_mangen::generate_to(cmd, &man) {
        eprintln!("Error generating man pages: {}", e);
        exit(1);
    }
}
