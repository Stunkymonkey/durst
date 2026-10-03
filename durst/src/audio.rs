//! Volume and mute of the default sink (speakers) and source (microphone),
//! natively through PipeWire.
//!
//! A thread runs a PipeWire main loop. It follows the `default` metadata for
//! the default nodes, their `Props` (volume, mute) and the active `Route`s of
//! audio devices. Like WirePlumber's mixer, changes go to the device's route
//! (and are saved there) when the node belongs to a device, else to the
//! node's `Props`. Volumes are shown on the cubic scale of pavucontrol and
//! wpctl: 50 % is a linear factor of 0.125.

use std::cell::RefCell;
use std::collections::HashMap;
use std::io::Cursor;
use std::rc::Rc;

use futures::channel::mpsc::{UnboundedSender, unbounded};
use futures::{SinkExt, Stream, StreamExt};
use iced::Subscription;
use pipewire as pw;
use pw::device::{Device, DeviceListener};
use pw::metadata::{Metadata, MetadataListener};
use pw::node::{Node, NodeListener};
use pw::spa::param::ParamType;
use pw::spa::pod::deserialize::PodDeserializer;
use pw::spa::pod::serialize::PodSerializer;
use pw::spa::pod::{Object, Pod, Property, PropertyFlags, Value, ValueArray};
use pw::spa::sys as spa;
use pw::types::ObjectType;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Target {
    /// the default sink: speakers, headphones
    Sink,
    /// the default source: the microphone
    Source,
}

impl Target {
    fn metadata_key(self) -> &'static str {
        match self {
            Target::Sink => "default.audio.sink",
            Target::Source => "default.audio.source",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Volume {
    /// cubic scale, may exceed 100
    pub percent: u32,
    pub muted: bool,
    /// the node's description, e.g. "Built-in Audio Analog Stereo"
    pub description: String,
}

#[derive(Debug, Clone)]
pub enum Event {
    /// connected; commands go through the handle
    Ready(Handle),
    /// the volume of the target's default node; `None`: there is none
    Changed(Target, Option<Volume>),
    /// PipeWire went away (e.g. restarted); a `Ready` follows on reconnect
    Disconnected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    SetPercent(Target, u32),
    SetMute(Target, bool),
}

/// Sends commands to the PipeWire thread.
#[derive(Clone)]
pub struct Handle(pw::channel::Sender<Command>);

impl Handle {
    pub fn send(&self, command: Command) {
        if self.0.send(command).is_err() {
            log::warn!("the PipeWire connection is gone");
        }
    }
}

impl std::fmt::Debug for Handle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("audio::Handle")
    }
}

pub fn subscription() -> Subscription<Event> {
    Subscription::run(stream)
}

/// Runs the PipeWire thread, and again whenever it ends (PipeWire missing or
/// restarted).
fn stream() -> impl Stream<Item = Event> {
    iced::stream::channel(16, async |mut output| {
        let mut backoff = crate::retry::Backoff::new("volume control");
        loop {
            backoff.attempt();
            let (tx, mut rx) = unbounded();
            let (done_tx, done_rx) = futures::channel::oneshot::channel();
            std::thread::Builder::new()
                .name("pipewire".into())
                .spawn(move || {
                    let result = run(tx).map_or_else(|e| e.to_string(), |()| "disconnected".into());
                    let _ = done_tx.send(result);
                })
                .expect("spawn pipewire thread");
            while let Some(event) = rx.next().await {
                if output.send(event).await.is_err() {
                    return;
                }
            }
            if output.send(Event::Disconnected).await.is_err() {
                return;
            }
            let reason = done_rx.await.unwrap_or_else(|_| "thread ended".into());
            backoff.failed(reason).await;
        }
    })
}

/// Linear PipeWire volume to the percent shown to users.
pub fn linear_to_percent(linear: f32) -> u32 {
    (linear.max(0.0).cbrt() * 100.0).round() as u32
}

