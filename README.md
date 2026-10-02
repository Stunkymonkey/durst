# durst

A Wayland notification daemon written in Rust, inspired by
[dunst](https://dunst-project.org). It also provides volume and media OSDs,
and everything can be controlled with the mouse and with the `durstctl` CLI.

See [PLAN.md](PLAN.md) for the target design and the roadmap.

## interfaces

### dbus-interfaces

- [org.freedesktop.Notifications](https://specifications.freedesktop.org/notification-spec/latest/) (served): receive notifications
- `org.durst_notification.Durst1` (served): control interface used by `durstctl`, see below
- [org.mpris.MediaPlayer2](https://specifications.freedesktop.org/mpris-spec/latest/) (client): media player control
- [org.freedesktop.login1](https://www.freedesktop.org/software/systemd/man/latest/org.freedesktop.login1.html) (client): session lock state via `LockedHint`

### used wayland-protocols

- [wlr-layer-shell-unstable-v1](https://wayland.app/protocols/wlr-layer-shell-unstable-v1): render notifications and OSDs
- [ext-idle-notify-v1](https://wayland.app/protocols/ext-idle-notify-v1): pause timeouts while the user is idle
- [wlr-foreign-toplevel-management-unstable-v1](https://wayland.app/protocols/wlr-foreign-toplevel-management-unstable-v1): detect fullscreen applications

### other

- [PipeWire](https://pipewire.org): volume and mute of the default sink and source

## development

Everything needed (toolchain, libraries, test tools) comes from the flake:

```sh
nix develop
cargo run -- -v              # fails if another daemon owns org.freedesktop.Notifications
scripts/notify-test.sh       # manual battery of notifications against any running daemon
```

An example config with all current options is in [contrib/config.toml](contrib/config.toml).

### durstctl

```sh
durstctl notif list            # displayed and waiting notifications (--json)
durstctl notif close [ID]      # default: the newest displayed one
durstctl notif action [ID] [KEY]
durstctl history pop           # show the last closed notification again
durstctl mode toggle dnd       # modes switch rules on and off
durstctl reload                # reload the config; errors keep the old one
durstctl info
```

Exit codes: 0 ok, 1 durst not running, 2 invalid arguments, 3 nothing to act
on, 4 invalid config. Everything is also available on the session bus as
`org.durst_notification.Durst1` (e.g. `busctl --user introspect
org.durst_notification.Durst /org/durst_notification/Durst`), including the
`DisplayedCount`, `WaitingCount` and `HistoryCount` properties.

### visual tests

`scripts/visual/run.sh <scenario>` starts durst inside an isolated headless sway
with a private D-Bus session (no service activation, no Xwayland), runs the
scenario, captures a screenshot with `grim` and prints the geometry of every
notification box as JSON, including the padding around its content. A
scenario may define a `verify` function; `check` evaluates assertions on the
measurement, `signals` lists the emitted D-Bus signals, `snap`/`geom`/`pixel`
inspect the screen mid-scenario, and a `<scenario>.toml` next to it is used
as durst's config. The sandbox has its own config and data directories. Mouse input goes
through `tools/vpointer`, a virtual pointer that lives inside the sandbox.
The running session, its notification daemon and the screen are not affected.

```sh
scripts/visual/run-all.sh    # builds, runs all scenarios, prints ok/FAIL
# ok   actions
# ok   close
# ...
```

Screenshots and measurements land in `target/visual/`. Rendering uses iced's
software renderer (`ICED_BACKEND=tiny-skia`), so pixels are deterministic.
The tools (sway, grim, foot, ...) come from the devshell; `tools/vpointer`
(virtual pointer) and `tools/fake-logind` (lock state) are part of the
workspace. `REPEAT=N scripts/visual/run-all.sh` runs every scenario N times.

## ToDo

The detailed milestones are in [PLAN.md](PLAN.md#5-milestones).

 - [x] visual test harness
 - [x] M0: zbus + single iced_layershell daemon, TOML config
 - [x] M1: full notification spec, stack layout, icons, markup, progress, actions, mouse bindings
 - [x] M2: `durstctl` + control interface, history
 - [x] M3: rules & modes (DND)
 - [x] M4: idle pause, fullscreen policy, lock detection, output = "all"
 - [ ] M5: hot reload, systemd unit, D-Bus activation
 - [ ] M6: volume OSD (PipeWire)
 - [ ] M7: media OSD (MPRIS)
