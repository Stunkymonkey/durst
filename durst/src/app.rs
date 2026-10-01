//! The iced_layershell daemon: owns all surfaces and turns events into
//! changes of the [`Store`]. After every event, [`App::sync`] reconciles the
//! open surfaces with the store.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use durst_proto::control::DaemonInfo;
use durst_proto::notifications::CloseReason;
use iced::widget::text;
use iced::{Color, Element, Subscription, Task, mouse, window};
use iced_layershell::daemon;
use iced_layershell::reexport::{
    Anchor as LayerAnchor, KeyboardInteractivity, Layer, NewLayerShellSettings, OutputOption,
};
use iced_layershell::settings::{LayerShellSettings, Settings, StartMode};
use iced_layershell::to_layer_message;
use zbus::Connection;

use crate::config::{self, Anchor, Config, MouseAction, Output};
use crate::core::history::History;
use crate::core::layout::Margin;
use crate::core::notification::{Notification, Urgency};
use crate::core::store::{Insert, Store};
use crate::dbus::control::{self, Command, Reply, Status, emit_status_changed};
use crate::dbus::notifications::{emit_action_invoked, emit_closed};
use crate::dbus::{self, Event};
use crate::ui::icons::Icon;
use crate::ui::markup::{self, Run};
use crate::ui::notification::{self as ui, Content};

const NAMESPACE: &str = "durst";
/// how often expiry is checked while a notification has a running timer
const TICK: Duration = Duration::from_millis(100);

#[to_layer_message(multi)]
#[derive(Debug, Clone)]
pub enum Message {
    Dbus(Event),
    Surface(window::Id, ui::Event),
    /// the surface exists now; changes sent before this are lost
    Opened(window::Id),
    Tick(Instant),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Key {
    Notification(u32),
    /// the "+N more" indicator after the stack
    More,
}

/// A layer surface and the size and margin last sent for it.
///
/// iced_layershell silently drops changes for surfaces it hasn't created
/// yet, so changes are only sent once the surface is `opened`; until then
/// `sync` leaves `size`/`margin` at the values it was created with.
struct Surface {
    window: window::Id,
    opened: bool,
    size: (u32, u32),
    margin: Margin,
}

/// Parsed content of a notification, prepared once instead of every frame.
struct Rendered {
    body: Vec<Run>,
    icon: Option<Icon>,
}

/// Where the config comes from, for reloading.
#[derive(Debug, Clone)]
pub struct ConfigSource {
    pub path: PathBuf,
    /// given with `-c`: a missing file is an error
    pub explicit: bool,
}

pub struct App {
    config: Config,
    source: ConfigSource,
    store: Store,
    history: History,
    conn: Option<Connection>,
    /// counts shared with the control interface, and the last published ones
    status: Option<Arc<Mutex<Status>>>,
    published: Status,
    rendered: HashMap<u32, Rendered>,
    surfaces: HashMap<Key, Surface>,
    windows: HashMap<window::Id, Key>,
    /// surfaces closed before they were opened, removed once they are
    orphans: HashSet<window::Id>,
}

pub fn run(config: Config, source: ConfigSource) -> Result<(), iced_layershell::Error> {
    daemon(
        move || App::new(config.clone(), source.clone()),
        || NAMESPACE.to_string(),
        App::update,
        App::view,
    )
    .subscription(App::subscription)
    .style(|_, theme| iced::theme::Style {
        // the surface itself is transparent, only the container is drawn
        background_color: Color::TRANSPARENT,
        text_color: theme.palette().text,
    })
    .settings(Settings {
        layer_settings: LayerShellSettings {
            start_mode: StartMode::Background,
            ..Default::default()
        },
        ..Default::default()
    })
    .run()
}

impl App {
    fn new(config: Config, source: ConfigSource) -> Self {
        Self {
            history: History::new(config.history.length),
            config,
            source,
            store: Store::default(),
            conn: None,
            status: None,
            published: Status::default(),
            rendered: HashMap::new(),
            surfaces: HashMap::new(),
            windows: HashMap::new(),
            orphans: HashSet::new(),
        }
    }

    fn subscription(&self) -> Subscription<Message> {
        let dbus = Subscription::run(dbus_stream).map(Message::Dbus);
        let opened = window::open_events().map(Message::Opened);
        let tick = self
            .store
            .next_deadline()
            .map(|_| iced::time::every(TICK).map(Message::Tick));
        Subscription::batch([dbus, opened].into_iter().chain(tick))
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        let task = self.handle(message);
        Task::batch([task, self.sync(Instant::now())])
    }

