use clap_complete::{Shell, generate_to};
use std::{fs, process::exit};

include!("src/cli.rs");

fn main() {
    println!("cargo:rerun-if-changed=src/cli.rs");

    let outdir = concat!(env!("CARGO_MANIFEST_DIR"), "/scripts/completion");
    if let Err(e) = fs::create_dir_all(outdir) {
        eprintln!("Error creating output directory '{}': {}", outdir, e);
        exit(1);
    }

    let mut app = build_cli();
    for shell in [Shell::Fish, Shell::Zsh, Shell::Bash, Shell::Elvish] {
        if let Err(e) = generate_to(shell, &mut app, env!("CARGO_PKG_NAME"), outdir) {
            eprintln!("Error generating completions for {:?}: {}", shell, e);
            exit(1);
        }
    }
}