pub fn percent_to_linear(percent: u32) -> f32 {
    (percent as f32 / 100.0).powi(3)
}

/// New channel volumes for `percent`, keeping the balance between channels.
fn scaled(volumes: &[f32], percent: u32) -> Vec<f32> {
    let target = percent_to_linear(percent);
    let max = volumes.iter().copied().fold(0.0, f32::max);
    match (volumes.is_empty(), max > 0.0) {
        (true, _) => vec![target; 2],
        (false, true) => volumes.iter().map(|v| v * target / max).collect(),
        (false, false) => vec![target; volumes.len()],
    }
}

/// `{ "name": "alsa_output..." }` as stored in the `default` metadata.
fn metadata_name(value: &str) -> Option<String> {
    let rest = &value[value.find("\"name\"")? + 6..];
    let rest = &rest[rest.find(':')? + 1..];
    let rest = &rest[rest.find('"')? + 1..];
    Some(rest[..rest.find('"')?].to_owned())
}

fn serialize(value: &Value) -> Vec<u8> {
    PodSerializer::serialize(Cursor::new(Vec::new()), value)
        .expect("serialize pod")
        .0
        .into_inner()
}

fn property(key: u32, value: Value) -> Property {
    Property {
        key,
        flags: PropertyFlags::empty(),
        value,
    }
}

fn props_value(volumes: Option<&[f32]>, mute: Option<bool>) -> Value {
    let mut properties = Vec::new();
    if let Some(volumes) = volumes {
        properties.push(property(
            spa::SPA_PROP_channelVolumes,
            Value::ValueArray(ValueArray::Float(volumes.to_vec())),
        ));
    }
    if let Some(mute) = mute {
        properties.push(property(spa::SPA_PROP_mute, Value::Bool(mute)));
    }
    Value::Object(Object {
        type_: spa::SPA_TYPE_OBJECT_Props,
        id: spa::SPA_PARAM_Props,
        properties,
    })
}

/// A `Route` param that changes the route's props and saves them.
fn route_value(index: i32, device: i32, props: Value) -> Value {
    Value::Object(Object {
        type_: spa::SPA_TYPE_OBJECT_ParamRoute,
        id: spa::SPA_PARAM_Route,
        properties: vec![
            property(spa::SPA_PARAM_ROUTE_index, Value::Int(index)),
            property(spa::SPA_PARAM_ROUTE_device, Value::Int(device)),
            property(spa::SPA_PARAM_ROUTE_props, props),
            property(spa::SPA_PARAM_ROUTE_save, Value::Bool(true)),
        ],
    })
}

fn to_object(pod: &Pod) -> Option<Object> {
    match PodDeserializer::deserialize_any_from(pod.as_bytes()) {
        Ok((_, Value::Object(object))) => Some(object),
        _ => None,
    }
}

/// Channel volumes and mute from a `Props` param, if it has them.
fn parse_props(object: &Object) -> (Option<Vec<f32>>, Option<bool>) {
    let mut volumes = None;
    let mut mute = None;
    for p in &object.properties {
        match (p.key, &p.value) {
            (spa::SPA_PROP_channelVolumes, Value::ValueArray(ValueArray::Float(v))) => {
                volumes = Some(v.clone())
            }
            (spa::SPA_PROP_mute, Value::Bool(m)) => mute = Some(*m),
            _ => {}
        }
    }
    (volumes, mute)
}

/// (route index, route device) of an active `Route` param.
fn parse_route(object: &Object) -> Option<(i32, i32)> {
    let int = |key| {
        object
            .properties
            .iter()
            .find_map(|p| match (p.key == key, &p.value) {
                (true, Value::Int(i)) => Some(*i),
                _ => None,
            })
    };
    Some((
        int(spa::SPA_PARAM_ROUTE_index)?,
        int(spa::SPA_PARAM_ROUTE_device)?,
    ))
}

struct NodeState {
    proxy: Node,
    _listener: NodeListener,
    name: String,
    description: String,
    /// (device id, route device) for nodes of a sound card
    device: Option<(u32, i32)>,
    volumes: Vec<f32>,
    mute: bool,
}

