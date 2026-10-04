//! Whether the session is locked, from systemd-logind's `LockedHint`.
//!
//! `DURST_LOGIND_BUS_ADDRESS` points to another bus than the system bus; the
//! visual tests use it for a fake logind.

use futures::{SinkExt, Stream, StreamExt};
use iced::Subscription;
use zbus::zvariant::OwnedObjectPath;

#[zbus::proxy(
    interface = "org.freedesktop.login1.Session",
    default_service = "org.freedesktop.login1"
)]
trait Session {
    #[zbus(property)]
    fn id(&self) -> zbus::Result<String>;

    #[zbus(property)]
    fn locked_hint(&self) -> zbus::Result<bool>;
}

#[zbus::proxy(
    interface = "org.freedesktop.login1.Manager",
    default_service = "org.freedesktop.login1",
    default_path = "/org/freedesktop/login1"
)]
trait Manager {
    fn get_session(&self, id: &str) -> zbus::Result<OwnedObjectPath>;
}

/// `true` while the session is locked.
pub fn subscription() -> Subscription<bool> {
    Subscription::run(stream)
}

/// Watches the lock state, and again whenever the connection or logind goes
/// away; meanwhile the last known state stays.
fn stream() -> impl Stream<Item = bool> {
    iced::stream::channel(4, async |mut output| {
        let mut backoff = crate::retry::Backoff::new("lock detection");
        loop {
            backoff.attempt();
            let reason = match watch(&mut output).await {
                Ok(()) => "disconnected".to_string(),
                Err(e) => e.to_string(),
            };
            if output.is_closed() {
                return;
            }
            backoff.failed(reason).await;
        }
    })
}

async fn watch(output: &mut futures::channel::mpsc::Sender<bool>) -> zbus::Result<()> {
    let conn = match std::env::var("DURST_LOGIND_BUS_ADDRESS") {
        Ok(address) => {
            zbus::connection::Builder::address(address.as_str())?
                .build()
                .await?
        }
        Err(_) => zbus::Connection::system().await?,
    };
    // "auto" is the caller's session, or the user's graphical one. Signals
    // carry the session's real path, so look that up.
    let auto = SessionProxy::builder(&conn)
        .path("/org/freedesktop/login1/session/auto")?
        .build()
        .await?;
    let id = auto.id().await?;
    let path = ManagerProxy::new(&conn).await?.get_session(&id).await?;
    let session = SessionProxy::builder(&conn).path(path)?.build().await?;
    log::debug!("watching the lock state of session {id}");

    let mut changes = session.receive_locked_hint_changed().await;
    let _ = output.send(session.locked_hint().await?).await;
    while let Some(change) = changes.next().await {
        let locked = change.get().await?;
        log::debug!("session locked: {locked}");
        if output.send(locked).await.is_err() {
            break;
        }
    }
    Ok(())
}