    fn handle(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Dbus(Event::Connected(conn, status)) => {
                log::info!("serving org.freedesktop.Notifications");
                self.conn = Some(conn);
                self.status = Some(status);
                Task::none()
            }
            Message::Dbus(Event::Control(command, responder)) => {
                log::debug!("control: {command:?}");
                let (reply, task) = self.control(command);
                responder.send(reply);
                task
            }
            Message::Dbus(Event::Failed(e)) => {
                log::error!("{e}");
                std::process::exit(1);
            }
            Message::Dbus(Event::Notify(n)) => self.notify(*n),
            Message::Dbus(Event::Close(id)) => self.close(id, CloseReason::Closed),
            Message::Surface(window, event) => match self.windows.get(&window) {
                Some(&Key::Notification(id)) => self.surface_event(id, event),
                _ => Task::none(),
            },
            Message::Opened(window) => {
                if self.orphans.remove(&window) {
                    return Task::done(Message::RemoveWindow(window));
                }
                if let Some(surface) = self.surfaces.values_mut().find(|s| s.window == window) {
                    surface.opened = true;
                }
                Task::none()
            }
            Message::Tick(now) => {
                self.store.update_timers(&self.config.general, now);
                let expired = self.store.expired(now);
                Task::batch(
                    expired
                        .into_iter()
                        .map(|id| self.close(id, CloseReason::Expired)),
                )
            }
            _ => Task::none(),
        }
    }

