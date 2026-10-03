//! durst's own Wayland connection, next to iced_layershell's: whether the
//! user is idle (ext-idle-notify-v1), whether the focused window is
//! fullscreen and which output it is on (wlr-foreign-toplevel-management),
//! the names of the outputs, and activation tokens (xdg-activation-v1).
//!
//! It runs on its own thread with a blocking event loop; events reach the UI
//! loop through a subscription. Missing protocols are logged and skipped.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

use futures::channel::mpsc::{UnboundedSender, unbounded};
use futures::channel::oneshot;
use futures::{SinkExt, StreamExt};
use iced::Subscription;
use wayland_client::backend::ObjectId;
use wayland_client::protocol::{wl_output, wl_registry, wl_seat};
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle, event_created_child};
use wayland_protocols::ext::idle_notify::v1::client::{
    ext_idle_notification_v1::{self, ExtIdleNotificationV1},
    ext_idle_notifier_v1::ExtIdleNotifierV1,
};
use wayland_protocols::xdg::activation::v1::client::{
    xdg_activation_token_v1::{self, XdgActivationTokenV1},
    xdg_activation_v1::XdgActivationV1,
};
use wayland_protocols_wlr::foreign_toplevel::v1::client::{
    zwlr_foreign_toplevel_handle_v1::{self, ZwlrForeignToplevelHandleV1},
    zwlr_foreign_toplevel_manager_v1::{self, ZwlrForeignToplevelManagerV1},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Idle(bool),
    Fullscreen(bool),
    /// the names of all outputs, sorted
    Outputs(Vec<String>),
    /// the output of the focused window; `None` without one (e.g. an empty
    /// workspace), then the compositor decides
    FocusedOutput(Option<String>),
    /// the compositor supports xdg-activation: tokens can be requested
    Activation(Activator),
}

/// Requests xdg-activation tokens, to pass to an application whose action
/// was invoked so it may focus its window.
#[derive(Clone)]
pub struct Activator {
    conn: Connection,
    activation: XdgActivationV1,
    qh: QueueHandle<State>,
}

impl std::fmt::Debug for Activator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Activator")
    }
}

impl PartialEq for Activator {
    fn eq(&self, other: &Self) -> bool {
        self.activation == other.activation
    }
}

impl Eq for Activator {}

impl Activator {
    /// A new token, `None` if the compositor doesn't answer in time.
    ///
    /// It carries no input serial: the click happened on iced_layershell's
    /// connection, whose serials are not valid on this one, and it doesn't
    /// pass them on. What a token without one does is the compositor's
    /// decision; sway marks the window urgent instead of focusing it.
    pub async fn token(self, app_id: Option<String>) -> Option<String> {
        let (tx, rx) = oneshot::channel();
        let token = self
            .activation
            .get_activation_token(&self.qh, Mutex::new(Some(tx)));
        if let Some(app_id) = app_id {
            token.set_app_id(app_id);
        }
        token.commit();
        let _ = self.conn.flush();
        tokio::time::timeout(Duration::from_millis(500), rx)
            .await
            .ok()?
            .ok()
    }
}

/// The Wayland events; restarted when `idle_threshold` changes (`None`: no
/// idle detection).
pub fn subscription(idle_threshold: Option<Duration>) -> Subscription<Event> {
    let millis = idle_threshold.map_or(0, |d| d.as_millis().min(u32::MAX as u128) as u32);
    Subscription::run_with(millis, stream)
}

fn stream(idle_millis: &u32) -> impl futures::Stream<Item = Event> + use<> {
    let idle_millis = *idle_millis;
    iced::stream::channel(16, async move |mut output| {
        let (tx, mut rx) = unbounded();
        std::thread::Builder::new()
            .name("wayland".into())
            .spawn(move || {
                if let Err(e) = run(idle_millis, tx) {
                    log::warn!("idle, fullscreen and output detection unavailable: {e}");
                }
            })
            .expect("spawn wayland thread");
        while let Some(event) = rx.next().await {
            if output.send(event).await.is_err() {
                break;
            }
        }
    })
}

#[derive(Default)]
struct Toplevel {
    activated: bool,
    fullscreen: bool,
    /// state sent before the next `done`
    pending: Option<(bool, bool)>,
    /// the outputs the window is on, in the order it entered them
    outputs: Vec<ObjectId>,
}