struct DeviceState {
    proxy: Device,
    _listener: DeviceListener,
    /// active routes: route device -> route index
    routes: HashMap<i32, i32>,
}

struct State {
    tx: UnboundedSender<Event>,
    nodes: HashMap<u32, NodeState>,
    devices: HashMap<u32, DeviceState>,
    metadata: Option<(u32, Metadata, MetadataListener)>,
    defaults: HashMap<Target, String>,
    sent: HashMap<Target, Option<Volume>>,
}

impl State {
    fn default_node(&self, target: Target) -> Option<&NodeState> {
        let name = self.defaults.get(&target)?;
        self.nodes.values().find(|n| &n.name == name)
    }

    fn volume(&self, target: Target) -> Option<Volume> {
        let node = self.default_node(target)?;
        let max = node
            .volumes
            .iter()
            .copied()
            .fold(None, |m: Option<f32>, v| Some(m.map_or(v, |m| m.max(v))))?;
        Some(Volume {
            percent: linear_to_percent(max),
            muted: node.mute,
            description: node.description.clone(),
        })
    }

    /// How a change of the target would be applied, for the debug log.
    fn path(&self, target: Target) -> String {
        let Some(node) = self.default_node(target) else {
            return "no default node".into();
        };
        match node.device {
            Some((device, route_device)) => {
                match self
                    .devices
                    .get(&device)
                    .and_then(|d| d.routes.get(&route_device))
                {
                    Some(index) => format!(
                        "{}: route {index} (device {route_device}) of device {device}",
                        node.name
                    ),
                    None => format!(
                        "{}: node props (device {device} has no route yet)",
                        node.name
                    ),
                }
            }
            None => format!("{}: node props", node.name),
        }
    }

    /// Sends the volumes that changed since last time.
    fn publish(&mut self) {
        for target in [Target::Sink, Target::Source] {
            let volume = self.volume(target);
            if self.sent.get(&target) != Some(&volume) {
                log::debug!("{target:?} volume {volume:?} via {}", self.path(target));
                self.sent.insert(target, volume.clone());
                let _ = self.tx.unbounded_send(Event::Changed(target, volume));
            }
        }
    }

    fn apply(&self, command: Command) {
        let target = match command {
            Command::SetPercent(t, _) | Command::SetMute(t, _) => t,
        };
        let Some(node) = self.default_node(target) else {
            log::warn!("no default {target:?} to change");
            return;
        };
        let props = match command {
            Command::SetPercent(_, percent) => {
                props_value(Some(&scaled(&node.volumes, percent)), None)
            }
            Command::SetMute(_, mute) => props_value(None, Some(mute)),
        };
        log::debug!("{command:?} on {}", node.name);
        let route = node.device.and_then(|(device_id, route_device)| {
            let device = self.devices.get(&device_id)?;
            Some((device, *device.routes.get(&route_device)?, route_device))
        });
        match route {
            Some((device, index, route_device)) => {
                let value = route_value(index, route_device, props);
                let bytes = serialize(&value);
                device.proxy.set_param(
                    ParamType::Route,
                    0,
                    Pod::from_bytes(&bytes).expect("route pod"),
                );
            }
            None => {
                let bytes = serialize(&props);
                node.proxy.set_param(
                    ParamType::Props,
                    0,
                    Pod::from_bytes(&bytes).expect("props pod"),
                );
            }
        }
    }
}

