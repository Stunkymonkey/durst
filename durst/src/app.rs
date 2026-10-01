//! The iced_layershell daemon: owns all surfaces and turns events into
//! changes of the [`Store`]. After every event, [`App::sync`] reconciles the
//! open surfaces with the store.

use std::collections::HashMap;
use std::time::{Duration, Instant};

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

use crate::config::{Anchor, Config, MouseAction, Output};
use crate::core::layout::Margin;
use crate::core::notification::{Notification, Urgency};
use crate::core::store::{Insert, Store};
use crate::dbus::notifications::{self as dbus_notifications, emit_action_invoked, emit_closed};
use crate::ui::icons::Icon;
use crate::ui::markup::{self, Run};
use crate::ui::notification::{self as ui, Content};

const NAMESPACE: &str = "durst";
/// how often expiry is checked while a notification has a running timer
const TICK: Duration = Duration::from_millis(100);

#[to_layer_message(multi)]
#[derive(Debug, Clone)]
pub enum Message {
    Dbus(dbus_notifications::Event),
    Surface(window::Id, ui::Event),
    Tick(Instant),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Key {
    Notification(u32),
    /// the "+N more" indicator after the stack
    More,
}

struct Surface {
    window: window::Id,
    size: (u32, u32),
    margin: Margin,
}

/// Parsed content of a notification, prepared once instead of every frame.
struct Rendered {
    body: Vec<Run>,
    icon: Option<Icon>,
}

pub struct App {
    config: Config,
    store: Store,
    conn: Option<Connection>,
    rendered: HashMap<u32, Rendered>,
    surfaces: HashMap<Key, Surface>,
    windows: HashMap<window::Id, Key>,
}

pub fn run(config: Config) -> Result<(), iced_layershell::Error> {
    daemon(
        move || App::new(config.clone()),
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
    fn new(config: Config) -> Self {
        Self {
            config,
            store: Store::default(),
            conn: None,
            rendered: HashMap::new(),
            surfaces: HashMap::new(),
            windows: HashMap::new(),
        }
    }

    fn subscription(&self) -> Subscription<Message> {
        let dbus = Subscription::run(dbus_stream).map(Message::Dbus);
        if self.store.next_deadline().is_some() {
            Subscription::batch([dbus, iced::time::every(TICK).map(Message::Tick)])
        } else {
            dbus
        }
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        let task = self.handle(message);
        Task::batch([task, self.sync(Instant::now())])
    }

    fn handle(&mut self, message: Message) -> Task<Message> {
        use dbus_notifications::Event;
        match message {
            Message::Dbus(Event::Connected(conn)) => {
                log::info!("serving org.freedesktop.Notifications");
                self.conn = Some(conn);
                Task::none()
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
        if self.store.remove(id).is_none() {
            return Task::none();
        }
        log::debug!("close {id}: {reason:?}");
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
        let general = &self.config.general;
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
        let windows = &mut self.windows;
        self.surfaces.retain(|key, surface| {
            let keep = wanted.iter().any(|(k, _)| k == key);
            if !keep {
                windows.remove(&surface.window);
                tasks.push(Task::done(Message::RemoveWindow(surface.window)));
            }
            keep
        });

        for ((key, height), margin) in wanted.into_iter().zip(margins) {
            let size = (general.width, height);
            match self.surfaces.get_mut(&key) {
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
        Task::batch(tasks)
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

fn dbus_stream() -> impl futures::Stream<Item = dbus_notifications::Event> {
    use futures::SinkExt;
    iced::stream::channel(32, async |mut events| {
        match dbus_notifications::serve(events.clone()).await {
            Ok(conn) => {
                let _ = events
                    .send(dbus_notifications::Event::Connected(conn))
                    .await;
                std::future::pending::<()>().await;
            }
            Err(e) => {
                let _ = events.send(dbus_notifications::Event::Failed(e)).await;
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
