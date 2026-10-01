//! Server side of `org.freedesktop.Notifications`.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};

use durst_proto::notifications::{BUS_NAME, CloseReason, INTERFACE, OBJECT_PATH, SPEC_VERSION};
use futures::SinkExt;
use futures::channel::mpsc::Sender;
use zbus::fdo::{self, RequestNameFlags, RequestNameReply};
use zbus::zvariant::{OwnedValue, Value};
use zbus::{Connection, interface};

use crate::core::notification::{Hints, Notification, Urgency, pair_actions};

const CAPABILITIES: &[&str] = &["body", "icon-static"];

#[derive(Debug, Clone)]
pub enum Event {
    /// the bus name is ours; keep the connection to emit signals
    Connected(Connection),
    Failed(String),
    Notify(Box<Notification>),
    Close(u32),
}

struct Server {
    events: Sender<Event>,
    next_id: AtomicU32,
}

impl Server {
    async fn send(&self, event: Event) -> fdo::Result<()> {
        self.events
            .clone()
            .send(event)
            .await
            .map_err(|e| fdo::Error::Failed(e.to_string()))
    }
}

#[interface(name = "org.freedesktop.Notifications")]
impl Server {
    fn get_capabilities(&self) -> Vec<&str> {
        CAPABILITIES.to_vec()
    }

    #[allow(clippy::too_many_arguments)]
    async fn notify(
        &self,
        app_name: String,
        replaces_id: u32,
        app_icon: String,
        summary: String,
        body: String,
        actions: Vec<String>,
        hints: HashMap<String, OwnedValue>,
        expire_timeout: i32,
    ) -> fdo::Result<u32> {
        let id = match replaces_id {
            0 => self.next_id.fetch_add(1, Ordering::Relaxed),
            id => id,
        };
        let notification = Notification {
            id,
            app_name,
            app_icon,
            summary,
            body,
            actions: pair_actions(&actions),
            hints: parse_hints(&hints),
            expire_timeout,
        };
        log::debug!("notify {notification:?}");
        self.send(Event::Notify(Box::new(notification))).await?;
        Ok(id)
    }

    async fn close_notification(&self, id: u32) -> fdo::Result<()> {
        log::debug!("close_notification {id}");
        self.send(Event::Close(id)).await
    }

    fn get_server_information(&self) -> (&str, &str, &str, &str) {
        (
            env!("CARGO_PKG_NAME"),
            "durst-notification.org",
            env!("CARGO_PKG_VERSION"),
            SPEC_VERSION,
        )
    }
}

/// Connects to the session bus, serves the interface and claims the
/// well-known name. Fails if another notification daemon owns it.
pub async fn serve(events: Sender<Event>) -> Result<Connection, String> {
    let server = Server {
        events,
        next_id: AtomicU32::new(1),
    };
    let conn = zbus::connection::Builder::session()
        .and_then(|b| b.serve_at(OBJECT_PATH, server))
        .map_err(|e| e.to_string())?
        .build()
        .await
        .map_err(|e| format!("cannot connect to the session bus: {e}"))?;
    let reply = conn
        .request_name_with_flags(BUS_NAME, RequestNameFlags::DoNotQueue.into())
        .await;
    match reply {
        Ok(RequestNameReply::PrimaryOwner | RequestNameReply::AlreadyOwner) => Ok(conn),
        Ok(_) | Err(zbus::Error::NameTaken) => Err(format!(
            "{BUS_NAME} is owned by another notification daemon"
        )),
        Err(e) => Err(e.to_string()),
    }
}

pub async fn emit_closed(conn: Connection, id: u32, reason: CloseReason) {
    let body = (id, reason as u32);
    if let Err(e) = conn
        .emit_signal(
            None::<&str>,
            OBJECT_PATH,
            INTERFACE,
            "NotificationClosed",
            &body,
        )
        .await
    {
        log::warn!("cannot emit NotificationClosed: {e}");
    }
}

fn parse_hints(hints: &HashMap<String, OwnedValue>) -> Hints {
    let get = |key: &str| hints.get(key).map(|v| unwrap_variant(v));
    let string = |key: &str| match get(key) {
        Some(Value::Str(s)) => Some(s.to_string()),
        _ => None,
    };
    let int = |key: &str| match get(key)? {
        Value::U8(i) => Some(*i as i64),
        Value::I16(i) => Some(*i as i64),
        Value::U16(i) => Some(*i as i64),
        Value::I32(i) => Some(*i as i64),
        Value::U32(i) => Some(*i as i64),
        Value::I64(i) => Some(*i),
        Value::U64(i) => i64::try_from(*i).ok(),
        _ => None,
    };
    let boolean = |key: &str| match get(key) {
        Some(Value::Bool(b)) => *b,
        // some clients send integers for booleans
        _ => int(key).is_some_and(|i| i != 0),
    };
    Hints {
        urgency: int("urgency")
            .map(|u| Urgency::from_byte(u.clamp(0, 255) as u8))
            .unwrap_or_default(),
        category: string("category"),
        desktop_entry: string("desktop-entry"),
        image_path: string("image-path").or_else(|| string("image_path")),
        transient: boolean("transient"),
        resident: boolean("resident"),
        value: int("value").map(|v| v.clamp(0, 100) as i32),
        stack_tag: string("x-dunst-stack-tag")
            .or_else(|| string("x-canonical-private-synchronous")),
    }
}

/// Some clients wrap hint values in an extra variant.
fn unwrap_variant<'a>(v: &'a Value<'a>) -> &'a Value<'a> {
    match v {
        Value::Value(inner) => unwrap_variant(inner),
        v => v,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hints(pairs: &[(&str, Value<'static>)]) -> Hints {
        let map = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.try_to_owned().unwrap()))
            .collect();
        parse_hints(&map)
    }

    #[test]
    fn parses_known_hints() {
        let h = hints(&[
            ("urgency", Value::U8(2)),
            ("category", Value::from("im.received")),
            ("value", Value::I32(150)),
            ("transient", Value::Bool(true)),
            ("x-canonical-private-synchronous", Value::from("volume")),
        ]);
        assert_eq!(h.urgency, Urgency::Critical);
        assert_eq!(h.category.as_deref(), Some("im.received"));
        assert_eq!(h.value, Some(100));
        assert!(h.transient);
        assert_eq!(h.stack_tag.as_deref(), Some("volume"));
    }

    #[test]
    fn tolerates_odd_types() {
        let h = hints(&[
            ("urgency", Value::I32(0)),
            ("resident", Value::U32(1)),
            ("value", Value::Value(Box::new(Value::U32(40)))),
        ]);
        assert_eq!(h.urgency, Urgency::Low);
        assert!(h.resident);
        assert_eq!(h.value, Some(40));
    }
}
