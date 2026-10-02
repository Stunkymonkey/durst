//! Media players over MPRIS: which ones exist, what they play, and remote
//! control. Runs as a task on its own session bus connection.

use std::collections::HashMap;

use futures::channel::mpsc::{UnboundedSender, unbounded};
use futures::stream::{AbortHandle, Abortable, BoxStream, SelectAll};
use futures::{SinkExt, Stream, StreamExt};
use iced::Subscription;
use zbus::Connection;
use zbus::fdo::DBusProxy;
use zbus::zvariant::{OwnedValue, Value};

use crate::core::media::{Status, Track};

const PREFIX: &str = "org.mpris.MediaPlayer2.";

#[zbus::proxy(
    interface = "org.mpris.MediaPlayer2",
    default_path = "/org/mpris/MediaPlayer2"
)]
trait MediaPlayer2 {
    #[zbus(property)]
    fn identity(&self) -> zbus::Result<String>;
}

#[zbus::proxy(
    interface = "org.mpris.MediaPlayer2.Player",
    default_path = "/org/mpris/MediaPlayer2"
)]
trait Player {
    fn play_pause(&self) -> zbus::Result<()>;
    fn play(&self) -> zbus::Result<()>;
    fn pause(&self) -> zbus::Result<()>;
    fn next(&self) -> zbus::Result<()>;
    fn previous(&self) -> zbus::Result<()>;

    #[zbus(property)]
    fn playback_status(&self) -> zbus::Result<String>;

    #[zbus(property)]
    fn metadata(&self) -> zbus::Result<HashMap<String, OwnedValue>>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    PlayPause,
    Play,
    Pause,
    Next,
    Previous,
}

#[derive(Debug, Clone)]
pub enum Event {
    Ready(Handle),
    /// what a player reported; `initial` for players found at startup
    Player {
        name: String,
        identity: Option<String>,
        status: Option<Status>,
        track: Option<Track>,
        initial: bool,
    },
    Gone(String),
}

/// Sends actions to a player (by bus name).
#[derive(Clone)]
pub struct Handle(UnboundedSender<(String, Action)>);

impl Handle {
    pub fn send(&self, player: &str, action: Action) {
        let _ = self.0.unbounded_send((player.to_owned(), action));
    }
}

impl std::fmt::Debug for Handle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("mpris::Handle")
    }
}

pub fn subscription() -> Subscription<Event> {
    Subscription::run(stream)
}

fn stream() -> impl Stream<Item = Event> {
    iced::stream::channel(32, async |mut output| {
        if let Err(e) = watch(&mut output).await {
            log::warn!("media players unavailable: {e}");
        }
        std::future::pending::<()>().await;
    })
}

