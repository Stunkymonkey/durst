use clap_complete::{generate_to, Shell};
use dbus_codegen::{generate, GenOpts, ServerAccess};
use std::{fs, process::exit};

include!("src/cli.rs");

fn main() {
    dbus_interface();
    cli();
}

fn dbus_interface() {
    let input_path = "org.freedesktop.Notifications.xml";
    let output_path = "src/dbus_notifications.rs";

    // Read the XML file
    let xml = match fs::read_to_string(input_path) {
        Ok(content) => content,
        Err(e) => {
            eprintln!("Error reading file '{}': {}", input_path, e);
            exit(1);
        }
    };

    // Set up dbus code generation options
    let mut dbus_opts = GenOpts::default();
    dbus_opts.serveraccess = ServerAccess::AsRefClosure;

    // Generate the interface
    let interface = match generate(&xml, &dbus_opts) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("Error generating DBus interface: {}", e);
            exit(1);
        }
    };

    // Write the output to the destination file
    if let Err(e) = fs::write(output_path, interface) {
        eprintln!("Error writing to file '{}': {}", output_path, e);
        exit(1);
    }

    // Inform Cargo to watch the input file for changes
    println!("cargo:rerun-if-changed={}", input_path);
}

fn cli() {
    let outdir = concat!(env!("CARGO_MANIFEST_DIR"), "/scripts/completion");

    if let Err(e) = fs::create_dir_all(&outdir) {
        eprintln!("Error creating output directory '{}': {}", outdir, e);
        exit(1);
    }

    let mut app = build_cli();

    for shell in [Shell::Fish, Shell::Zsh, Shell::Bash, Shell::Elvish] {
        if let Err(e) = generate_to(shell, &mut app, env!("CARGO_PKG_NAME"), &outdir) {
            eprintln!("Error generating completions for {:?}: {}", shell, e);
            exit(1);
        }
    }
}