    fn view(&self, window: window::Id) -> Element<'_, Message> {
        let element = match self.windows.get(&window) {
            Some(Key::Notification(id)) => {
                let (Some(entry), Some(rendered)) = (self.store.get(*id), self.rendered.get(id))
                else {
                    return text("").into();
                };
                let content = Content {
                    notification: &entry.notification,
                    count: entry.count,
                    body: &rendered.body,
                    icon: rendered.icon.as_ref(),
                };
                ui::view(content, self.config.style(entry.notification.hints.urgency))
            }
            Some(Key::More) => ui::more_view(
                self.store.waiting(&self.config.general),
                self.config.style(Urgency::Normal),
            ),
            None => return text("").into(),
        };
        element.map(move |event| Message::Surface(window, event))
    }

    fn notify(&mut self, n: Notification) -> Task<Message> {
        let style = self.config.style(n.hints.urgency);
        let rendered = Rendered {
            body: markup::parse(&n.body),
            icon: crate::ui::icons::resolve(
                &n,
                style.icon_size,
                self.config.general.icon_theme.as_deref(),
            ),
        };
        let timeout = n.timeout(&self.config.urgency);
        let id = n.id;

        let mut task = Task::none();
        match self.store.insert(n, 0, timeout, &self.config.general) {
            Insert::Added | Insert::Replaced => {}
            Insert::Superseded(old) => {
                // keep the surface, it now shows the new notification
                if let Some(surface) = self.surfaces.remove(&Key::Notification(old)) {
                    self.windows.insert(surface.window, Key::Notification(id));
                    self.surfaces.insert(Key::Notification(id), surface);
                }
                self.rendered.remove(&old);
                task = self.emit_closed(old, CloseReason::Undefined);
            }
        }
        // the height depends on the duplicate counter, known only now
        let entry = self.store.get(id).expect("just inserted");
        let content = Content {
            notification: &entry.notification,
            count: entry.count,
            body: &rendered.body,
            icon: rendered.icon.as_ref(),
        };
        let height = ui::height(&content, style, self.config.general.width);
        log::debug!("notification {id}: height {height}, timeout {timeout:?}");
        self.store.set_height(id, height);
        self.rendered.insert(id, rendered);
        task
    }

    fn close(&mut self, id: u32, reason: CloseReason) -> Task<Message> {
        let Some(entry) = self.store.remove(id) else {
            return Task::none();
        };
        log::debug!("close {id}: {reason:?}");
        // the user may want these back; the sender closed the others itself
        if matches!(reason, CloseReason::Expired | CloseReason::Dismissed) {
            self.history.push(entry.notification);
        }
        self.rendered.remove(&id);
        self.emit_closed(id, reason)
    }

    fn close_all(&mut self) -> Task<Message> {
        let ids: Vec<u32> = self
            .store
            .visible(&self.config.general)
            .iter()
            .map(|e| e.notification.id)
            .collect();
        Task::batch(
            ids.into_iter()
                .map(|id| self.close(id, CloseReason::Dismissed)),
        )
    }

    fn surface_event(&mut self, id: u32, event: ui::Event) -> Task<Message> {
        log::debug!("surface event of {id}: {event:?}");
        let mouse = &self.config.mouse;
        match event {
            ui::Event::Press(button) => {
                let actions = match button {
                    mouse::Button::Left => mouse.left.clone(),
                    mouse::Button::Middle => mouse.middle.clone(),
                    mouse::Button::Right => mouse.right.clone(),
                    _ => vec![],
                };
                self.mouse_actions(id, &actions)
            }
            ui::Event::Scroll(y) if y > 0.0 => {
                let actions = mouse.scroll_up.clone();
                self.mouse_actions(id, &actions)
            }
            ui::Event::Scroll(y) if y < 0.0 => {
                let actions = mouse.scroll_down.clone();
                self.mouse_actions(id, &actions)
            }
            ui::Event::Scroll(_) => Task::none(),
            ui::Event::Hover(hovered) => {
                self.store.set_hovered(id, hovered);
                Task::none()
            }
            ui::Event::Action(key) => {
                let invoke = self.invoke(id, &key);
                Task::batch([invoke, self.close_after_action(id)])
            }
            ui::Event::Link(url) => {
                self.open_url(&url);
                Task::none()
            }
        }
    }

    fn mouse_actions(&mut self, id: u32, actions: &[MouseAction]) -> Task<Message> {
        let mut tasks = Vec::new();
        let mut invoked = false;
        for action in actions {
            match action {
                MouseAction::None => {}
                MouseAction::CloseCurrent if invoked => tasks.push(self.close_after_action(id)),
                MouseAction::CloseCurrent => tasks.push(self.close(id, CloseReason::Dismissed)),
                MouseAction::CloseAll => tasks.push(self.close_all()),
                MouseAction::DoAction => {
                    let key = self
                        .store
                        .get(id)
                        .and_then(|e| default_action(&e.notification));
                    if let Some(key) = key {
                        tasks.push(self.invoke(id, &key));
                        invoked = true;
                    }
                }
                MouseAction::OpenUrl => {
                    let url = self
                        .rendered
                        .get(&id)
                        .and_then(|r| markup::first_link(&r.body))
                        .map(str::to_owned);
                    if let Some(url) = url {
                        self.open_url(&url);
                    }
                }
            }
        }
        Task::batch(tasks)
    }

    fn invoke(&self, id: u32, key: &str) -> Task<Message> {
        log::debug!("invoke action {key:?} of {id}");
        match self.conn.clone() {
            Some(conn) => Task::future(emit_action_invoked(conn, id, key.to_owned())).discard(),
            None => Task::none(),
        }
    }

    /// After an action the notification closes, unless it asked to stay.
    fn close_after_action(&mut self, id: u32) -> Task<Message> {
        match self.store.get(id) {
            Some(e) if e.notification.hints.resident => Task::none(),
            _ => self.close(id, CloseReason::Dismissed),
        }
    }

    fn open_url(&self, url: &str) {
        let Some((program, args)) = self.config.general.browser.split_first() else {
            return;
        };
        log::debug!("open {url} with {program}");
        match std::process::Command::new(program)
            .args(args)
            .arg(url)
            .spawn()
        {
            // reap the child so it doesn't stay a zombie
            Ok(mut child) => drop(std::thread::spawn(move || child.wait())),
            Err(e) => log::warn!("cannot run {program}: {e}"),
        }
    }

    fn emit_closed(&self, id: u32, reason: CloseReason) -> Task<Message> {
        match self.conn.clone() {
            Some(conn) => Task::future(emit_closed(conn, id, reason)).discard(),
            None => Task::none(),
        }
    }

    /// Opens, moves, resizes and closes surfaces to match the store.
    fn sync(&mut self, now: Instant) -> Task<Message> {
        let general = &self.config.general.clone();
        self.store.update_timers(general, now);

        let mut wanted: Vec<(Key, u32)> = self
            .store
            .visible(general)
            .iter()
            .map(|e| (Key::Notification(e.notification.id), e.height))
            .collect();
        let waiting = self.store.waiting(general);
        let more_height =
            (waiting > 0).then(|| ui::more_height(waiting, self.config.style(Urgency::Normal)));
        wanted.extend(more_height.map(|h| (Key::More, h)));
        let margins = self.store.margins(general, more_height);

        let mut tasks = Vec::new();
        let was_empty = self.surfaces.is_empty();
        let gone: Vec<Key> = self
            .surfaces
            .keys()
            .filter(|key| !wanted.iter().any(|(k, _)| k == *key))
            .copied()
            .collect();
        for key in gone {
            tasks.push(self.remove_surface(key));
        }

        for ((key, height), margin) in wanted.into_iter().zip(margins) {
            let size = (general.width, height);
            match self.surfaces.get_mut(&key) {
                // changes wait until the surface exists
                Some(surface) if !surface.opened => {}
                Some(surface) => {
                    if surface.size != size {
                        surface.size = size;
                        tasks.push(Task::done(Message::SizeChange {
                            id: surface.window,
                            size,
                        }));
                    }
                    if surface.margin != margin {
                        surface.margin = margin;
                        tasks.push(Task::done(Message::MarginChange {
                            id: surface.window,
                            margin,
                        }));
                    }
                }
                None => {
                    let (window, open) = Message::layershell_open(NewLayerShellSettings {
                        size: Some(size),
                        layer: Layer::Overlay,
                        anchor: layer_anchor(general.anchor),
                        exclusive_zone: None,
                        margin: Some(margin),
                        keyboard_interactivity: KeyboardInteractivity::None,
                        output_option: match &general.output {
                            Output::Focused => OutputOption::LastOutput,
                            Output::Name(name) => OutputOption::OutputName(name.clone()),
                        },
                        events_transparent: false,
                        namespace: Some(NAMESPACE.to_string()),
                    });
                    self.windows.insert(window, key);
                    self.surfaces.insert(
                        key,
                        Surface {
                            window,
                            opened: false,
                            size,
                            margin,
                        },
                    );
                    tasks.push(open);
                }
            }
        }

        // with an empty stack, the next notification may go to another output
        if !was_empty && self.surfaces.is_empty() && general.output == Output::Focused {
            tasks.push(Task::done(Message::ForgetLastOutput));
        }
        tasks.push(self.publish_status());
        Task::batch(tasks)
    }

    fn publish_status(&mut self) -> Task<Message> {
        let general = &self.config.general;
        let status = Status {
            displayed: self.store.visible(general).len() as u32,
            waiting: self.store.waiting(general) as u32,
            history: self.history.len() as u32,
        };
        let (Some(shared), Some(conn)) = (&self.status, &self.conn) else {
            return Task::none();
        };
        if status == self.published {
            return Task::none();
        }
        *shared.lock().unwrap() = status;
        let old = std::mem::replace(&mut self.published, status);
        Task::future(emit_status_changed(conn.clone(), old, status)).discard()
    }

    /// Handles a durstctl command; the reply goes back over D-Bus.
    fn control(&mut self, command: Command) -> (Reply, Task<Message>) {
        let general = &self.config.general;
        let resolve = |store: &Store, id: u32| match id {
            0 => store
                .newest_visible(general)
                .ok_or_else(|| Reply::NotFound("no notification is displayed".into())),
            id => store
                .get(id)
                .map(|e| e.notification.id)
                .ok_or_else(|| Reply::NotFound(format!("no notification {id}"))),
        };
        match command {
            Command::List => {
                let visible = self.store.visible(general).len();
                let list = self
                    .store
                    .iter()
                    .enumerate()
                    .map(|(i, e)| {
                        let state = if i < visible { "displayed" } else { "waiting" };
                        control::info(&e.notification, state)
                    })
                    .collect();
                (Reply::Notifications(list), Task::none())
            }
            Command::Close(id) => match resolve(&self.store, id) {
                Ok(id) => (Reply::Done, self.close(id, CloseReason::Dismissed)),
                Err(reply) => (reply, Task::none()),
            },
            Command::CloseAll => (Reply::Done, self.close_all()),
            Command::Action(id, key) => {
                let id = match resolve(&self.store, id) {
                    Ok(id) => id,
                    Err(reply) => return (reply, Task::none()),
                };
                let n = &self.store.get(id).expect("resolved").notification;
                let key = match key.as_str() {
                    "" => default_action(n),
                    key => n
                        .actions
                        .iter()
                        .any(|(k, _)| k == key)
                        .then(|| key.to_owned()),
                };
                match key {
                    Some(key) => {
                        let invoke = self.invoke(id, &key);
                        (
                            Reply::Done,
                            Task::batch([invoke, self.close_after_action(id)]),
                        )
                    }
                    None => (
                        Reply::NotFound(format!("notification {id} has no such action")),
                        Task::none(),
                    ),
                }
            }
            Command::History => {
                let list = self
                    .history
                    .iter()
                    .map(|n| control::info(n, "history"))
                    .collect();
                (Reply::Notifications(list), Task::none())
            }
            Command::HistoryPop => match self.history.pop() {
                Some(mut n) => {
                    let id = n.id;
                    if self.config.history.sticky {
                        n.expire_timeout = 0;
                    }
                    (Reply::Id(id), self.notify(n))
                }
                None => (Reply::NotFound("the history is empty".into()), Task::none()),
            },
            Command::HistoryClear => {
                self.history.clear();
                (Reply::Done, Task::none())
            }
            Command::Reload => match config::load(&self.source.path, self.source.explicit) {
                Ok(config) => (Reply::Done, self.apply_config(config)),
                Err(e) => {
                    log::warn!("keeping the old config: {e}");
                    (Reply::InvalidConfig(e), Task::none())
                }
            },
            Command::Info => {
                let info = DaemonInfo {
                    version: env!("CARGO_PKG_VERSION").to_owned(),
                    config_path: self.source.path.display().to_string(),
                    displayed: self.store.visible(general).len() as u32,
                    waiting: self.store.waiting(general) as u32,
                    history: self.history.len() as u32,
                };
                (Reply::Info(info), Task::none())
            }
        }
    }

    /// Switches to a new config: re-renders every notification (styles and
    /// sizes may have changed) and reopens all surfaces, since anchor and
    /// output can only be set when a surface is created.
    fn apply_config(&mut self, config: Config) -> Task<Message> {
        log::info!("config reloaded");
        self.config = config;
        self.history.set_capacity(self.config.history.length);
        let ids: Vec<u32> = self.store.iter().map(|e| e.notification.id).collect();
        for id in ids {
            let entry = self.store.get(id).expect("listed");
            let n = &entry.notification;
            let style = self.config.style(n.hints.urgency);
            let rendered = Rendered {
                body: markup::parse(&n.body),
                icon: crate::ui::icons::resolve(
                    n,
                    style.icon_size,
                    self.config.general.icon_theme.as_deref(),
                ),
            };
            let content = Content {
                notification: n,
                count: entry.count,
                body: &rendered.body,
                icon: rendered.icon.as_ref(),
            };
            let height = ui::height(&content, style, self.config.general.width);
            self.store.set_height(id, height);
            self.rendered.insert(id, rendered);
        }
        let keys: Vec<Key> = self.surfaces.keys().copied().collect();
        Task::batch(keys.into_iter().map(|key| self.remove_surface(key)))
    }

    fn remove_surface(&mut self, key: Key) -> Task<Message> {
        let Some(surface) = self.surfaces.remove(&key) else {
            return Task::none();
        };
        self.windows.remove(&surface.window);
        if surface.opened {
            Task::done(Message::RemoveWindow(surface.window))
        } else {
            // removing it now would be dropped, and it would appear later
            self.orphans.insert(surface.window);
            Task::none()
        }
    }
}