pub struct State {
    tx: UnboundedSender<Event>,
    conn: Connection,
    idle_millis: u32,
    seat: Option<wl_seat::WlSeat>,
    idle_notifier: Option<ExtIdleNotifierV1>,
    idle_notification: Option<ExtIdleNotificationV1>,
    /// by registry name: the output and its name once known
    outputs: HashMap<u32, (wl_output::WlOutput, Option<String>)>,
    toplevels: HashMap<ObjectId, Toplevel>,
    sent_fullscreen: bool,
    sent_outputs: Vec<String>,
    sent_focused_output: Option<String>,
}

impl State {
    fn send(&self, event: Event) {
        log::debug!("wayland: {event:?}");
        let _ = self.tx.unbounded_send(event);
    }

    /// Creates the idle notification once both seat and notifier are bound.
    fn setup_idle(&mut self, qh: &QueueHandle<Self>) {
        if self.idle_notification.is_some() || self.idle_millis == 0 {
            return;
        }
        if let (Some(seat), Some(notifier)) = (&self.seat, &self.idle_notifier) {
            self.idle_notification =
                Some(notifier.get_idle_notification(self.idle_millis, seat, qh, ()));
        }
    }

    fn update_fullscreen(&mut self) {
        let fullscreen = self.toplevels.values().any(|t| t.activated && t.fullscreen);
        if fullscreen != self.sent_fullscreen {
            self.sent_fullscreen = fullscreen;
            self.send(Event::Fullscreen(fullscreen));
        }
        self.update_focused_output();
    }

    fn update_focused_output(&mut self) {
        let focused = self.toplevels.values().find(|t| t.activated).and_then(|t| {
            t.outputs.iter().find_map(|id| {
                self.outputs
                    .values()
                    .find(|(output, _)| &output.id() == id)
                    .and_then(|(_, name)| name.clone())
            })
        });
        if focused != self.sent_focused_output {
            self.sent_focused_output = focused.clone();
            self.send(Event::FocusedOutput(focused));
        }
    }

    fn update_outputs(&mut self) {
        let mut names: Vec<String> = self
            .outputs
            .values()
            .filter_map(|(_, name)| name.clone())
            .collect();
        names.sort();
        if names != self.sent_outputs {
            self.sent_outputs = names.clone();
            self.send(Event::Outputs(names));
        }
        self.update_focused_output();
    }
}

fn run(idle_millis: u32, tx: UnboundedSender<Event>) -> Result<(), String> {
    let conn = Connection::connect_to_env().map_err(|e| e.to_string())?;
    let mut queue = conn.new_event_queue();
    let qh = queue.handle();
    conn.display().get_registry(&qh, ());
    let mut state = State {
        tx,
        conn: conn.clone(),
        idle_millis,
        seat: None,
        idle_notifier: None,
        idle_notification: None,
        outputs: HashMap::new(),
        toplevels: HashMap::new(),
        sent_fullscreen: false,
        sent_outputs: Vec::new(),
        sent_focused_output: None,
    };
    queue.roundtrip(&mut state).map_err(|e| e.to_string())?;
    if state.idle_notifier.is_none() && idle_millis > 0 {
        log::warn!("the compositor has no ext-idle-notify-v1: timeouts don't pause when idle");
    }
    // the subscription is gone (e.g. restarted with another threshold)
    while !state.tx.is_closed() {
        queue
            .blocking_dispatch(&mut state)
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

impl Dispatch<wl_registry::WlRegistry, ()> for State {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        match event {
            wl_registry::Event::Global {
                name,
                interface,
                version,
            } => match interface.as_str() {
                "wl_seat" if state.seat.is_none() => {
                    state.seat = Some(registry.bind(name, version.min(7), qh, ()));
                    state.setup_idle(qh);
                }
                "ext_idle_notifier_v1" => {
                    state.idle_notifier = Some(registry.bind(name, 1, qh, ()));
                    state.setup_idle(qh);
                }
                "zwlr_foreign_toplevel_manager_v1" => {
                    registry.bind::<ZwlrForeignToplevelManagerV1, _, _>(
                        name,
                        version.min(3),
                        qh,
                        (),
                    );
                }
                "xdg_activation_v1" => {
                    let activation = registry.bind(name, 1, qh, ());
                    state.send(Event::Activation(Activator {
                        conn: state.conn.clone(),
                        activation,
                        qh: qh.clone(),
                    }));
                }
                // version 4 announces the output's name
                "wl_output" if version >= 4 => {
                    let output = registry.bind(name, 4, qh, name);
                    state.outputs.insert(name, (output, None));
                }
                _ => {}
            },
            wl_registry::Event::GlobalRemove { name } => {
                if let Some((output, _)) = state.outputs.remove(&name) {
                    output.release();
                    state.update_outputs();
                }
            }
            _ => {}
        }
    }
}

impl Dispatch<wl_output::WlOutput, u32> for State {
    fn event(
        state: &mut Self,
        _: &wl_output::WlOutput,
        event: wl_output::Event,
        global: &u32,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wl_output::Event::Name { name } => {
                if let Some(entry) = state.outputs.get_mut(global) {
                    entry.1 = Some(name);
                }
            }
            wl_output::Event::Done => state.update_outputs(),
            _ => {}
        }
    }
}

