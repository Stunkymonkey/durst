//! Server side of `org.durst_notification.Durst1`.
//!
//! Commands are answered by the UI loop, which owns all state: each method
//! sends a [`Command`] with a [`Responder`] and waits for the [`Reply`]. The
//! counts are read from a [`Status`] snapshot the UI loop keeps up to date.

use std::sync::{Arc, Mutex};

use durst_proto::control::{DaemonInfo, NotificationInfo, OBJECT_PATH};
use futures::SinkExt;
use futures::channel::mpsc::Sender;
use futures::channel::oneshot;
use zbus::object_server::SignalEmitter;
use zbus::{Connection, interface};

use super::Event;
use crate::core::notification::Notification;

#[derive(Debug, Clone)]
pub enum Command {
    List,
    /// 0 = the newest displayed notification
    Close(u32),
    CloseAll,
    /// id (0 = newest displayed), action key ("" = default action)
    Action(u32, String),
    History,
    HistoryPop,
    HistoryClear,
    Reload,
    Info,
    Modes(ModeChange),
    KnownModes,
}

#[derive(Debug, Clone)]
pub enum ModeChange {
    Set(Vec<String>),
    Enable(String),
    Disable(String),
    Toggle(String),
}

#[derive(Debug)]
pub enum Reply {
    Done,
    Id(u32),
    Notifications(Vec<NotificationInfo>),
    Info(DaemonInfo),
    Modes(Vec<String>),
    NotFound(String),
    InvalidConfig(String),
}

/// Answers a [`Command`] once; cloneable so it can travel in a message.
#[derive(Clone)]
pub struct Responder(Arc<Mutex<Option<oneshot::Sender<Reply>>>>);

impl Responder {
    pub fn send(&self, reply: Reply) {
        if let Some(tx) = self.0.lock().unwrap().take() {
            let _ = tx.send(reply);
        }
    }
}

impl std::fmt::Debug for Responder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Responder")
    }
}

/// State published as properties.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Status {
    pub displayed: u32,
    pub waiting: u32,
    pub held: u32,
    pub history: u32,
    pub modes: Vec<String>,
}

#[derive(Debug, zbus::DBusError)]
#[zbus(prefix = "org.durst_notification.Error")]
pub enum Error {
    #[zbus(error)]
    ZBus(zbus::Error),
    NotFound(String),
    InvalidConfig(String),
}

pub struct Control {
    events: Sender<Event>,
    status: Arc<Mutex<Status>>,
}

impl Control {
    pub fn new(events: Sender<Event>, status: Arc<Mutex<Status>>) -> Self {
        Self { events, status }
    }

    async fn request(&self, command: Command) -> Result<Reply, Error> {
        let (tx, rx) = oneshot::channel();
        let responder = Responder(Arc::new(Mutex::new(Some(tx))));
        self.events
            .clone()
            .send(Event::Control(command, responder))
            .await
            .map_err(|e| Error::ZBus(zbus::Error::Failure(e.to_string())))?;
        match rx.await {
            Ok(Reply::NotFound(msg)) => Err(Error::NotFound(msg)),
            Ok(Reply::InvalidConfig(msg)) => Err(Error::InvalidConfig(msg)),
            Ok(reply) => Ok(reply),
            Err(_) => Err(Error::ZBus(zbus::Error::Failure("no reply".into()))),
        }
    }

    async fn done(&self, command: Command) -> Result<(), Error> {
        self.request(command).await.map(|_| ())
    }

    async fn modes(&self, command: Command) -> Result<Vec<String>, Error> {
        match self.request(command).await? {
            Reply::Modes(modes) => Ok(modes),
            reply => Err(unexpected(reply)),
        }
    }

    async fn notifications(&self, command: Command) -> Result<Vec<NotificationInfo>, Error> {
        match self.request(command).await? {
            Reply::Notifications(list) => Ok(list),
            reply => Err(unexpected(reply)),
        }
    }
}

fn unexpected(reply: Reply) -> Error {
    Error::ZBus(zbus::Error::Failure(format!("unexpected reply {reply:?}")))
}

