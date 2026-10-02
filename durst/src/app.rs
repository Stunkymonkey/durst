//! The iced_layershell daemon: owns all surfaces and turns events into
//! changes of the [`Store`]. After every event, [`App::sync`] reconciles the
//! open surfaces with the store.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use durst_proto::control::{DaemonInfo, MediaInfo, VolumeInfo};
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

use crate::audio::{self, Target, Volume};
use crate::config::{self, Anchor, Config, General, MouseAction, Output, Style};
use crate::config_watch;
use crate::core::history::History;
use crate::core::layout::Margin;
use crate::core::media::{Players, Status as PlayStatus};
use crate::core::notification::{Notification, Urgency};
use crate::core::rules::{self, Context, Fullscreen, IconPosition, Outcome, ScriptOn};
use crate::core::store::{Insert, Store};
use crate::dbus::control::{self, Command, ModeChange, Reply, Status, emit_status_changed};
use crate::dbus::notifications::{emit_action_invoked, emit_closed};
use crate::dbus::{self, Event};
use crate::effects;
use crate::logind;
use crate::mpris;
use crate::ui::cover;
use crate::ui::icons::{self, Icon};
use crate::ui::markup::{self, Run};
use crate::ui::notification::{self as ui, Content};
use crate::ui::osd;
use crate::wayland;

const NAMESPACE: &str = "durst";
/// how often expiry is checked while a notification has a running timer
const TICK: Duration = Duration::from_millis(100);
/// at most this often a dragged volume slider sets the volume
const SLIDER_RATE: Duration = Duration::from_millis(30);