impl Dispatch<ExtIdleNotificationV1, ()> for State {
    fn event(
        state: &mut Self,
        _: &ExtIdleNotificationV1,
        event: ext_idle_notification_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            ext_idle_notification_v1::Event::Idled => state.send(Event::Idle(true)),
            ext_idle_notification_v1::Event::Resumed => state.send(Event::Idle(false)),
            _ => {}
        }
    }
}

impl Dispatch<ZwlrForeignToplevelManagerV1, ()> for State {
    fn event(
        _: &mut Self,
        _: &ZwlrForeignToplevelManagerV1,
        _: zwlr_foreign_toplevel_manager_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        // new toplevels arrive as handles, see event_created_child
    }

    event_created_child!(State, ZwlrForeignToplevelManagerV1, [
        zwlr_foreign_toplevel_manager_v1::EVT_TOPLEVEL_OPCODE => (ZwlrForeignToplevelHandleV1, ()),
    ]);
}

impl Dispatch<ZwlrForeignToplevelHandleV1, ()> for State {
    fn event(
        state: &mut Self,
        handle: &ZwlrForeignToplevelHandleV1,
        event: zwlr_foreign_toplevel_handle_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        use zwlr_foreign_toplevel_handle_v1::{Event as E, State as S};
        let toplevel = state.toplevels.entry(handle.id()).or_default();
        match event {
            E::State { state: raw } => {
                let states: Vec<u32> = raw
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .map(|b| u32::from_ne_bytes(*b))
                    .collect();
                toplevel.pending = Some((
                    states.contains(&(S::Activated as u32)),
                    states.contains(&(S::Fullscreen as u32)),
                ));
            }
            E::OutputEnter { output } => {
                if !toplevel.outputs.contains(&output.id()) {
                    toplevel.outputs.push(output.id());
                }
            }
            E::OutputLeave { output } => toplevel.outputs.retain(|id| *id != output.id()),
            E::Done => {
                if let Some((activated, fullscreen)) = toplevel.pending.take() {
                    toplevel.activated = activated;
                    toplevel.fullscreen = fullscreen;
                }
                state.update_fullscreen();
            }
            E::Closed => {
                state.toplevels.remove(&handle.id());
                handle.destroy();
                state.update_fullscreen();
            }
            _ => {}
        }
    }
}

impl Dispatch<XdgActivationTokenV1, Mutex<Option<oneshot::Sender<String>>>> for State {
    fn event(
        _: &mut Self,
        token: &XdgActivationTokenV1,
        event: xdg_activation_token_v1::Event,
        reply: &Mutex<Option<oneshot::Sender<String>>>,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_activation_token_v1::Event::Done { token: value } = event {
            if let Some(reply) = reply.lock().ok().and_then(|mut r| r.take()) {
                let _ = reply.send(value);
            }
            token.destroy();
        }
    }
}

macro_rules! ignore_events {
    ($($t:ty),*) => {$(
        impl Dispatch<$t, ()> for State {
            fn event(_: &mut Self, _: &$t, _: <$t as Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
        }
    )*};
}
ignore_events!(wl_seat::WlSeat, ExtIdleNotifierV1, XdgActivationV1);