fn run(tx: UnboundedSender<Event>) -> Result<(), pw::Error> {
    pw::init();
    let main_loop = pw::main_loop::MainLoopRc::new(None)?;
    let context = pw::context::ContextRc::new(&main_loop, None)?;
    let core = context.connect_rc(None)?;
    let registry = core.get_registry_rc()?;

    let state = Rc::new(RefCell::new(State {
        tx: tx.clone(),
        nodes: HashMap::new(),
        devices: HashMap::new(),
        metadata: None,
        defaults: HashMap::new(),
        sent: HashMap::new(),
    }));

    let (commands, receiver) = pw::channel::channel();
    let _receiver = receiver.attach(main_loop.loop_(), {
        let state = state.clone();
        move |command| state.borrow().apply(command)
    });
    let _ = tx.unbounded_send(Event::Ready(Handle(commands)));

    // PipeWire went away: stop, the volume shows as unavailable
    let main_loop_weak = main_loop.downgrade();
    let _core_listener = core
        .add_listener_local()
        .error(move |id, _, _, message| {
            if id == pw::core::PW_ID_CORE {
                log::warn!("PipeWire: {message}");
                if let Some(main_loop) = main_loop_weak.upgrade() {
                    main_loop.quit();
                }
            }
        })
        .register();

    let registry_weak = registry.downgrade();
    let global_state = state.clone();
    let remove_state = state.clone();
    let _registry_listener = registry
        .add_listener_local()
        .global(move |global| {
            let Some(registry) = registry_weak.upgrade() else {
                return;
            };
            let props = global.props;
            let get = |key| props.and_then(|p| p.get(key));
            let id = global.id;
            match global.type_ {
                ObjectType::Node
                    if get("media.class").is_some_and(|c| {
                        c.starts_with("Audio/Sink") || c.starts_with("Audio/Source")
                    }) =>
                {
                    let Ok(node) = registry.bind::<Node, _>(global) else {
                        return;
                    };
                    let info_state = Rc::downgrade(&global_state);
                    let param_state = Rc::downgrade(&global_state);
                    let listener = node
                        .add_listener_local()
                        .info(move |info| {
                            let Some(state) = info_state.upgrade() else {
                                return;
                            };
                            let mut state = state.borrow_mut();
                            // later info events only carry what changed, e.g. params
                            let props = info
                                .change_mask()
                                .contains(pw::node::NodeChangeMask::PROPS)
                                .then(|| info.props())
                                .flatten();
                            if let (Some(node), Some(props)) = (state.nodes.get_mut(&id), props) {
                                let get = |k| props.get(k).map(str::to_owned);
                                node.name = get("node.name").unwrap_or_default();
                                node.description = get("node.description")
                                    .or_else(|| get("node.nick"))
                                    .unwrap_or_else(|| node.name.clone());
                                node.device = get("device.id")
                                    .and_then(|d| d.parse().ok())
                                    .zip(get("card.profile.device").and_then(|d| d.parse().ok()));
                            }
                            state.publish();
                        })
                        .param(move |_, kind, _, _, pod| {
                            let (Some(state), Some(object)) =
                                (param_state.upgrade(), pod.and_then(to_object))
                            else {
                                return;
                            };
                            if kind != ParamType::Props {
                                return;
                            }
                            let mut state = state.borrow_mut();
                            if let Some(node) = state.nodes.get_mut(&id) {
                                let (volumes, mute) = parse_props(&object);
                                if let Some(volumes) = volumes {
                                    node.volumes = volumes;
                                }
                                if let Some(mute) = mute {
                                    node.mute = mute;
                                }
                            }
                            state.publish();
                        })
                        .register();
                    node.subscribe_params(&[ParamType::Props]);
                    global_state.borrow_mut().nodes.insert(
                        id,
                        NodeState {
                            proxy: node,
                            _listener: listener,
                            name: get("node.name").unwrap_or_default().to_owned(),
                            description: String::new(),
                            device: None,
                            volumes: Vec::new(),
                            mute: false,
                        },
                    );
                }
                ObjectType::Device if get("media.class") == Some("Audio/Device") => {
                    let Ok(device) = registry.bind::<Device, _>(global) else {
                        return;
                    };
                    let param_state = Rc::downgrade(&global_state);
                    let listener = device
                        .add_listener_local()
                        .param(move |_, kind, _, _, pod| {
                            let (Some(state), Some(object)) =
                                (param_state.upgrade(), pod.and_then(to_object))
                            else {
                                return;
                            };
                            if kind != ParamType::Route {
                                return;
                            }
                            if let (Some(device), Some((index, route_device))) = (
                                state.borrow_mut().devices.get_mut(&id),
                                parse_route(&object),
                            ) {
                                device.routes.insert(route_device, index);
                            }
                        })
                        .register();
                    device.subscribe_params(&[ParamType::Route]);
                    global_state.borrow_mut().devices.insert(
                        id,
                        DeviceState {
                            proxy: device,
                            _listener: listener,
                            routes: HashMap::new(),
                        },
                    );
                }
                ObjectType::Metadata if get("metadata.name") == Some("default") => {
                    let Ok(metadata) = registry.bind::<Metadata, _>(global) else {
                        return;
                    };
                    let property_state = Rc::downgrade(&global_state);
                    let listener = metadata
                        .add_listener_local()
                        .property(move |subject, key, _, value| {
                            let Some(state) = property_state.upgrade() else {
                                return 0;
                            };
                            let mut state = state.borrow_mut();
                            for target in [Target::Sink, Target::Source] {
                                // key None: everything was cleared
                                if subject == 0 && key.is_none_or(|k| k == target.metadata_key()) {
                                    match value.and_then(metadata_name) {
                                        Some(name) => state.defaults.insert(target, name),
                                        None => state.defaults.remove(&target),
                                    };
                                }
                            }
                            state.publish();
                            0
                        })
                        .register();
                    global_state.borrow_mut().metadata = Some((id, metadata, listener));
                }
                _ => {}
            }
        })
        .global_remove(move |id| {
            let mut state = remove_state.borrow_mut();
            state.nodes.remove(&id);
            state.devices.remove(&id);
            if state.metadata.as_ref().is_some_and(|(m, _, _)| *m == id) {
                state.metadata = None;
                state.defaults.clear();
            }
            state.publish();
        })
        .register();

    main_loop.run();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cubic_percent() {
        assert_eq!(linear_to_percent(1.0), 100);
        assert_eq!(linear_to_percent(0.125), 50);
        assert_eq!(linear_to_percent(0.0), 0);
        assert!((percent_to_linear(50) - 0.125).abs() < 1e-6);
        assert_eq!(linear_to_percent(percent_to_linear(150)), 150);
    }

    #[test]
    fn scaling_keeps_the_balance() {
        assert_eq!(scaled(&[1.0, 0.5], 50), vec![0.125, 0.0625]);
        assert_eq!(scaled(&[0.0, 0.0], 100), vec![1.0, 1.0]);
        assert_eq!(scaled(&[], 100), vec![1.0, 1.0]);
    }

    #[test]
    fn metadata_names() {
        assert_eq!(
            metadata_name(r#"{ "name": "alsa_output.pci-0000_00_1f.3.analog-stereo" }"#).as_deref(),
            Some("alsa_output.pci-0000_00_1f.3.analog-stereo")
        );
        assert_eq!(metadata_name(r#"{"name":"x"}"#).as_deref(), Some("x"));
        assert_eq!(metadata_name("{}"), None);
    }

    #[test]
    fn pods_round_trip() {
        let bytes = serialize(&props_value(Some(&[0.5, 0.25]), Some(true)));
        let object = to_object(Pod::from_bytes(&bytes).unwrap()).unwrap();
        assert_eq!(parse_props(&object), (Some(vec![0.5, 0.25]), Some(true)));

        let route = route_value(3, 7, props_value(None, Some(false)));
        let bytes = serialize(&route);
        let object = to_object(Pod::from_bytes(&bytes).unwrap()).unwrap();
        assert_eq!(parse_route(&object), Some((3, 7)));
        let props = object
            .properties
            .iter()
            .find(|p| p.key == spa::SPA_PARAM_ROUTE_props)
            .unwrap();
        match &props.value {
            Value::Object(o) => assert_eq!(parse_props(o), (None, Some(false))),
            v => panic!("{v:?}"),
        }
    }
}
