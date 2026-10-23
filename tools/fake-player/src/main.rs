//! A fake MPRIS media player for the visual test harness.
//!
//! usage: fake-player NAME — owns `org.mpris.MediaPlayer2.NAME` on the
//! session bus. Every method call is printed to stdout (`Next`, `PlayPause`,
//! ...). Commands on stdin, one per line:
//!   status Playing|Paused|Stopped
//!   track ID|TITLE|ARTIST|ART_URL     (ART_URL may be empty)

use std::collections::HashMap;

use tokio::io::{AsyncBufReadExt, BufReader};
use zbus::interface;
use zbus::object_server::SignalEmitter;
use zbus::zvariant::{ObjectPath, OwnedValue, Value};

const PATH: &str = "/org/mpris/MediaPlayer2";

struct Root {
    identity: String,
}

#[interface(name = "org.mpris.MediaPlayer2")]
impl Root {
    #[zbus(property)]
    fn identity(&self) -> String {
        self.identity.clone()
    }

    #[zbus(property)]
    fn can_quit(&self) -> bool {
        false
    }

    #[zbus(property)]
    fn can_raise(&self) -> bool {
        false
    }

    #[zbus(property)]
    fn has_track_list(&self) -> bool {
        false
    }
}

#[derive(Default)]
struct Track {
    id: String,
    title: String,
    artist: String,
    art_url: String,
}

struct Player {
    status: String,
    track: Track,
}

impl Player {
    fn called(&self, method: &str) {
        println!("{method}");
    }
}

#[interface(name = "org.mpris.MediaPlayer2.Player")]
impl Player {
    async fn play_pause(&mut self, #[zbus(signal_emitter)] emitter: SignalEmitter<'_>) {
        self.called("PlayPause");
        self.status = if self.status == "Playing" {
            "Paused"
        } else {
            "Playing"
        }
        .into();
        let _ = self.playback_status_changed(&emitter).await;
    }

    async fn play(&mut self, #[zbus(signal_emitter)] emitter: SignalEmitter<'_>) {
        self.called("Play");
        self.status = "Playing".into();
        let _ = self.playback_status_changed(&emitter).await;
    }

    async fn pause(&mut self, #[zbus(signal_emitter)] emitter: SignalEmitter<'_>) {
        self.called("Pause");
        self.status = "Paused".into();
        let _ = self.playback_status_changed(&emitter).await;
    }

    fn next(&self) {
        self.called("Next");
    }

    fn previous(&self) {
        self.called("Previous");
    }

    fn stop(&self) {
        self.called("Stop");
    }

    #[zbus(property)]
    fn playback_status(&self) -> String {
        self.status.clone()
    }

    #[zbus(property)]
    fn metadata(&self) -> HashMap<String, OwnedValue> {
        let t = &self.track;
        let mut m: HashMap<String, OwnedValue> = HashMap::new();
        let mut put = |k: &str, v: Value<'_>| {
            m.insert(k.into(), v.try_to_owned().unwrap());
        };
        if !t.id.is_empty() {
            let path = format!("/org/durst/track/{}", t.id);
            put(
                "mpris:trackid",
                Value::from(ObjectPath::try_from(path).unwrap()),
            );
        }
        put("xesam:title", Value::from(t.title.as_str()));
        put("xesam:artist", Value::from(vec![t.artist.as_str()]));
        if !t.art_url.is_empty() {
            put("mpris:artUrl", Value::from(t.art_url.as_str()));
        }
        m
    }

    #[zbus(property)]
    fn can_go_next(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn can_go_previous(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn can_play(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn can_pause(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn can_control(&self) -> bool {
        true
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> zbus::Result<()> {
    let name = std::env::args().nth(1).expect("usage: fake-player NAME");
    let conn = zbus::connection::Builder::session()?
        .name(format!("org.mpris.MediaPlayer2.{name}"))?
        .serve_at(
            PATH,
            Root {
                identity: format!("Fake {name}"),
            },
        )?
        .serve_at(
            PATH,
            Player {
                status: "Stopped".into(),
                track: Track::default(),
            },
        )?
        .build()
        .await?;

    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    while let Some(line) = lines.next_line().await? {
        let iface = conn.object_server().interface::<_, Player>(PATH).await?;
        let mut player = iface.get_mut().await;
        match line.split_once(' ') {
            Some(("status", status)) => {
                player.status = status.trim().into();
                player
                    .playback_status_changed(iface.signal_emitter())
                    .await?;
            }
            Some(("track", track)) => {
                let mut parts = track.splitn(4, '|');
                let mut next = || parts.next().unwrap_or("").trim().to_owned();
                player.track = Track {
                    id: next(),
                    title: next(),
                    artist: next(),
                    art_url: next(),
                };
                player.metadata_changed(iface.signal_emitter()).await?;
            }
            _ => eprintln!("unknown command {line:?}"),
        }
    }
    Ok(())
}
