//! The session bus: `org.freedesktop.Notifications` and durst's own control
//! interface, served on one connection. Both forward everything to the UI
//! loop as [`Event`]s.

pub mod control;
pub mod notifications;

use std::sync::{Arc, Mutex};

use futures::channel::mpsc::Sender;
use zbus::Connection;
use zbus::fdo::{RequestNameFlags, RequestNameReply};

use crate::core::notification::Notification;
use control::{Command, Responder, Status};

#[derive(Debug, Clone)]
pub enum Event {
    /// both bus names are ours; keep the connection to emit signals, and the
    /// status to publish counts through the control interface
    Connected(Connection, Arc<Mutex<Status>>),
    Failed(String),
    Notify(Box<Notification>),
    Close(u32),
    Control(Command, Responder),
}

/// Connects to the session bus, serves both interfaces and claims both
/// well-known names. Fails if another notification daemon is running.
pub async fn serve(events: Sender<Event>) -> Result<(Connection, Arc<Mutex<Status>>), String> {
    use durst_proto::{control, notifications};

    let status = Arc::new(Mutex::new(Status::default()));
    let conn = zbus::connection::Builder::session()
        .and_then(|b| {
            b.serve_at(
                notifications::OBJECT_PATH,
                self::notifications::Server::new(events.clone()),
            )
        })
        .and_then(|b| {
            b.serve_at(
                control::OBJECT_PATH,
                self::control::Control::new(events, status.clone()),
            )
        })
        .map_err(|e| e.to_string())?
        .build()
        .await
        .map_err(|e| format!("cannot connect to the session bus: {e}"))?;
    for name in [notifications::BUS_NAME, control::BUS_NAME] {
        let reply = conn
            .request_name_with_flags(name, RequestNameFlags::DoNotQueue.into())
            .await;
        match reply {
            Ok(RequestNameReply::PrimaryOwner | RequestNameReply::AlreadyOwner) => {}
            Ok(_) | Err(zbus::Error::NameTaken) => {
                return Err(format!("{name} is owned by another notification daemon"));
            }
            Err(e) => return Err(e.to_string()),
        }
    }
    Ok((conn, status))
}