/// The `Metadata` property: `xesam:artist` is a list by the spec, but some
/// players send a string.
pub fn parse_metadata(metadata: &HashMap<String, OwnedValue>) -> Track {
    let get = |key: &str| metadata.get(key).map(|v| unwrap_variant(v));
    let string = |key: &str| match get(key) {
        Some(Value::Str(s)) => s.to_string(),
        Some(Value::ObjectPath(p)) => p.to_string(),
        Some(Value::Array(a)) => a
            .iter()
            .filter_map(|v| match v {
                Value::Str(s) => Some(s.to_string()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join(", "),
        _ => String::new(),
    };
    let art = string("mpris:artUrl");
    Track {
        id: string("mpris:trackid"),
        title: string("xesam:title"),
        artist: string("xesam:artist"),
        album: string("xesam:album"),
        art_url: (!art.is_empty()).then_some(art),
    }
}

fn unwrap_variant<'a>(v: &'a Value<'a>) -> &'a Value<'a> {
    match v {
        Value::Value(inner) => unwrap_variant(inner),
        v => v,
    }
}

type Updates = SelectAll<BoxStream<'static, Event>>;

async fn watch(output: &mut futures::channel::mpsc::Sender<Event>) -> zbus::Result<()> {
    let conn = Connection::session().await?;
    let dbus = DBusProxy::new(&conn).await?;
    let (actions, mut action_rx) = unbounded();
    let _ = output.send(Event::Ready(Handle(actions))).await;

    let mut owners = dbus.receive_name_owner_changed().await?;
    let mut updates: Updates = SelectAll::new();
    // the property streams of each player, stopped when it goes away
    let mut streams: HashMap<String, AbortHandle> = HashMap::new();
    for name in dbus.list_names().await? {
        if name.starts_with(PREFIX) {
            add_player(
                &conn,
                name.to_string(),
                true,
                output,
                &mut updates,
                &mut streams,
            )
            .await;
        }
    }

    loop {
        tokio::select! {
            Some(change) = owners.next() => {
                let Ok(args) = change.args() else { continue };
                let name = args.name().to_string();
                if !name.starts_with(PREFIX) {
                    continue;
                }
                if let Some(handle) = streams.remove(&name) {
                    handle.abort();
                }
                if args.new_owner().is_some() {
                    add_player(&conn, name, false, output, &mut updates, &mut streams).await;
                } else {
                    log::debug!("media player gone: {name}");
                    let _ = output.send(Event::Gone(name)).await;
                }
            }
            Some(event) = updates.next() => {
                let _ = output.send(event).await;
            }
            Some((name, action)) = action_rx.next() => {
                if let Err(e) = call(&conn, &name, action).await {
                    log::warn!("{action:?} on {name}: {e}");
                }
            }
        }
    }
}

async fn add_player(
    conn: &Connection,
    name: String,
    initial: bool,
    output: &mut futures::channel::mpsc::Sender<Event>,
    updates: &mut Updates,
    streams: &mut HashMap<String, AbortHandle>,
) {
    let result: zbus::Result<()> = async {
        let player = PlayerProxy::builder(conn)
            .destination(name.clone())?
            .build()
            .await?;
        let root = MediaPlayer2Proxy::builder(conn)
            .destination(name.clone())?
            .build()
            .await?;
        log::debug!("media player: {name}");
        let _ = output
            .send(Event::Player {
                name: name.clone(),
                identity: root.identity().await.ok(),
                status: player
                    .playback_status()
                    .await
                    .ok()
                    .map(|s| Status::parse(&s)),
                track: player.metadata().await.ok().map(|m| parse_metadata(&m)),
                initial,
            })
            .await;

        let status_name = name.clone();
        let status = player
            .receive_playback_status_changed()
            .await
            .filter_map(move |c| {
                let name = status_name.clone();
                async move {
                    let status = Status::parse(&c.get().await.ok()?);
                    Some(Event::Player {
                        name,
                        identity: None,
                        status: Some(status),
                        track: None,
                        initial: false,
                    })
                }
            });
        let track_name = name.clone();
        let metadata = player
            .receive_metadata_changed()
            .await
            .filter_map(move |c| {
                let name = track_name.clone();
                async move {
                    let track = parse_metadata(&c.get().await.ok()?);
                    Some(Event::Player {
                        name,
                        identity: None,
                        status: None,
                        track: Some(track),
                        initial: false,
                    })
                }
            });
        let (handle, registration) = AbortHandle::new_pair();
        updates
            .push(Abortable::new(futures::stream::select(status, metadata), registration).boxed());
        streams.insert(name.clone(), handle);
        Ok(())
    }
    .await;
    if let Err(e) = result {
        log::warn!("media player {name}: {e}");
    }
}

async fn call(conn: &Connection, name: &str, action: Action) -> zbus::Result<()> {
    let player = PlayerProxy::builder(conn)
        .destination(name.to_owned())?
        .build()
        .await?;
    log::debug!("{action:?} on {name}");
    match action {
        Action::PlayPause => player.play_pause().await,
        Action::Play => player.play().await,
        Action::Pause => player.pause().await,
        Action::Next => player.next().await,
        Action::Previous => player.previous().await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zbus::zvariant::ObjectPath;

    fn metadata(pairs: Vec<(&str, Value<'static>)>) -> HashMap<String, OwnedValue> {
        pairs
            .into_iter()
            .map(|(k, v)| (k.to_owned(), v.try_to_owned().unwrap()))
            .collect()
    }

    #[test]
    fn parses_metadata() {
        let t = parse_metadata(&metadata(vec![
            (
                "mpris:trackid",
                Value::from(ObjectPath::try_from("/track/1").unwrap()),
            ),
            ("xesam:title", Value::from("Song")),
            ("xesam:artist", Value::from(vec!["A", "B"])),
            ("xesam:album", Value::from("Album")),
            ("mpris:artUrl", Value::from("file:///tmp/cover.png")),
        ]));
        assert_eq!(
            t,
            Track {
                id: "/track/1".into(),
                title: "Song".into(),
                artist: "A, B".into(),
                album: "Album".into(),
                art_url: Some("file:///tmp/cover.png".into()),
            }
        );
    }

    #[test]
    fn tolerates_a_string_artist_and_missing_fields() {
        let t = parse_metadata(&metadata(vec![("xesam:artist", Value::from("Solo"))]));
        assert_eq!(
            (t.artist.as_str(), t.title.as_str(), t.art_url),
            ("Solo", "", None)
        );
    }
}