#[interface(name = "org.durst_notification.Durst1")]
impl Control {
    async fn list(&self) -> Result<Vec<NotificationInfo>, Error> {
        self.notifications(Command::List).await
    }

    async fn close(&self, id: u32) -> Result<(), Error> {
        self.done(Command::Close(id)).await
    }

    async fn close_all(&self) -> Result<(), Error> {
        self.done(Command::CloseAll).await
    }

    async fn action(&self, id: u32, key: String) -> Result<(), Error> {
        self.done(Command::Action(id, key)).await
    }

    async fn history(&self) -> Result<Vec<NotificationInfo>, Error> {
        self.notifications(Command::History).await
    }

    async fn history_pop(&self) -> Result<u32, Error> {
        match self.request(Command::HistoryPop).await? {
            Reply::Id(id) => Ok(id),
            reply => Err(unexpected(reply)),
        }
    }

    async fn history_clear(&self) -> Result<(), Error> {
        self.done(Command::HistoryClear).await
    }

    async fn reload(&self) -> Result<(), Error> {
        self.done(Command::Reload).await
    }

    async fn info(&self) -> Result<DaemonInfo, Error> {
        match self.request(Command::Info).await? {
            Reply::Info(info) => Ok(info),
            reply => Err(unexpected(reply)),
        }
    }

    async fn set_modes(&self, modes: Vec<String>) -> Result<Vec<String>, Error> {
        self.modes(Command::Modes(ModeChange::Set(modes))).await
    }

    async fn enable_mode(&self, mode: String) -> Result<Vec<String>, Error> {
        self.modes(Command::Modes(ModeChange::Enable(mode))).await
    }

    async fn disable_mode(&self, mode: String) -> Result<Vec<String>, Error> {
        self.modes(Command::Modes(ModeChange::Disable(mode))).await
    }

    async fn toggle_mode(&self, mode: String) -> Result<Vec<String>, Error> {
        self.modes(Command::Modes(ModeChange::Toggle(mode))).await
    }

    async fn known_modes(&self) -> Result<Vec<String>, Error> {
        self.modes(Command::KnownModes).await
    }

    #[zbus(property)]
    fn active_modes(&self) -> Vec<String> {
        self.status.lock().unwrap().modes.clone()
    }

    #[zbus(property)]
    fn held_count(&self) -> u32 {
        self.status.lock().unwrap().held
    }

    #[zbus(property)]
    fn displayed_count(&self) -> u32 {
        self.status.lock().unwrap().displayed
    }

    #[zbus(property)]
    fn waiting_count(&self) -> u32 {
        self.status.lock().unwrap().waiting
    }

    #[zbus(property)]
    fn history_count(&self) -> u32 {
        self.status.lock().unwrap().history
    }
}

/// Emits `PropertiesChanged` for the counts that differ between `old` and `new`.
pub async fn emit_status_changed(conn: Connection, old: Status, new: Status) {
    let result: zbus::Result<()> = async {
        let iface = conn
            .object_server()
            .interface::<_, Control>(OBJECT_PATH)
            .await?;
        let emitter: &SignalEmitter<'_> = iface.signal_emitter();
        let control = iface.get().await;
        if old.displayed != new.displayed {
            control.displayed_count_changed(emitter).await?;
        }
        if old.waiting != new.waiting {
            control.waiting_count_changed(emitter).await?;
        }
        if old.held != new.held {
            control.held_count_changed(emitter).await?;
        }
        if old.history != new.history {
            control.history_count_changed(emitter).await?;
        }
        if old.modes != new.modes {
            control.active_modes_changed(emitter).await?;
        }
        Ok(())
    }
    .await;
    if let Err(e) = result {
        log::warn!("cannot emit PropertiesChanged: {e}");
    }
}

pub fn info(n: &Notification, state: &str) -> NotificationInfo {
    NotificationInfo {
        id: n.id,
        app_name: n.app_name.clone(),
        summary: n.summary.clone(),
        body: n.body.clone(),
        urgency: n.hints.urgency.name().to_owned(),
        category: n.hints.category.clone().unwrap_or_default(),
        actions: n.actions.clone(),
        received: n.received,
        state: state.to_owned(),
    }
}