#[to_layer_message(multi)]
#[derive(Debug, Clone)]
pub enum Message {
    Dbus(Event),
    Surface(window::Id, ui::Event),
    /// the surface exists now; changes sent before this are lost
    Opened(window::Id),
    Tick(Instant),
    Wayland(wayland::Event),
    Locked(bool),
    /// the config file changed on disk
    ConfigChanged,
    Audio(audio::Event),
    Osd(osd::Event),
    /// send the volume of a dragged slider
    FlushVolume,
    Mpris(mpris::Event),
    MediaOsd(osd::MediaEvent),
    /// a cover finished loading (`None`: failed)
    Cover(String, Option<iced::widget::image::Handle>),
    ConfigError(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Key {
    Notification(u32),
    /// the "+N more" indicator after the stack
    More,
    /// the volume OSD, separate from the stack
    VolumeOsd,
    MediaOsd,
}

/// One surface of a [`Key`]; with `output = "all"` there is one per output.
type SurfaceKey = (Key, Option<String>);

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

/// A notification as received, what the rules made of it, and its parsed
/// content, prepared once instead of every frame.
struct Prepared {
    /// as sent, before the rules; they run again when modes change
    raw: Notification,
    /// false for notifications popped from the history: shown as they were
    rules: bool,
    /// arrived during fullscreen with `fullscreen = "delay"`: held until it ends
    delayed: bool,
    outcome: Outcome,
    style: Style,
    body: Vec<Run>,
    icon: Option<Icon>,
}

/// The volume OSD while it is shown.
struct VolumeOsd {
    target: Target,
    /// hidden after this, unless hovered or dragged; `None`: stays
    until: Option<Instant>,
    hovered: bool,
    /// the slider is dragged to this percent
    drag: Option<u32>,
    /// dragged to, not sent yet
    pending: Option<u32>,
    last_sent: Option<Instant>,
}

/// The media OSD while it is shown.
struct MediaOsd {
    /// hidden after this, unless hovered; `None`: stays
    until: Option<Instant>,
    hovered: bool,
}

/// Covers kept in memory, by URL; cleared when there are more.
const COVER_CACHE: usize = 32;

/// Where the config comes from, for reloading.
#[derive(Debug, Clone)]
pub struct ConfigSource {
    pub path: PathBuf,
    /// given with `-c`: a missing file is an error
    pub explicit: bool,
    /// the text of the active config; `None` = defaults
    pub text: Option<String>,
    /// the config could not be loaded at startup, the defaults are active
    pub error: Option<String>,
}

/// The id of durst's own notification about an invalid config; far away
/// from the ids handed out to senders, which count up from 1.
const CONFIG_ERROR_ID: u32 = u32::MAX;

pub struct App {
    config: Config,
    source: ConfigSource,
    store: Store,
    history: History,
    modes: BTreeSet<String>,
    /// no input for `idle_threshold`
    idle: bool,
    locked: bool,
    /// a focused window is fullscreen
    fullscreen: bool,
    outputs: Vec<String>,
    audio: Option<audio::Handle>,
    volumes: HashMap<Target, Option<Volume>>,
    osd: Option<VolumeOsd>,
    mpris: Option<mpris::Handle>,
    players: Players,
    media_osd: Option<MediaOsd>,
    /// loaded covers by URL (`None`: failed); loading ones are absent
    covers: HashMap<String, Option<iced::widget::image::Handle>>,
    covers_loading: HashSet<String>,
    conn: Option<Connection>,
    /// state shared with the control interface, and the last published one
    status: Option<Arc<Mutex<Status>>>,
    published: Status,
    prepared: HashMap<u32, Prepared>,
    surfaces: HashMap<SurfaceKey, Surface>,
    windows: HashMap<window::Id, Key>,
    /// surfaces closed before they were opened, removed once they are
    orphans: HashSet<window::Id>,
}

pub fn run(config: Config, source: ConfigSource) -> Result<(), iced_layershell::Error> {
    daemon(
        move || {
            let task = match &source.error {
                Some(e) => Task::done(Message::ConfigError(e.clone())),
                None => Task::none(),
            };
            (App::new(config.clone(), source.clone()), task)
        },
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
            modes: BTreeSet::new(),
            idle: false,
            locked: false,
            fullscreen: false,
            outputs: Vec::new(),
            audio: None,
            volumes: HashMap::new(),
            osd: None,
            mpris: None,
            players: Players::default(),
            media_osd: None,
            covers: HashMap::new(),
            covers_loading: HashSet::new(),
            conn: None,
            status: None,
            published: Status::default(),
            prepared: HashMap::new(),
            surfaces: HashMap::new(),
            windows: HashMap::new(),
            orphans: HashSet::new(),
        }
    }

    fn subscription(&self) -> Subscription<Message> {
        let dbus = Subscription::run(dbus_stream).map(Message::Dbus);
        let opened = window::open_events().map(Message::Opened);
        let wayland =
            wayland::subscription(self.config.general.idle_threshold.0).map(Message::Wayland);
        let locked = logind::subscription().map(Message::Locked);
        let config =
            config_watch::subscription(self.source.path.clone()).map(|()| Message::ConfigChanged);
        let audio = audio::subscription().map(Message::Audio);
        let media = mpris::subscription().map(Message::Mpris);
        let tick = (self.store.next_deadline().is_some()
            || self.osd.is_some()
            || self.media_osd.is_some())
        .then(|| iced::time::every(TICK).map(Message::Tick));
        let flush = self
            .osd
            .as_ref()
            .and_then(|o| o.pending)
            .map(|_| iced::time::every(SLIDER_RATE).map(|_| Message::FlushVolume));
        Subscription::batch(
            [dbus, opened, wayland, locked, config, audio, media]
                .into_iter()
                .chain(tick)
                .chain(flush),
        )
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
            Message::Dbus(Event::Notify(n)) => self.receive(*n, true),
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
            Message::Wayland(wayland::Event::Idle(idle)) => {
                log::debug!("idle: {idle}");
                self.idle = idle;
                Task::none()
            }
            Message::Wayland(wayland::Event::Fullscreen(fullscreen)) => {
                log::debug!("fullscreen: {fullscreen}");
                self.fullscreen = fullscreen;
                self.reevaluate()
            }
            Message::Wayland(wayland::Event::Outputs(outputs)) => {
                log::debug!("outputs: {outputs:?}");
                self.outputs = outputs;
                Task::none()
            }
            Message::ConfigChanged => {
                log::debug!("config file changed");
                self.reload(false).unwrap_or_else(|(_, task)| task)
            }
            Message::ConfigError(e) => self.show_config_error(&e, "durst uses the defaults"),
            Message::Locked(locked) => {
                log::debug!("locked: {locked}");
                self.locked = locked;
                Task::none()
            }
            Message::Audio(audio::Event::Ready(handle)) => {
                log::info!("volume control connected to PipeWire");
                self.audio = Some(handle);
                Task::none()
            }
            Message::Audio(audio::Event::Changed(target, volume)) => {
                log::debug!("volume {target:?}: {volume:?}");
                let previous = self.volumes.insert(target, volume.clone());
                // a change of the same device, not the first value or a switch
                let changed = match (previous.flatten(), &volume) {
                    (Some(old), Some(new)) => old.description == new.description && old != *new,
                    _ => false,
                };
                if changed && self.config.osd.volume.show_on_external_change {
                    self.show_osd(target);
                }
                Task::none()
            }
            Message::Osd(event) => {
                self.osd_event(event);
                Task::none()
            }
            Message::FlushVolume => {
                self.flush_volume(true);
                Task::none()
            }
            Message::Mpris(mpris::Event::Ready(handle)) => {
                self.mpris = Some(handle);
                Task::none()
            }
            Message::Mpris(mpris::Event::Player {
                name,
                identity,
                status,
                track,
                initial,
            }) => {
                let changed = self.players.update(&name, identity, status, track);
                // players found at startup only set the scene
                if changed && !initial && self.config.osd.media.show_on_track_change {
                    self.show_media_osd();
                }
                self.load_cover()
            }
            Message::Mpris(mpris::Event::Gone(name)) => {
                self.players.remove(&name);
                if self.players.active().is_none() {
                    self.media_osd = None;
                }
                self.load_cover()
            }
            Message::Cover(url, handle) => {
                self.covers_loading.remove(&url);
                if self.covers.len() >= COVER_CACHE {
                    self.covers.clear();
                }
                self.covers.insert(url, handle);
                Task::none()
            }
            Message::MediaOsd(event) => {
                match event {
                    osd::MediaEvent::Previous => self.media_action(mpris::Action::Previous),
                    osd::MediaEvent::PlayPause => self.media_action(mpris::Action::PlayPause),
                    osd::MediaEvent::Next => self.media_action(mpris::Action::Next),
                    osd::MediaEvent::Hover(hovered) => {
                        if let Some(osd) = &mut self.media_osd {
                            osd.hovered = hovered;
                        }
                        if !hovered {
                            self.show_media_osd();
                        }
                    }
                }
                Task::none()
            }
            Message::Tick(now) => {
                let hide = self.osd.as_ref().is_some_and(|o| {
                    !o.hovered && o.drag.is_none() && o.until.is_some_and(|t| t <= now)
                });
                if hide {
                    self.osd = None;
                }
                let hide_media = self
                    .media_osd
                    .as_ref()
                    .is_some_and(|o| !o.hovered && o.until.is_some_and(|t| t <= now));
                if hide_media {
                    self.media_osd = None;
                }
                self.store
                    .update_timers(&self.config.general, now, self.idle || self.locked);
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
                let (Some(entry), Some(prepared)) = (self.store.get(*id), self.prepared.get(id))
                else {
                    return text("").into();
                };
                ui::view(
                    content(&entry.notification, entry.count, prepared),
                    &prepared.style,
                )
            }
            Some(Key::More) => ui::more_view(
                self.store.waiting(&self.config.general),
                self.config.style(Urgency::Normal),
            ),
            Some(Key::MediaOsd) => {
                let Some(content) = self.media_content() else {
                    return text("").into();
                };
                return osd::media_view(content, self.config.style(Urgency::Normal))
                    .map(Message::MediaOsd);
            }
            Some(Key::VolumeOsd) => {
                let Some(content) = self.osd_content() else {
                    return text("").into();
                };
                return osd::volume_view(content, self.config.style(Urgency::Normal))
                    .map(Message::Osd);
            }
            None => return text("").into(),
        };
        element.map(move |event| Message::Surface(window, event))
    }

