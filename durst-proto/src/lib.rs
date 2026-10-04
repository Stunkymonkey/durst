//! D-Bus interface definitions shared by the durst daemon and durstctl.

pub mod control;
pub mod notifications;

/// Fingerprint of the interface definitions (see build.rs); the same in
/// durst and durstctl exactly when they speak the same interface.
pub const INTERFACE_HASH: &str = include!(concat!(env!("OUT_DIR"), "/interface_hash.rs"));
