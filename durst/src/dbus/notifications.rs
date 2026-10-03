//! Server side of `org.freedesktop.Notifications`.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};

use durst_proto::notifications::{CloseReason, INTERFACE, OBJECT_PATH, SPEC_VERSION};
use futures::channel::mpsc::{Sender, UnboundedReceiver};
use futures::future::BoxFuture;
use futures::{SinkExt, StreamExt};
use zbus::fdo;
use zbus::zvariant::{OwnedValue, Value};
use zbus::{Connection, interface};

use super::Event;

use crate::core::notification::{
    ColorHints, Hints, ImageData, Notification, Urgency, pair_actions,
};

const CAPABILITIES: &[&str] = &[
    "actions",
    "body",
    "body-hyperlinks",
    "body-markup",
    "icon-static",
    "sound",
    "x-dunst-stack-tag",
];

pub struct Server {
    events: Sender<Event>,
    next_id: AtomicU32,
}

impl Server {
    pub fn new(events: Sender<Event>) -> Self {
        Self {
            events,
            next_id: AtomicU32::new(1),
        }
    }

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
            received: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs()),
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

/// A signal of `org.freedesktop.Notifications`, see [`emit_in_order`].
pub enum Signal {
    /// preceded by `ActivationToken` if `token` yields one
    ActionInvoked {
        id: u32,
        key: String,
        token: Option<BoxFuture<'static, Option<String>>>,
    },
    Closed(u32, CloseReason),
}

/// Emits the queued signals strictly in order: an `ActionInvoked` waits for
/// its activation token, and the `NotificationClosed` queued after it must
/// not overtake it (clients drop an action's callback once the notification
/// is closed).
pub async fn emit_in_order(conn: Connection, mut signals: UnboundedReceiver<Signal>) {
    while let Some(signal) = signals.next().await {
        match signal {
            Signal::ActionInvoked { id, key, token } => {
                if let Some(token) = token {
                    match token.await {
                        Some(token) => emit(&conn, "ActivationToken", &(id, token)).await,
                        None => log::debug!("no activation token for {id}"),
                    }
                }
                emit(&conn, "ActionInvoked", &(id, key)).await;
            }
            Signal::Closed(id, reason) => {
                emit(&conn, "NotificationClosed", &(id, reason as u32)).await;
            }
        }
    }
}

async fn emit<B>(conn: &Connection, name: &str, body: &B)
where
    B: serde::Serialize + zbus::zvariant::DynamicType,
{
    if let Err(e) = conn
        .emit_signal(None::<&str>, OBJECT_PATH, INTERFACE, name, body)
        .await
    {
        log::warn!("cannot emit {name}: {e}");
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
        image_data: ["image-data", "image_data", "icon_data"]
            .into_iter()
            .find_map(|key| get(key).and_then(image_data)),
        transient: boolean("transient"),
        resident: boolean("resident"),
        value: int("value").map(|v| v.clamp(0, 100) as i32),
        stack_tag: string("x-dunst-stack-tag")
            .or_else(|| string("x-canonical-private-synchronous")),
        sound_file: string("sound-file"),
        sound_name: string("sound-name"),
        suppress_sound: boolean("suppress-sound"),
        colors: ColorHints {
            foreground: string("fgcolor"),
            background: string("bgcolor"),
            frame: string("frcolor"),
            highlight: string("hlcolor"),
        },
    }
}

/// `(iiibiiay)`: width, height, rowstride, has_alpha, bits per sample,
/// channels, data
fn image_data(v: &Value) -> Option<ImageData> {
    let Value::Structure(s) = v else {
        return None;
    };
    let f = s.fields();
    let int = |i: usize| match f.get(i)? {
        Value::I32(n) => Some(*n),
        _ => None,
    };
    let (Some(Value::Bool(alpha)), Some(Value::Array(data))) = (f.get(3), f.get(6)) else {
        return None;
    };
    let bytes: Vec<u8> = data
        .iter()
        .map(|b| match b {
            Value::U8(b) => Some(*b),
            _ => None,
        })
        .collect::<Option<_>>()?;
    let img = ImageData::from_spec(int(0)?, int(1)?, int(2)?, *alpha, int(4)?, int(5)?, &bytes);
    if img.is_none() {
        log::warn!("ignoring malformed image-data hint");
    }
    img
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
    fn parses_image_data() {
        let data: Vec<u8> = vec![255; 2 * 2 * 4];
        let value = Value::from(zbus::zvariant::Structure::from((
            2i32, 2i32, 8i32, true, 8i32, 4i32, data,
        )));
        let h = hints(&[("image-data", value)]);
        let img = h.image_data.unwrap();
        assert_eq!((img.width, img.height, img.rgba.len()), (2, 2, 16));
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