    /// A notification arrived (or came back from the history, then without
    /// rules and side effects).
    fn receive(&mut self, raw: Notification, apply_rules: bool) -> Task<Message> {
        let (n, outcome) = self.apply_rules(&raw, apply_rules);
        let id = n.id;
        if !outcome.matched.is_empty() {
            log::debug!("notification {id}: rules {:?}", outcome.matched);
        }
        let delayed = self.fullscreen && outcome.fullscreen == Fullscreen::Delay;
        let held = self.held(&outcome, delayed);
        // arrival side effects, but not for held ones (quiet during e.g. dnd)
        if apply_rules && !held {
            self.play_sound(&n, &outcome);
            run_scripts(&outcome, &n, ScriptOn::Receive, None);
        }

        if outcome.skip_display {
            // replaces a displayed one: that one is gone now
            if self.store.remove(id).is_some() {
                self.prepared.remove(&id);
            }
            if !outcome.history_ignore {
                self.history.push(n);
            }
            return self.emit_closed(id, CloseReason::Undefined);
        }

        let timeout = match outcome.timeout {
            Some(timeout) => timeout.0,
            None => n.timeout(
                &self.config.urgency,
                self.config.general.ignore_dbus_timeout,
            ),
        };
        let auto_invoke = outcome.auto_invoke.clone();
        let prepared = self.prepare(raw, &n, outcome, apply_rules, delayed);

        let mut tasks = Vec::new();
        match self.store.insert(n, 0, timeout, held, &self.config.general) {
            Insert::Added | Insert::Replaced => {}
            Insert::Superseded(old) => {
                // keep the surface, it now shows the new notification
                let moved: Vec<SurfaceKey> = self
                    .surfaces
                    .keys()
                    .filter(|(key, _)| *key == Key::Notification(old))
                    .cloned()
                    .collect();
                for skey in moved {
                    let surface = self.surfaces.remove(&skey).expect("listed");
                    self.windows.insert(surface.window, Key::Notification(id));
                    self.surfaces
                        .insert((Key::Notification(id), skey.1), surface);
                }
                self.prepared.remove(&old);
                tasks.push(self.emit_closed(old, CloseReason::Undefined));
            }
        }
        // the height depends on the duplicate counter, known only now
        let entry = self.store.get(id).expect("just inserted");
        let height = ui::height(
            &content(&entry.notification, entry.count, &prepared),
            &prepared.style,
            self.config.general.width,
        );
        log::debug!("notification {id}: height {height}, timeout {timeout:?}, held {held}");
        self.store.set_height(id, height);
        self.prepared.insert(id, prepared);

        if let Some(key) = auto_invoke.filter(|_| apply_rules && !held) {
            if self.has_action(id, &key) {
                tasks.push(self.invoke(id, &key));
                tasks.push(self.close_after_action(id));
            } else {
                log::warn!("auto_invoke: notification {id} has no action {key:?}");
            }
        }
        Task::batch(tasks)
    }

