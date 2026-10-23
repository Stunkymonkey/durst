//! A fingerprint of the interface definitions: durst publishes it, durstctl
//! compares it with its own, so a daemon still running from an older build
//! is recognized instead of failing with a D-Bus signature error.

use std::{env, fs, path::Path};

const SOURCES: [&str; 2] = ["src/control.rs", "src/notifications.rs"];

fn main() {
    // FNV-1a: stable across Rust versions, unlike std's hasher
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for source in SOURCES {
        println!("cargo:rerun-if-changed={source}");
        for byte in fs::read(source).expect("read interface definition") {
            hash = (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3);
        }
    }
    let out = Path::new(&env::var("OUT_DIR").unwrap()).join("interface_hash.rs");
    fs::write(out, format!("\"{hash:016x}\"")).expect("write interface hash");
}
