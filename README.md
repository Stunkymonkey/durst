# durst

A Wayland notification daemon written in Rust, inspired by
[dunst](https://dunst-project.org). It also provides volume and media OSDs,
and everything can be controlled with the mouse and with the `durstctl` CLI.

See [PLAN.md](PLAN.md) for the target design and the roadmap.

## interfaces

### dbus-interfaces

- [org.freedesktop.Notifications](https://specifications.freedesktop.org/notification-spec/latest/) (served): receive notifications
- `org.durst_notification.Durst1` (served): control interface used by `durstctl`
- [org.mpris.MediaPlayer2](https://specifications.freedesktop.org/mpris-spec/latest/) (client): media player control
- [org.freedesktop.login1](https://www.freedesktop.org/software/systemd/man/latest/org.freedesktop.login1.html) (client): session lock state via `LockedHint`

### used wayland-protocols

- [wlr-layer-shell-unstable-v1](https://wayland.app/protocols/wlr-layer-shell-unstable-v1): render notifications and OSDs
- [ext-idle-notify-v1](https://wayland.app/protocols/ext-idle-notify-v1): detect user idle
- [wlr-foreign-toplevel-management-unstable-v1](https://wayland.app/protocols/wlr-foreign-toplevel-management-unstable-v1): detect fullscreen applications

### other

- [PipeWire](https://pipewire.org): volume and mute of the default sink and source

## development

### visual tests

`scripts/visual/run.sh <scenario>` starts durst inside an isolated headless sway
with a private D-Bus session, runs the scenario (e.g. `notify-send` calls),
captures a screenshot with `grim` and prints the geometry of every
notification box as JSON. The running session and its notification daemon are
not affected.

```sh
cargo build
nix develop -c scripts/visual/run.sh scripts/visual/scenarios/single.sh
# screenshot: target/visual/single.png
# {"size": [1280, 720], "boxes": [{"x": 830, "y": 50, "w": 400, "h": 100}], "gaps": []}
```

Requires `sway`, `grim`, `dbus-run-session` and `python3` with Pillow.
Rendering uses iced's software renderer (`ICED_BACKEND=tiny-skia`).

## ToDo

The detailed milestones are in [PLAN.md](PLAN.md#5-milestones).

 - [x] `dbus`: get notifications
 - [x] render
 - [x] multi window
 - [x] visual test harness
 - [ ] M0: zbus + single iced_layershell daemon, TOML config
 - [ ] M1: full notification spec, stack layout, icons, markup, progress, actions, mouse bindings
 - [ ] M2: `durstctl` + control interface, history
 - [ ] M3: rules & modes (DND)
 - [ ] M4: idle pause, fullscreen policy, lock detection
 - [ ] M5: hot reload, systemd unit, D-Bus activation
 - [ ] M6: volume OSD (PipeWire)
 - [ ] M7: media OSD (MPRIS)
