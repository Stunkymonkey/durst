# durst – Project Plan

durst is a Wayland-only notification daemon written in Rust, inspired by
[dunst](https://dunst-project.org). Beyond notifications it provides on-screen
displays (OSDs) for **volume** and **media playback**, and everything can be
controlled with **mouse clicks** and the **`durstctl` CLI**.

This document is the target design. It records the decisions made so far, the
architecture, the feature specification and the order of implementation.

---

## 1. Principles

1. **Wayland only.** Rendering uses `wlr-layer-shell` via `iced_layershell`.
   There is no X11 code path, and the `--force-output xorg|stdout` option is removed.
2. **One daemon, one event loop.** A single long-running iced_layershell
   *daemon*. D-Bus, PipeWire, MPRIS, idle and timers all feed it messages
   through iced `Subscription`s.
3. **Pure core, thin UI.** All notification logic (queue, timeouts, rules,
   stacking, modes, history) lives in a `core` module with no iced or D-Bus
   dependencies, and with time injected so it can be unit-tested
   deterministically. The UI only renders state and forwards input.
4. **Never steal focus.** Surfaces use `KeyboardInteractivity::None`. Keyboard
   control works through compositor keybinds that call `durstctl`.
5. **Spec first.** Implement the
   [Desktop Notifications Spec 1.2](https://specifications.freedesktop.org/notification-spec/latest/)
   fully before adding extras.

## 2. Decisions

| Topic | Decision |
|---|---|
| Notification surfaces | One layer surface **per notification**. durst computes the stack positions with margins and reflows the stack on close. |
| Output | Configurable: `focused` (default), `name:<DP-1>`, or `all` |
| Content | Body markup, icons & images, progress bar (`value` hint), action buttons |
| Behavior | Urgency-based timeouts and styles, pause on idle, modes (DND etc.), duplicate stacking |
| History | In memory as a ring buffer, popable (dunst-like) |
| Rules | Match & restyle, hide/skip, run script, play sound, modes, fullscreen policy, content rewrite, action automation |
| Mouse | dunst defaults, every button configurable |
| Keyboard | Never focused; CLI only |
| Config | TOML at `$XDG_CONFIG_HOME/durst/config.toml`, hot-reloadable |
| Theming | Structured style keys, per urgency and per rule |
| D-Bus | `zbus` (async, pure Rust) replaces `dbus` + `dbus-tree` + codegen |
| IPC | Our own D-Bus interface `org.durst_notification.Durst1` on the session bus |
| CLI | Our own noun–verb design (not dunstctl-compatible); no bar subscribe stream |
| Volume | Native PipeWire (`pipewire-rs`): default sink and default source, volume + mute |
| Volume OSD | Popup with an **interactive** slider (drag, scroll) and a mute button. Stays open while hovered. |
| Media | MPRIS over D-Bus. OSD pops up on track change (cover, title, artist) with prev / play-pause / next buttons |
| Distribution | systemd user unit + D-Bus activation |
| Order | Dunst parity → CLI → rules/modes → environment awareness → ops → volume OSD → media OSD |

## 3. Architecture

### 3.1 Workspace layout

```
durst/            daemon (iced_layershell app)
  src/
    main.rs        arg parsing, logging, start daemon
    app.rs         iced daemon: update/view/subscriptions, surface bookkeeping
    core/          pure logic, no iced / zbus imports
      notification.rs  Notification model, urgency, hints (typed)
      store.rs         active set, waiting queue, history ring
      timer.rs         expiry calculation, pause/resume (idle, hover)
      rules.rs         matching + rule actions
      stack.rs         duplicate stacking, stack-tag replacement
      layout.rs        per-surface position/margin computation
      modes.rs         active mode set
    dbus/
      notifications.rs org.freedesktop.Notifications server (zbus)
      control.rs       org.durst_notification.Durst1 server (zbus)
      mpris.rs         MPRIS client + player tracking
    audio/
      pipewire.rs      default sink/source volume+mute, change events
    wayland/
      idle.rs          ext-idle-notify-v1
      toplevel.rs      wlr-foreign-toplevel-management (fullscreen detection)
    ui/
      notification.rs  notification view
      osd_volume.rs    volume OSD view
      osd_media.rs     media OSD view
      markup.rs        spec markup → iced rich text spans
      icons.rs         icon theme lookup, image-data decoding, cache
      style.rs         config style → iced styles
    config/
      mod.rs           TOML schema (serde), defaults, validation
      watch.rs         hot reload (notify crate)
    sound.rs           sound playback
    script.rs          rule scripts (env vars)
durstctl/         CLI (clap) – thin client over the Durst1 D-Bus interface
durst-proto/      shared: zbus proxy traits, serializable types (Mode, VolumeTarget, …)
contrib/          durst.service, org.freedesktop.Notifications.service, example config
```

`durst-proto` keeps the daemon and the CLI in sync at compile time.

### 3.2 Runtime data flow

```
             ┌──────────────── iced_layershell daemon (single thread UI loop) ──────────────┐
 D-Bus  ───► │ Subscription(zbus Notifications) ─┐                                          │
 durstctl──► │ Subscription(zbus Control)  ──────┤                                          │
 PipeWire──► │ Subscription(pipewire thread) ────┤──► Message ──► update() ──► core::State ──┤
 MPRIS  ───► │ Subscription(mpris watcher) ──────┤                     │                    │
 Wayland ──► │ Subscription(idle / toplevel) ────┤                     ▼                    │
 timer  ───► │ Subscription(tick, only while needed)                Effects:               │
 mouse  ───► │ view() events ────────────────────┘   open/close/move surfaces, emit signals,│
             │                                       run scripts, play sound, set volume    │
             └──────────────────────────────────────────────────────────────────────────────┘
```

- `core::State::handle(event, now) -> Vec<Effect>` is the single point where
  logic happens. `app.rs` turns effects into iced `Task`s (layer-shell
  `NewLayerShell` / `RemoveWindow` / `MarginChange` / `SizeChange`) or into
  calls back into D-Bus, PipeWire or MPRIS.
- D-Bus method calls that need a reply (`Notify` returns the id) are answered
  inside the zbus task. The ID is allocated there with an atomic counter, then
  the notification is forwarded to the UI loop over a channel.
- Long-running external work (PipeWire main loop, scripts, sounds) runs on its
  own threads or tokio tasks and only ever communicates through messages.

### 3.3 Surfaces

Each window id maps to a `SurfaceKind`:

```rust
enum SurfaceKind { Notification(NotifId), VolumeOsd, MediaOsd }
```

- **Notification surfaces:** fixed width (config), height measured (see
  risk R1). They are anchored to the configured corner. The margin along the
  stacking axis is `offset + Σ(height_i + gap)` over the notifications above
  them. Closing one sends `MarginChange` to all surfaces below it.
- **`max_visible`:** notifications beyond the limit wait in the queue, and the
  last visible slot can show "+N more".
- **OSDs:** separate surfaces with their own anchor, margin and output. Only
  one instance of each exists, and repeated triggers reset its hide timer.
- **Output selection:** `focused` → layer surface with no output (the
  compositor picks the focused one); `name:X` → the named output; `all` → one
  surface per output for each item.

## 4. Feature specification

### 4.1 Notification spec compliance

- Methods: `Notify`, `CloseNotification`, `GetCapabilities`,
  `GetServerInformation`.
- Signals: `NotificationClosed(id, reason)` with reason 1 = expired,
  2 = dismissed by user, 3 = `CloseNotification`/CLI, 4 = undefined;
  `ActionInvoked(id, key)`; `ActivationToken(id, token)` when we have one
  (xdg-activation, best effort).
- `replaces_id`: update in place (same surface, re-layout if the height changes).
- `expire_timeout`: `-1` → urgency default, `0` → never, `>0` → ms. The config
  option `ignore_dbus_timeout` overrides it.
- Capabilities: `actions`, `body`, `body-markup`, `body-hyperlinks`,
  `icon-static`, `persistence`, `sound` (once sound playback exists),
  `x-dunst-stack-tag`.
- Hints: `urgency`, `category`, `desktop-entry`, `image-data` /
  `image_data` / `icon_data`, `image-path` / `image_path`, `sound-file`,
  `sound-name`, `suppress-sound`, `transient`, `resident`, `value`,
  `x-dunst-stack-tag`, `x-canonical-private-synchronous`, `hlcolor`.
  Hints are parsed once into a typed `Hints` struct instead of being kept as
  `RefArg`s.

### 4.2 Display

- Layout of one notification: icon (left/right/top/off), app name (optional),
  summary, body, progress bar, action buttons. Text supports word wrap and
  ellipsizing, with `max_lines` for the body.
- **Markup:** `<b> <i> <u> <a href> <img>` (img → alt text). Everything else is
  escaped or stripped. A small tolerant parser produces iced `rich_text` spans.
  Links are clickable when `body-hyperlinks` is set.
- **Icons:** lookup order is `image-data` → `image-path` → `app_icon` (path or
  theme name) → `desktop-entry` icon → rule `default_icon`. Supports SVG and
  raster images, with min/max icon size and a cache keyed by name and size.
- **Progress:** the `value` hint renders a bar with an optional percentage.
- **Actions:** buttons for each action (except `default`, which runs on click).
- **Duplicates:** an identical app+summary+body+urgency increments a counter
  ("×3") and resets the timer instead of creating a new surface.
  `x-dunst-stack-tag` / `x-canonical-private-synchronous` replace the existing
  notification with the same tag (volume/brightness scripts).
- **Timers:** pause while the notification is hovered. They also pause for all
  notifications while the user is idle or the session is locked (critical
  notifications have no timer anyway).

### 4.3 Mouse

Configurable per button: `left`, `middle`, `right`, `scroll_up`,
`scroll_down`. Each maps to a list of actions:

`close_current`, `close_all`, `do_action` (default action, otherwise opens the
first one), `open_url` (first link in the body), `history_pop`, `none`.

Defaults (same as dunst): left = `close_current`, middle = `do_action, close_current`,
right = `close_all`.

### 4.4 Modes & DND

- The daemon holds a **set** of active modes (mako-style). Only `default` is
  built in; all others, such as `dnd`, `work` and `gaming`, are defined by the
  user's rules.
- Rules can match `mode = "dnd"`. The rule action `defer = true` keeps a
  notification in the waiting queue instead of showing it.
- Whenever the modes change, the waiting queue is re-evaluated, so turning off
  `dnd` shows everything that was held back.
- The shipped example config contains:
  `[[rule]] mode="dnd" urgency!="critical" → defer=true`.

### 4.5 Rules

`[[rule]]` entries are evaluated in order, and each matching rule applies its
actions (later rules override earlier ones), as in dunst.

**Matchers** (all optional, AND-combined; strings are regexes):
`app_name`, `summary`, `body`, `icon`, `category`, `desktop_entry`,
`urgency`, `stack_tag`, `transient`, `mode`, `fullscreen_active`,
`has_actions`, and `not_*` variants for negation.

**Actions:**

| Group | Keys |
|---|---|
| Restyle | `style.*` (any style key), `icon`, `default_icon`, `icon_position`, `position`/`anchor`, `output` |
| Timing | `timeout`, `urgency` (rewrite), `ignore_dbus_timeout` |
| Visibility | `skip_display` (history only), `history_ignore`, `defer`, `transient` |
| Content | `format` (template with `{app_name} {summary} {body} {count}`…), `hide_body`, `category`, `stack_tag` |
| Fullscreen | `fullscreen = "show" \| "delay" \| "pushback"` |
| Actions | `default_action` (action key for left-click), `auto_invoke` (action key run immediately) |
| Side effects | `script` (run with `DURST_*` env vars), `script_on = "receive" \| "action" \| "close"`, `sound` (file path or sound-theme name), `mute_sound` |

### 4.6 History

- Ring buffer of `history_length` entries (default 20).
- Closed notifications go into history unless they are `history_ignore` or
  `transient`.
- `durstctl history pop` re-shows the most recent one; `history list` prints
  JSON; `history clear` empties it.

### 4.7 Environment awareness

- **Idle:** `ext-idle-notify-v1` with `idle_threshold` (default 120 s). While
  idle, timers are frozen.
- **Fullscreen:** `zwlr_foreign_toplevel_manager_v1` reports which toplevel is
  activated and fullscreen. Each rule's fullscreen policy applies:
  `show` shows normally, `delay` holds the notification until fullscreen ends,
  and `pushback` hides it into the waiting queue. If the protocol isn't
  available, durst falls back to `show` and logs one warning.
- **Lock:** logind `LockedHint` on the session over system D-Bus is treated
  the same as idle for timers.

### 4.8 Volume OSD

- **Backend:** a PipeWire thread tracks the default sink and default source
  (via metadata `default.audio.sink/source`). It reads and writes the
  `channelVolumes` and `mute` props, and sends a `VolumeChanged` message on
  every change.
- **UI:** an icon that reflects the level and mute state, a slider (0–100 %,
  optionally up to `max_volume`, e.g. 150 %) and a mute button. Scrolling over
  the OSD changes the volume by `step`. Drag updates are debounced (about 30 ms).
- **When shown:** after a `durstctl volume …` command; on external changes if
  `show_on_external_change = true`. Hides after `timeout`, except while hovered.
- The microphone uses the same OSD in mic variant (`--mic`).

### 4.9 Media OSD

- **Backend:** watches `org.mpris.MediaPlayer2.*` names. The active player is
  the most recent one that changed to `Playing`, otherwise the last one
  active. Metadata comes from `PropertiesChanged`.
- **UI:** cover art (`mpris:artUrl`, either file:// or http(s), downloaded with
  a timeout and cached), title, artist, prev / play-pause / next buttons.
- **When shown:** on track change (configurable), and after `durstctl media …`
  commands. Hides after `timeout`, except while hovered.

### 4.10 CLI – `durstctl`

```
durstctl notif   list [--json] | count | close [ID] | close-all
                 action [ID] [KEY]          # default: newest, "default" key
durstctl history list [--json] | pop | clear | count
durstctl mode    list | set <M>... | enable <M> | disable <M> | toggle <M>
durstctl volume  get [--mic] | set <N|+N%|-N%> [--mic] | mute <on|off|toggle> [--mic]
durstctl media   status [--json] | play | pause | toggle | next | prev
durstctl osd     show <volume|mic|media>
durstctl reload
durstctl info                               # version, config path, active modes, counts
```

- Exit codes: 0 = ok, 1 = daemon not running, 2 = bad argument, 3 = nothing to act on.
- Shell completions and a man page are generated in `build.rs` (`clap_complete`, `clap_mangen`).

### 4.11 Control D-Bus interface

Bus name `org.durst_notification.Durst`, object `/org/durst_notification/Durst`,
interface `org.durst_notification.Durst1`. Its methods map one-to-one to the CLI
commands above. It also exposes the properties `ActiveModes`, `WaitingCount`,
`DisplayedCount` and `HistoryCount` with `PropertiesChanged`, which keeps it
scriptable through `busctl` even without a dedicated subscribe command.

### 4.12 Configuration

`$XDG_CONFIG_HOME/durst/config.toml`, which can be overridden with `-c`. An
unknown key is an error, reported with its line and column. On reload, an
invalid config keeps the old one and shows a critical notification from
durst itself.

```toml
[general]
output = "focused"          # "focused" | "all" | "name:DP-1"
anchor = "top-right"
offset = [20, 20]           # x, y
gap = 8
width = 380
max_visible = 5
history_length = 20
idle_threshold = "120s"
ignore_dbus_timeout = false

[mouse]
left = ["close_current"]
middle = ["do_action", "close_current"]
right = ["close_all"]

[style]                      # base style
font = "Inter 11"
background = "#1e1e2ecc"
foreground = "#cdd6f4"
border = { width = 2, color = "#89b4fa", radius = 10 }
padding = [10, 12]
icon = { position = "left", min_size = 32, max_size = 64 }
progress = { height = 6, color = "#89b4fa" }

[urgency.low]
timeout = "5s"
style = { border.color = "#6c7086" }
[urgency.normal]
timeout = "10s"
[urgency.critical]
timeout = "0"
style = { border.color = "#f38ba8" }

[[rule]]
mode = "dnd"
not_urgency = "critical"
defer = true

[[rule]]
app_name = "^(Signal|Element)$"
hide_body = true
sound = "message-new-instant"

[[rule]]
app_name = "^spotify$"
skip_display = true          # the media OSD handles this

[osd.volume]
enabled = true
anchor = "bottom"
timeout = "2s"
step = 5
max_volume = 100
show_on_external_change = true

[osd.media]
enabled = true
anchor = "top"
timeout = "4s"
show_on_track_change = true
```

## 5. Milestones

Each milestone ends with a working, tagged build.

### M0 – Foundation (core rework)
- Replace `dbus`, `dbus-tree` and `dbus-codegen` with `zbus`. Delete `build.rs`
  codegen and the XML file, and keep `build.rs` only for completions.
- Upgrade `iced` / `iced_layershell` to versions that support daemon
  mode (`NewLayerShell` / `RemoveWindow` / `MarginChange`). Pin them.
- Single daemon: the zbus server runs as a Subscription and Notify opens a surface.
- `serde_yaml` → `toml`, with the config schema skeleton and defaults.
- Remove the `--force-output` option; replace `println!` with `log`.
- Create the `durst-proto` crate.
- Add `scripts/notify-test.sh` (a battery of `notify-send` / `gdbus` calls).
- **Done when:** `notify-send a b` shows a popup, it closes on click, and
  `NotificationClosed` is emitted.

### M1 – Notification parity (daily driver)
- Full spec (4.1), per-urgency timeouts, hover pause, `replaces_id`.
- Stack layout with reflow, `max_visible`, anchors, output selection.
- Icons and images, markup, progress bar, action buttons, duplicate counter,
  stack tags.
- Mouse bindings (4.3), styles per urgency.
- **Done when:** it replaces dunst on the maintainer's machine for a week
  without regressions.

### M2 – CLI & control interface
- The Durst1 interface (4.11) and the `notif`, `history`, `reload` and `info`
  commands in durstctl.
- History ring (4.6).
- Completions and man page.

### M3 – Rules & modes
- Matchers and all rule actions (4.5), modes and defer (4.4), the `mode` CLI.
- Scripts with env vars, sound playback (sound theme lookup + `rodio` or
  `pw-play`; decided in this milestone).
- `default_action` / `auto_invoke`.

### M4 – Environment awareness
- Idle pause, fullscreen policy, lock detection (4.7).

### M5 – Operations
- Config hot reload (file watch plus `durstctl reload`), error notification on
  a bad config.
- `contrib/durst.service` (`Type=dbus`, `BusName=org.freedesktop.Notifications`,
  `PartOf=graphical-session.target`) and the D-Bus activation file.
- Flake package output, a CI matrix (fmt, clippy, test) and a README rewrite
  with the config reference.

### M6 – Volume OSD
- PipeWire backend (4.8), volume OSD UI, `durstctl volume`, `osd show volume|mic`.

### M7 – Media OSD
- MPRIS watcher and active-player logic (4.9), media OSD UI, `durstctl media`.

### M8 – Polish
- Performance (icon cache, idle CPU ≈ 0: no tick subscription when nothing
  is timing out), fuzzing the markup parser, and accessibility (font scaling).

## 6. Testing strategy

- **Unit:** `core` is fully deterministic with injected `Instant`s. This covers
  timeouts, pause/resume, rule matching, stacking, defer/modes, history and
  layout math.
- **Markup:** table-driven tests plus a `cargo fuzz` target.
- **Integration:** start a private bus (`dbus-daemon --session --print-address`)
  and run the daemon's zbus service headless (without UI), using the `core`
  effects as observable output. A zbus client sends `Notify` and asserts the
  replies and signals.
- **Config:** parse the shipped example config in a test, and check that
  unknown-key errors include line and column.
- **Manual:** `scripts/notify-test.sh` on sway, Hyprland and niri.

## 7. Risks & open questions

| # | Risk | Mitigation |
|---|---|---|
| R1 | With one surface per notification, durst must know each surface's height before positioning the ones below. | Do a spike in M0: measure text with iced's `Paragraph` API, using the same font and width as the view. Fallback: render first, report the size from a measuring widget, then send `SizeChange` and reflow. |
| R2 | iced_layershell API churn between versions | Pin the versions and keep all layer-shell calls in `app.rs`. |
| R3 | Not every compositor implements `wlr-foreign-toplevel-management` (e.g. GNOME). | Optional feature with a graceful fallback. GNOME/KDE aren't targets anyway, since they run their own notification servers. |
| R4 | `focused` output depends on the compositor, which places output-less layer surfaces where it likes. | Document it. `name:` is always available for deterministic placement. |
| R5 | PipeWire API complexity (default node tracking, channel volumes). | Isolate it behind `audio::Backend` with a trait so a `wpctl` fallback stays possible. |
| R6 | The interactive slider floods PipeWire with updates. | Debounce, and only send the last value. |

Open, to decide during implementation:
- Sound playback library (`rodio` in-process vs. spawning `pw-play`).
- Whether the "+N more" indicator is its own surface or part of the last
  visible notification.
- Animations (deliberately out of scope for now).

## 8. Corrections to the current README

- `ext-foreign-toplevel-list-v1` doesn't expose fullscreen or activated state,
  so `wlr-foreign-toplevel-management-unstable-v1` is used instead.
- `ext-session-lock-v1` is only for the lock client itself, so lock state comes
  from logind `LockedHint` instead.
- The ToDo "replace dbus with zbus?" is decided: yes (M0).
