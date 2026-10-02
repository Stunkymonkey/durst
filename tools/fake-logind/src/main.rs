//! A fake systemd-logind for the visual test harness: serves
//! `org.freedesktop.login1` on the session bus with one session, `c1`, also
//! reachable as `session/auto`. `lock` / `unlock` lines on stdin set its
//! `LockedHint` and emit `PropertiesChanged`, like the real logind.

use tokio::io::{AsyncBufReadExt, BufReader};
use zbus::interface;
use zbus::zvariant::OwnedObjectPath;

const SESSION: &str = "/org/freedesktop/login1/session/c1";
const AUTO: &str = "/org/freedesktop/login1/session/auto";

struct Manager;

#[interface(name = "org.freedesktop.login1.Manager")]
impl Manager {
    fn get_session(&self, id: &str) -> zbus::fdo::Result<OwnedObjectPath> {
        match id {
            "c1" => Ok(OwnedObjectPath::try_from(SESSION).unwrap()),
            _ => Err(zbus::fdo::Error::Failed(format!("no session {id}"))),
        }
    }
}

struct Session {
    locked: bool,
}

#[interface(name = "org.freedesktop.login1.Session")]
impl Session {
    #[zbus(property)]
    fn id(&self) -> String {
        "c1".into()
    }

    #[zbus(property)]
    fn locked_hint(&self) -> bool {
        self.locked
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> zbus::Result<()> {
    let conn = zbus::connection::Builder::session()?
        .name("org.freedesktop.login1")?
        .serve_at("/org/freedesktop/login1", Manager)?
        .serve_at(SESSION, Session { locked: false })?
        .serve_at(AUTO, Session { locked: false })?
        .build()
        .await?;

    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    while let Some(line) = lines.next_line().await? {
        let locked = match line.trim() {
            "lock" => true,
            "unlock" => false,
            "" => continue,
            other => {
                eprintln!("unknown command {other:?}");
                continue;
            }
        };
        for path in [SESSION, AUTO] {
            let iface = conn.object_server().interface::<_, Session>(path).await?;
            let mut session = iface.get_mut().await;
            session.locked = locked;
            session.locked_hint_changed(iface.signal_emitter()).await?;
        }
    }
    Ok(())
}