    fn apply_rules(&self, raw: &Notification, apply: bool) -> (Notification, Outcome) {
        let mut n = raw.clone();
        let outcome = if apply {
            let ctx = Context {
                modes: self.modes.clone(),
                fullscreen: self.fullscreen,
            };
            rules::apply(&self.config.rules, &mut n, &ctx)
        } else {
            Outcome::default()
        };
        (n, outcome)
    }

    /// Held back: by `defer`, or by the fullscreen policy.
    fn held(&self, outcome: &Outcome, delayed: bool) -> bool {
        outcome.defer || delayed || (self.fullscreen && outcome.fullscreen == Fullscreen::Pushback)
    }

    /// Style, body and icon of a notification as the rules left it.
    fn prepare(
        &self,
        raw: Notification,
        n: &Notification,
        outcome: Outcome,
        rules: bool,
        delayed: bool,
    ) -> Prepared {
        let urgency = n.hints.urgency;
        let style = self
            .config
            .style_with(urgency, &outcome.style)
            .unwrap_or_else(|e| {
                log::warn!("rule style: {e}");
                self.config.style(urgency).clone()
            });
        let theme = self.config.general.icon_theme.as_deref();
        let icon = match outcome.icon_position {
            IconPosition::Off => None,
            _ => icons::resolve(n, style.icon_size, theme).or_else(|| {
                let name = outcome.default_icon.as_deref()?;
                icons::resolve_name(name, style.icon_size, theme)
            }),
        };
        Prepared {
            raw,
            rules,
            delayed,
            body: markup::parse(&n.body),
            icon,
            style,
            outcome,
        }
    }

    /// Runs the rules of all notifications again, after the modes or the
    /// config changed: they may be held or released, restyled, rewritten.
    fn reevaluate(&mut self) -> Task<Message> {
        let ids: Vec<u32> = self.store.iter().map(|e| e.notification.id).collect();
        let mut tasks = Vec::new();
        for id in ids {
            let Some(old) = self.prepared.remove(&id) else {
                continue;
            };
            let (n, outcome) = self.apply_rules(&old.raw, old.rules);
            if outcome.skip_display {
                self.prepared.insert(id, Prepared { outcome, ..old });
                tasks.push(self.close(id, CloseReason::Undefined));
                continue;
            }
            // delayed ones are released when fullscreen ends
            let delayed = old.delayed && self.fullscreen;
            let held = self.held(&outcome, delayed);
            let prepared = self.prepare(old.raw, &n, outcome, old.rules, delayed);
            let count = self.store.get(id).map_or(1, |e| e.count);
            let height = ui::height(
                &content(&n, count, &prepared),
                &prepared.style,
                self.config.general.width,
            );
            self.store
                .refresh(id, n, height, held, &self.config.general);
            self.prepared.insert(id, prepared);
        }
        Task::batch(tasks)
    }

