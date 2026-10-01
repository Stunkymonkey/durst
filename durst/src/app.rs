//! The iced_layershell daemon: owns all surfaces and turns events into
//! changes of the [`Store`].

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use durst_proto::notifications::CloseReason;
use iced::widget::text;
use iced::window;
use iced::{Color, Element, Subscription, Task};
use iced_layershell::daemon;
use iced_layershell::reexport::{
    Anchor as LayerAnchor, KeyboardInteractivity, Layer, NewLayerShellSettings, OutputOption,
};
use iced_layershell::settings::{LayerShellSettings, Settings, StartMode};
use iced_layershell::to_layer_message;
use zbus::Connection;

use crate::config::{Anchor, Config};
use crate::core::notification::Notification;
use crate::core::store::{Store, Upsert};
use crate::dbus::notifications::{self as dbus_notifications, emit_closed};
use crate::ui;

const NAMESPACE: &str = "durst";
/// how often expiry is checked while a notification has a timeout
const TICK: Duration = Duration::from_millis(100);

#[to_layer_message(multi)]
#[derive(Debug, Clone)]
pub enum Message {
    Dbus(dbus_notifications::Event),
    Clicked(window::Id),
    Tick(Instant),
}

struct Surface {
    window: window::Id,
    icon: Option<PathBuf>,
}

pub struct App {
    config: Config,
    store: Store,
    conn: Option<Connection>,
    /// by notification id
    surfaces: HashMap<u32, Surface>,
    windows: HashMap<window::Id, u32>,
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
            Message::Clicked(window) => match self.windows.get(&window) {
                Some(&id) => self.close(id, CloseReason::Dismissed),
                None => Task::none(),
            },
            Message::Tick(now) => {
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
        let entry = self
            .windows
            .get(&window)
            .and_then(|id| self.store.get(*id).zip(self.surfaces.get(id)));
        match entry {
            Some((entry, surface)) => ui::notification::view(
                &entry.notification,
                surface.icon.as_deref(),
                &self.config.style,
                Message::Clicked(window),
            ),
            None => text("").into(),
        }
    }

    fn notify(&mut self, n: Notification) -> Task<Message> {
        let style = &self.config.style;
        let width = self.config.general.width;
        let icon = ui::icons::resolve(&n, style.icon_size);
        let height = ui::notification::height(&n, icon.is_some(), style, width);
        let timeout = n.timeout(&self.config.urgency);
        let id = n.id;
        log::debug!("notification {id}: {width}x{height}, timeout {timeout:?}");

        match self.store.upsert(n, height, timeout, Instant::now()) {
            Upsert::Added => {
                let margin = self.margin_of(id);
                let (window, open) = Message::layershell_open(NewLayerShellSettings {
                    size: Some((width, height)),
                    layer: Layer::Overlay,
                    anchor: layer_anchor(self.config.general.anchor),
                    exclusive_zone: None,
                    margin,
                    keyboard_interactivity: KeyboardInteractivity::None,
                    output_option: OutputOption::Active,
                    events_transparent: false,
                    namespace: Some(NAMESPACE.to_string()),
                });
                self.windows.insert(window, id);
                self.surfaces.insert(id, Surface { window, icon });
                open
            }
            Upsert::Replaced => {
                let surface = self
                    .surfaces
                    .get_mut(&id)
                    .expect("surface of stored notification");
                surface.icon = icon;
                let resize = Task::done(Message::SizeChange {
                    id: surface.window,
                    size: (width, height),
                });
                Task::batch([resize, self.reflow()])
            }
        }
    }

    fn close(&mut self, id: u32, reason: CloseReason) -> Task<Message> {
        if self.store.remove(id).is_none() {
            return Task::none();
        }
        log::debug!("close {id}: {reason:?}");
        let mut tasks = vec![self.reflow()];
        if let Some(surface) = self.surfaces.remove(&id) {
            self.windows.remove(&surface.window);
            tasks.push(Task::done(Message::RemoveWindow(surface.window)));
        }
        if let Some(conn) = self.conn.clone() {
            tasks.push(Task::future(emit_closed(conn, id, reason)).discard());
        }
        Task::batch(tasks)
    }

    fn margin_of(&self, id: u32) -> Option<(i32, i32, i32, i32)> {
        self.store
            .margins(&self.config.general)
            .into_iter()
            .find_map(|(i, m)| (i == id).then_some(m))
    }

    /// Moves every surface to its current place in the stack.
    fn reflow(&self) -> Task<Message> {
        Task::batch(
            self.store
                .margins(&self.config.general)
                .into_iter()
                .filter_map(|(id, margin)| {
                    let window = self.surfaces.get(&id)?.window;
                    Some(Task::done(Message::MarginChange { id: window, margin }))
                }),
        )
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
