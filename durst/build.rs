use clap_complete::{generate_to, Shell};
use dbus_codegen::{GenOpts, ServerAccess};
use std::fs::{self, File};
use std::io::Write;

include!("src/cli.rs");

fn main() {
    dbus_interface();
    cli();
}

fn dbus_interface() {
    let xml = fs::read_to_string("interface.xml").unwrap();
    let mut dbus_opts = GenOpts::default();
    dbus_opts.serveraccess = ServerAccess::AsRefClosure;
    let interface = dbus_codegen::generate(&xml, &dbus_opts).unwrap();
    let mut out = File::create("src/dbus_interface.rs").unwrap();
    out.write_all(&interface.into_bytes()).unwrap();
    println!("cargo:rerun-if-changed=interface.xml");
}

fn cli() {
    let outdir = concat!(env!("CARGO_MANIFEST_DIR"), "/scripts/completion");
    fs::create_dir_all(&outdir).expect("Failed to create output directory");

    let mut app = build_cli();

    for shell in [Shell::Fish, Shell::Zsh, Shell::Bash, Shell::Elvish] {
        generate_to(shell, &mut app, env!("CARGO_PKG_NAME"), &outdir)
            .expect("Failed to generate shell completions");
    }
}