/// The action triggered by clicking: "default", or the only one there is.
fn default_action(n: &Notification) -> Option<String> {
    match &n.actions[..] {
        [(key, _)] => Some(key.clone()),
        actions => actions
            .iter()
            .find(|(key, _)| key == "default")
            .map(|(key, _)| key.clone()),
    }
}

fn dbus_stream() -> impl futures::Stream<Item = Event> {
    use futures::SinkExt;
    iced::stream::channel(32, async |mut events| {
        match dbus::serve(events.clone()).await {
            Ok((conn, status)) => {
                let _ = events.send(Event::Connected(conn, status)).await;
                std::future::pending::<()>().await;
            }
            Err(e) => {
                let _ = events.send(Event::Failed(e)).await;
            }
        }
    })
}

fn layer_anchor(anchor: Anchor) -> LayerAnchor {
    match anchor {
        Anchor::TopLeft => LayerAnchor::Top | LayerAnchor::Left,
        Anchor::Top => LayerAnchor::Top,
        Anchor::TopRight => LayerAnchor::Top | LayerAnchor::Right,
        Anchor::BottomLeft => LayerAnchor::Bottom | LayerAnchor::Left,
        Anchor::Bottom => LayerAnchor::Bottom,
        Anchor::BottomRight => LayerAnchor::Bottom | LayerAnchor::Right,
    }
}
