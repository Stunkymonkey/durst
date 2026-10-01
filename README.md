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

Everything needed (toolchain, libraries, test tools) comes from the flake:

```sh
nix develop
cargo run -- -v              # fails if another daemon owns org.freedesktop.Notifications
scripts/notify-test.sh       # manual battery of notifications against any running daemon
```

An example config with all current options is in [contrib/config.toml](contrib/config.toml).

### visual tests

`scripts/visual/run.sh <scenario>` starts durst inside an isolated headless sway
with a private D-Bus session (no service activation, no Xwayland), runs the
scenario, captures a screenshot with `grim` and prints the geometry of every
notification box as JSON, including the padding around its content. A
scenario may define a `verify` function; `check` evaluates assertions on the
measurement and `signals` lists the emitted D-Bus signals. Mouse input goes
through `tools/vpointer`, a virtual pointer that lives inside the sandbox.
The running session, its notification daemon and the screen are not affected.

```sh
scripts/visual/run-all.sh    # builds, runs all scenarios, prints ok/FAIL
# ok   close
# ok   single
# ok   stack
```

Screenshots and measurements land in `target/visual/`. Rendering uses iced's
software renderer (`ICED_BACKEND=tiny-skia`), so pixels are deterministic.

## ToDo

The detailed milestones are in [PLAN.md](PLAN.md#5-milestones).

 - [x] visual test harness
 - [x] M0: zbus + single iced_layershell daemon, TOML config
 - [ ] M1: full notification spec, stack layout, icons, markup, progress, actions, mouse bindings
 - [ ] M2: `durstctl` + control interface, history
 - [ ] M3: rules & modes (DND)
 - [ ] M4: idle pause, fullscreen policy, lock detection
 - [ ] M5: hot reload, systemd unit, D-Bus activation
 - [ ] M6: volume OSD (PipeWire)
 - [ ] M7: media OSD (MPRIS)