    fn close(&mut self, id: u32, reason: CloseReason) -> Task<Message> {
        let Some(entry) = self.store.remove(id) else {
            return Task::none();
        };
        log::debug!("close {id}: {reason:?}");
        let prepared = self.prepared.remove(&id);
        if let Some(p) = &prepared {
            run_scripts(
                &p.outcome,
                &entry.notification,
                ScriptOn::Close,
                Some(reason_name(reason)),
            );
        }
        // the user may want these back; the sender closed the others itself
        let ignore = prepared.is_some_and(|p| p.outcome.history_ignore);
        if matches!(reason, CloseReason::Expired | CloseReason::Dismissed) && !ignore {
            self.history.push(entry.notification);
        }
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
                    if let Some(key) = self.default_action(id) {
                        tasks.push(self.invoke(id, &key));
                        invoked = true;
                    }
                }
                MouseAction::OpenUrl => {
                    let url = self
                        .prepared
                        .get(&id)
                        .and_then(|p| markup::first_link(&p.body))
                        .map(str::to_owned);
                    if let Some(url) = url {
                        self.open_url(&url);
                    }
                }
            }
        }
        Task::batch(tasks)
    }

    fn has_action(&self, id: u32, key: &str) -> bool {
        self.store
            .get(id)
            .is_some_and(|e| e.notification.actions.iter().any(|(k, _)| k == key))
    }

    /// The action of a click: a rule's `default_action`, else "default" or
    /// the only action there is.
    fn default_action(&self, id: u32) -> Option<String> {
        let from_rule = self
            .prepared
            .get(&id)
            .and_then(|p| p.outcome.default_action.clone())
            .filter(|key| self.has_action(id, key));
        from_rule.or_else(|| default_action(&self.store.get(id)?.notification))
    }

    fn invoke(&self, id: u32, key: &str) -> Task<Message> {
        log::debug!("invoke action {key:?} of {id}");
        if let (Some(p), Some(e)) = (self.prepared.get(&id), self.store.get(id)) {
            run_scripts(&p.outcome, &e.notification, ScriptOn::Action, Some(key));
        }
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
        log::debug!("open {url}");
        effects::spawn(&self.config.general.browser, &[url], &[]);
    }

    /// A rule's sound wins; otherwise the sender's sound hints, unless
    /// suppressed. `mute_sound` silences both.
    fn play_sound(&self, n: &Notification, outcome: &Outcome) {
        if outcome.mute_sound {
            return;
        }
        let hint = || {
            let h = &n.hints;
            (self.config.sound.play_hints && !h.suppress_sound)
                .then(|| h.sound_file.clone().or_else(|| h.sound_name.clone()))
                .flatten()
        };
        if let Some(sound) = outcome.sound.clone().or_else(hint) {
            effects::play_sound(&self.config.sound, &sound);
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
        self.store
            .update_timers(general, now, self.idle || self.locked);

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
        // the outputs each key is shown on
        let outputs: Vec<Option<String>> = match &general.output {
            Output::All => self.outputs.iter().cloned().map(Some).collect(),
            _ => vec![None],
        };
        let mut placed: Vec<(Key, (u32, u32), Margin, Anchor)> = wanted
            .into_iter()
            .zip(margins)
            .map(|((key, height), margin)| (key, (general.width, height), margin, general.anchor))
            .collect();
        if let Some(content) = self.osd_content() {
            let osd = &self.config.osd.volume;
            let height = osd::volume_height(&content, self.config.style(Urgency::Normal));
            let place = General {
                anchor: osd.anchor,
                offset: osd.offset,
                ..general.clone()
            };
            let margin = crate::core::layout::stack_margins(&place, &[height])[0];
            placed.push((Key::VolumeOsd, (osd.width, height), margin, osd.anchor));
        }
        if let Some(content) = self.media_content() {
            let osd = &self.config.osd.media;
            let height = osd::media_height(&content, self.config.style(Urgency::Normal));
            let place = General {
                anchor: osd.anchor,
                offset: osd.offset,
                ..general.clone()
            };
            let margin = crate::core::layout::stack_margins(&place, &[height])[0];
            placed.push((Key::MediaOsd, (osd.width, height), margin, osd.anchor));
        }
        let wanted: Vec<(SurfaceKey, (u32, u32), Margin, Anchor)> = placed
            .into_iter()
            .flat_map(|(key, size, margin, anchor)| {
                outputs
                    .iter()
                    .map(move |output| ((key, output.clone()), size, margin, anchor))
            })
            .collect();
        let gone: Vec<SurfaceKey> = self
            .surfaces
            .keys()
            .filter(|skey| !wanted.iter().any(|(k, ..)| k == *skey))
            .cloned()
            .collect();
        for skey in gone {
            tasks.push(self.remove_surface(&skey));
        }

        for (skey, size, margin, anchor) in wanted {
            match self.surfaces.get_mut(&skey) {
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
                        anchor: layer_anchor(anchor),
                        exclusive_zone: None,
                        margin: Some(margin),
                        keyboard_interactivity: KeyboardInteractivity::None,
                        output_option: match (&general.output, &skey.1) {
                            (_, Some(name)) | (Output::Name(name), None) => {
                                OutputOption::OutputName(name.clone())
                            }
                            _ => OutputOption::LastOutput,
                        },
                        events_transparent: false,
                        namespace: Some(NAMESPACE.to_string()),
                    });
                    self.windows.insert(window, skey.0);
                    self.surfaces.insert(
                        skey,
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

    fn status(&self) -> Status {
        let general = &self.config.general;
        Status {
            displayed: self.store.visible(general).len() as u32,
            waiting: self.store.waiting(general) as u32,
            held: self.store.held() as u32,
            history: self.history.len() as u32,
            modes: self.modes.iter().cloned().collect(),
        }
    }

    fn publish_status(&mut self) -> Task<Message> {
        let status = self.status();
        let (Some(shared), Some(conn)) = (&self.status, &self.conn) else {
            return Task::none();
        };
        if status == self.published {
            return Task::none();
        }
        *shared.lock().unwrap() = status.clone();
        let old = std::mem::replace(&mut self.published, status.clone());
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
                        let state = match (i < visible, e.held) {
                            (_, true) => "held",
                            (true, _) => "displayed",
                            (false, _) => "waiting",
                        };
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
                let key = match key.as_str() {
                    "" => self.default_action(id),
                    key => self.has_action(id, key).then(|| key.to_owned()),
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
                    (Reply::Id(id), self.receive(n, false))
                }
                None => (Reply::NotFound("the history is empty".into()), Task::none()),
            },
            Command::HistoryClear => {
                self.history.clear();
                (Reply::Done, Task::none())
            }
            Command::Reload => match self.reload(true) {
                Ok(task) => (Reply::Done, task),
                Err((e, task)) => (Reply::InvalidConfig(e), task),
            },
            Command::Info => {
                let status = self.status();
                let info = DaemonInfo {
                    version: env!("CARGO_PKG_VERSION").to_owned(),
                    config_path: self.source.path.display().to_string(),
                    displayed: status.displayed,
                    waiting: status.waiting,
                    held: status.held,
                    history: status.history,
                    modes: status.modes,
                    idle: self.idle,
                    locked: self.locked,
                    fullscreen: self.fullscreen,
                    outputs: self.outputs.clone(),
                };
                (Reply::Info(info), Task::none())
            }
            Command::Modes(change) => {
                match change {
                    ModeChange::Set(modes) => self.modes = modes.into_iter().collect(),
                    ModeChange::Enable(mode) => {
                        self.modes.insert(mode);
                    }
                    ModeChange::Disable(mode) => {
                        self.modes.remove(&mode);
                    }
                    ModeChange::Toggle(mode) => {
                        if !self.modes.remove(&mode) {
                            self.modes.insert(mode);
                        }
                    }
                }
                log::info!("modes: {:?}", self.modes);
                let task = self.reevaluate();
                (Reply::Modes(self.modes.iter().cloned().collect()), task)
            }
            Command::Volume(mic) => match self.volume_info(mic) {
                Ok(info) => (Reply::Volume(info), Task::none()),
                Err(reply) => (reply, Task::none()),
            },
            Command::SetVolume(mic, change) => {
                let mut info = match self.volume_info(mic) {
                    Ok(info) => info,
                    Err(reply) => return (reply, Task::none()),
                };
                let step = self.config.osd.volume.step as i64;
                let current = info.percent as i64;
                let change = change.trim().trim_end_matches('%');
                let percent = match change {
                    "up" => Some(current + step),
                    "down" => Some(current - step),
                    c if c.starts_with(['+', '-']) => c.parse::<i64>().ok().map(|d| current + d),
                    c => c.parse::<i64>().ok(),
                };
                let Some(percent) = percent else {
                    let msg = format!("invalid volume {change:?}: N, +N, -N, up or down");
                    return (Reply::InvalidArgument(msg), Task::none());
                };
                let target = target(mic);
                info.percent = self.set_volume(target, percent);
                self.show_osd(target);
                (Reply::Volume(info), Task::none())
            }
            Command::SetMute(mic, state) => {
                let mut info = match self.volume_info(mic) {
                    Ok(info) => info,
                    Err(reply) => return (reply, Task::none()),
                };
                info.muted = match state.as_str() {
                    "on" => true,
                    "off" => false,
                    "toggle" => !info.muted,
                    _ => {
                        let msg = format!("invalid mute state {state:?}: on, off or toggle");
                        return (Reply::InvalidArgument(msg), Task::none());
                    }
                };
                let target = target(mic);
                self.send_audio(audio::Command::SetMute(target, info.muted));
                self.show_osd(target);
                (Reply::Volume(info), Task::none())
            }
            Command::ShowVolumeOsd(mic) => match self.volume_info(mic) {
                Ok(_) => {
                    self.show_osd(target(mic));
                    (Reply::Done, Task::none())
                }
                Err(reply) => (reply, Task::none()),
            },
            Command::MediaStatus => match self.media_info() {
                Ok(info) => (Reply::Media(info), Task::none()),
                Err(reply) => (reply, Task::none()),
            },
            Command::MediaAction(action) => {
                let info = match self.media_info() {
                    Ok(info) => info,
                    Err(reply) => return (reply, Task::none()),
                };
                let action = match action.as_str() {
                    "play" => mpris::Action::Play,
                    "pause" => mpris::Action::Pause,
                    "toggle" => mpris::Action::PlayPause,
                    "next" => mpris::Action::Next,
                    "prev" | "previous" => mpris::Action::Previous,
                    _ => {
                        let msg = format!(
                            "invalid media action {action:?}: play, pause, toggle, next or prev"
                        );
                        return (Reply::InvalidArgument(msg), Task::none());
                    }
                };
                self.media_action(action);
                (Reply::Media(info), Task::none())
            }
            Command::ShowMediaOsd => match self.media_info() {
                Ok(_) => {
                    self.show_media_osd();
                    (Reply::Done, Task::none())
                }
                Err(reply) => (reply, Task::none()),
            },
            Command::KnownModes => {
                let known: BTreeSet<String> = self
                    .config
                    .rules
                    .iter()
                    .flat_map(|r| r.modes())
                    .map(str::to_owned)
                    .collect();
                (Reply::Modes(known.into_iter().collect()), Task::none())
            }
        }
    }

    fn media_info(&self) -> Result<MediaInfo, Reply> {
        let player = self
            .players
            .active()
            .ok_or_else(|| Reply::NotFound("no media player".into()))?;
        let track = &player.track;
        Ok(MediaInfo {
            player: player.identity.clone(),
            status: player.status.name().into(),
            title: track.title.clone(),
            artist: track.artist.clone(),
            album: track.album.clone(),
        })
    }

    fn media_content(&self) -> Option<osd::MediaContent<'_>> {
        self.media_osd.as_ref()?;
        let player = self.players.active()?;
        let track = &player.track;
        Some(osd::MediaContent {
            title: if track.title.is_empty() {
                &player.identity
            } else {
                &track.title
            },
            artist: &track.artist,
            playing: player.status == PlayStatus::Playing,
            cover: track
                .art_url
                .as_ref()
                .and_then(|url| self.covers.get(url))
                .and_then(Option::as_ref),
            cover_size: self.config.osd.media.cover_size,
        })
    }

    fn show_media_osd(&mut self) {
        let config = &self.config.osd.media;
        if !config.enabled || self.players.active().is_none() {
            return;
        }
        let until = config.timeout.0.map(|t| Instant::now() + t);
        let hovered = self.media_osd.as_ref().is_some_and(|o| o.hovered);
        self.media_osd = Some(MediaOsd { until, hovered });
    }

    fn media_action(&mut self, action: mpris::Action) {
        let (Some(player), Some(mpris)) = (self.players.active(), &self.mpris) else {
            return;
        };
        mpris.send(&player.name, action);
        self.show_media_osd();
    }

    /// Starts loading the active track's cover unless it is known already.
    fn load_cover(&mut self) -> Task<Message> {
        let Some(url) = self.players.active().and_then(|p| p.track.art_url.clone()) else {
            return Task::none();
        };
        if self.covers.contains_key(&url) || !self.covers_loading.insert(url.clone()) {
            return Task::none();
        }
        let size = self.config.osd.media.cover_size;
        Task::perform(cover::load(url.clone(), size), move |handle| {
            Message::Cover(url.clone(), handle)
        })
    }

    fn volume_info(&self, mic: bool) -> Result<VolumeInfo, Reply> {
        let name = if mic { "source" } else { "sink" };
        if self.audio.is_none() {
            return Err(Reply::NotFound("no connection to PipeWire".into()));
        }
        match self.volumes.get(&target(mic)).cloned().flatten() {
            Some(v) => Ok(VolumeInfo {
                percent: v.percent,
                muted: v.muted,
                description: v.description,
            }),
            None => Err(Reply::NotFound(format!("no default {name}"))),
        }
    }

    fn osd_content(&self) -> Option<osd::VolumeContent<'_>> {
        let state = self.osd.as_ref()?;
        let volume = self.volumes.get(&state.target)?.as_ref()?;
        Some(osd::VolumeContent {
            title: &volume.description,
            percent: state.drag.unwrap_or(volume.percent),
            muted: volume.muted,
            max: self.config.osd.volume.max_volume,
        })
    }

    fn show_osd(&mut self, target: Target) {
        let config = &self.config.osd.volume;
        if !config.enabled {
            return;
        }
        let until = config.timeout.0.map(|t| Instant::now() + t);
        match &mut self.osd {
            Some(osd) if osd.target == target => osd.until = until,
            _ => {
                self.osd = Some(VolumeOsd {
                    target,
                    until,
                    hovered: false,
                    drag: None,
                    pending: None,
                    last_sent: None,
                })
            }
        }
    }

    fn osd_event(&mut self, event: osd::Event) {
        let Some(target) = self.osd.as_ref().map(|o| o.target) else {
            return;
        };
        match event {
            osd::Event::Slide(value) => {
                if let Some(osd) = &mut self.osd {
                    osd.drag = Some(value.round() as u32);
                    osd.pending = osd.drag;
                }
                self.flush_volume(false);
            }
            osd::Event::Release => {
                self.flush_volume(true);
                if let Some(osd) = &mut self.osd {
                    osd.drag = None;
                }
                self.show_osd(target);
            }
            osd::Event::Scroll(y) => {
                let step = self.config.osd.volume.step as i64;
                let delta = if y > 0.0 { step } else { -step };
                if let Some(current) = self.volumes.get(&target).cloned().flatten() {
                    self.set_volume(target, current.percent as i64 + delta);
                }
                self.show_osd(target);
            }
            osd::Event::ToggleMute => {
                if let Some(current) = self.volumes.get(&target).cloned().flatten() {
                    self.send_audio(audio::Command::SetMute(target, !current.muted));
                }
                self.show_osd(target);
            }
            osd::Event::Hover(hovered) => {
                if let Some(osd) = &mut self.osd {
                    osd.hovered = hovered;
                }
                if !hovered {
                    // the timeout starts again when the pointer leaves
                    self.show_osd(target);
                }
            }
        }
    }

    /// Sends the dragged volume, unless one was sent less than
    /// `SLIDER_RATE` ago (and `force` isn't set): dragging produces far more
    /// values than PipeWire needs.
    fn flush_volume(&mut self, force: bool) {
        let now = Instant::now();
        let Some(osd) = &mut self.osd else { return };
        let Some(percent) = osd.pending else { return };
        if !force && osd.last_sent.is_some_and(|t| now - t < SLIDER_RATE) {
            return;
        }
        osd.pending = None;
        osd.last_sent = Some(now);
        let target = osd.target;
        self.set_volume(target, percent as i64);
    }

    /// Sets the volume, limited to 0..=max_volume; returns the new percent.
    fn set_volume(&mut self, target: Target, percent: i64) -> u32 {
        let percent = percent.clamp(0, self.config.osd.volume.max_volume as i64) as u32;
        self.send_audio(audio::Command::SetPercent(target, percent));
        percent
    }

    fn send_audio(&self, command: audio::Command) {
        match &self.audio {
            Some(audio) => audio.send(command),
            None => log::warn!("no PipeWire connection for {command:?}"),
        }
    }

    /// Loads the config file again. Unless `force`d, an unchanged file is
    /// left alone (the surfaces would be reopened for nothing). An invalid
    /// config keeps the active one and shows the error as a notification.
    fn reload(&mut self, force: bool) -> Result<Task<Message>, (String, Task<Message>)> {
        let loaded = config::read(&self.source.path, self.source.explicit).and_then(|text| {
            if !force && text == self.source.text && self.store.get(CONFIG_ERROR_ID).is_none() {
                return Ok(None);
            }
            let config = match &text {
                Some(s) => {
                    config::parse(s).map_err(|e| format!("{}: {e}", self.source.path.display()))?
                }
                None => Config::default(),
            };
            Ok(Some((config, text)))
        });
        match loaded {
            Ok(None) => Ok(Task::none()),
            Ok(Some((config, text))) => {
                self.source.text = text;
                let resolved = self.close(CONFIG_ERROR_ID, CloseReason::Closed);
                Ok(Task::batch([self.apply_config(config), resolved]))
            }
            Err(e) => {
                log::warn!("keeping the old config: {e}");
                let task = self.show_config_error(&e, "the previous one stays active");
                Err((e, task))
            }
        }
    }

    /// durst's own critical notification about an invalid config; replaced
    /// by the next error, closed once the config is valid again.
    fn show_config_error(&mut self, error: &str, consequence: &str) -> Task<Message> {
        let escape = |s: &str| {
            s.replace('&', "&amp;")
                .replace('<', "&lt;")
                .replace('>', "&gt;")
        };
        let mut hints = crate::core::notification::Hints {
            urgency: Urgency::Critical,
            ..Default::default()
        };
        hints.transient = true;
        let n = Notification {
            id: CONFIG_ERROR_ID,
            app_name: "durst".into(),
            app_icon: "dialog-error".into(),
            summary: format!("Invalid config, {consequence}"),
            body: escape(&config::short_error(error)),
            actions: vec![],
            hints,
            expire_timeout: 0,
            received: 0,
        };
        self.receive(n, false)
    }

    /// Switches to a new config: evaluates the rules and renders every
    /// notification again, and reopens all surfaces, since anchor and output
    /// can only be set when a surface is created.
    fn apply_config(&mut self, config: Config) -> Task<Message> {
        log::info!("config reloaded");
        self.config = config;
        self.history.set_capacity(self.config.history.length);
        let reevaluate = self.reevaluate();
        let keys: Vec<SurfaceKey> = self.surfaces.keys().cloned().collect();
        Task::batch(
            keys.iter()
                .map(|key| self.remove_surface(key))
                .chain([reevaluate]),
        )
    }

    fn remove_surface(&mut self, key: &SurfaceKey) -> Task<Message> {
        let Some(surface) = self.surfaces.remove(key) else {
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

fn target(mic: bool) -> Target {
    if mic { Target::Source } else { Target::Sink }
}

fn content<'a>(n: &'a Notification, count: u32, p: &'a Prepared) -> Content<'a> {
    Content {
        notification: n,
        count,
        body: &p.body,
        icon: p.icon.as_ref(),
        icon_right: p.outcome.icon_position == IconPosition::Right,
    }
}

fn run_scripts(outcome: &Outcome, n: &Notification, on: ScriptOn, detail: Option<&str>) {
    for (when, command) in &outcome.scripts {
        if *when == on {
            effects::run_script(command, n, on, detail);
        }
    }
}

fn reason_name(reason: CloseReason) -> &'static str {
    match reason {
        CloseReason::Expired => "expired",
        CloseReason::Dismissed => "dismissed",
        CloseReason::Closed => "closed",
        CloseReason::Undefined => "undefined",
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
