# durst – Project Plan

durst is a Wayland-only notification daemon written in Rust, inspired by
[dunst](https://dunst-project.org). Beyond notifications it provides on-screen
displays (OSDs) for **volume** and **media playback**, and everything can be
controlled with **mouse clicks** and the **`durstctl` CLI**.

This document describes durst **as built** (state: 2026-10-03, branch
`iced-layers`, not merged into `master` yet), the milestones, the risks, and
in [section 8](#8-missing-parts) everything that is still missing.

---

## 1. Principles

1. **Wayland only.** Rendering uses `wlr-layer-shell` via `iced_layershell`.
   There is no X11 code path.
2. **One daemon, one event loop.** A single long-running iced_layershell
   *daemon*. D-Bus, PipeWire, MPRIS, Wayland state, logind, the config watcher
   and timers all feed it messages through iced `Subscription`s.
3. **Pure core, thin UI.** Notification logic (store, timers, rules, history,
   layout, media players) lives in `core/` without iced or D-Bus types, with
   time injected, and is unit-tested. The UI renders state and forwards input.
4. **Never steal focus.** Surfaces use `KeyboardInteractivity::None`. Keyboard
   control works through compositor keybinds that call `durstctl`.
5. **Spec first.** The
   [Desktop Notifications Spec 1.2](https://specifications.freedesktop.org/notification-spec/latest/)
   is implemented before extras.
6. **Every visible behavior has a visual scenario** (section 6) that checks
   it by measuring screenshots, not by trusting durst's own state.

## 2. Decisions

| Topic | Decision (as built) |
|---|---|
| Notification surfaces | One layer surface **per notification**; durst predicts each height, stacks them with margins and reflows on changes |
| Output | `focused` (default: the focused window's output, the stack stays there until empty), `all`, or `name:<output>` |
| Content | Body markup, icons & images, progress bar (`value` hint), action buttons, duplicate counter |
| Behavior | Urgency-based timeouts and styles; timers pause while hovered, idle or locked; modes (DND etc.) |
| History | In memory as a ring buffer, popable (dunst-like); not persisted |
| Rules | dunst-style `[[rule]]`s: match, rewrite, restyle, hide/skip/defer, fullscreen policy, actions, scripts, sounds |
| Mouse | dunst defaults, every button configurable |
| Keyboard | Never focused; CLI only |
| Config | TOML at `$XDG_CONFIG_HOME/durst/config.toml`, watched and reloaded; errors are shown as a notification |
| Theming | Structured style keys, per urgency and per rule (tables merged over the base style) |
| D-Bus | `zbus` 5 (async, pure Rust) |
| IPC | Own D-Bus interface `org.durst_notification.Durst1` on the session bus |
| CLI | Own noun–verb design (not dunstctl-compatible); no bar subscribe stream |
| Volume | Native PipeWire (`pipewire-rs` 0.10): default sink and source, volume + mute, through device routes like WirePlumber's mixer |
| Volume OSD | Interactive: slider (click, drag, scroll), mute button; stays open while hovered |
| Media | MPRIS; OSD on song change with cover, title, artist and prev / play-pause / next |
| Sound | A configurable player command (default `pw-play`), freedesktop sound themes |
| Distribution | Nix flake package, systemd user unit, D-Bus activation for both bus names |

## 3. Architecture

### 3.1 Workspace layout

```
durst/                daemon
  src/
    main.rs           arg parsing, logging, config loading, start
    app.rs            the iced_layershell daemon: messages, surfaces (App::sync),
                      control commands, OSD state
    config.rs         TOML schema, defaults, style merging, validation
    config_watch.rs   hot reload (notify crate, watches the directory)
    effects.rs        spawning scripts, sounds (sound theme lookup), browser
    wayland.rs        own Wayland connection: idle, fullscreen + focused output
                      (wlr-foreign-toplevel), output names
    logind.rs         session lock state (LockedHint)
    audio.rs          PipeWire thread: default nodes, volume, mute, routes
    mpris.rs          MPRIS client: players, metadata, remote control
    core/             pure logic
      notification.rs Notification model, urgency, typed hints, image data
      store.rs        displayed / waiting / held entries, timers, sorting
      layout.rs       stack margins
      history.rs      ring buffer
      rules.rs        matching + rule actions, modes, fullscreen policy
      media.rs        players and the active one
    dbus/
      mod.rs          one connection, both bus names
      notifications.rs org.freedesktop.Notifications server, hint parsing
      control.rs      org.durst_notification.Durst1 server
    ui/
      notification.rs notification view + height prediction, "+N more"
      osd.rs          volume and media OSD views + height prediction
      markup.rs       spec markup → rich text runs
      icons.rs        icon lookup (freedesktop-icons), raster pre-scaling
      cover.rs        cover art loading (file / http)
durstctl/             CLI (clap), thin client over Durst1
durst-proto/          shared: proxies and serializable types
tools/                test helpers: vpointer, fake-logind, fake-player
contrib/              example config, systemd unit, D-Bus service files
scripts/visual/       visual test harness and scenarios
```

### 3.2 Runtime data flow

```
 D-Bus (Notify, durstctl) ──┐
 PipeWire thread ───────────┤
 MPRIS task ────────────────┤
 Wayland thread ────────────┼──► Message ──► App::update ──► state (Store, History,
 logind task ───────────────┤                    │            Players, volumes, OSDs)
 config watcher ────────────┤                    ▼
 timer (only while needed) ─┤               App::sync: open / move / resize / close
 mouse (view events) ───────┘               surfaces, publish status, emit signals
```

- `App::update` handles a message, then `App::sync` reconciles the open
  surfaces with the state. Surfaces only get changes after their `Opened`
  event (R8).
- `Notify` is answered inside the zbus task (id from an atomic counter); the
  notification reaches the UI loop over a channel. Control commands are
  answered by the UI loop over a oneshot channel; counts and modes are a
  shared snapshot, published with `PropertiesChanged`.
- Blocking or long work runs elsewhere: PipeWire and Wayland on own threads,
  MPRIS/logind/config watching as async tasks, scripts/sounds/browser as
  child processes, covers in `spawn_blocking`.
- PipeWire, MPRIS and logind reconnect by themselves when their connection
  ends or isn't there at startup (`retry.rs`: 1 s, doubling up to 30 s; only
  the first failure in a row is a warning).

### 3.3 Surfaces

`Key` = `Notification(id)`, `More` ("+N more"), `VolumeOsd`, `MediaOsd`; with
`output = "all"` each key has one surface per output.

- **Stack:** fixed width, predicted height (R1), anchored to the configured
  corner, `offset + Σ(height + gap)` along the stacking axis. Beyond
  `max_visible`, notifications wait and a "+N more" surface follows the stack.
- **OSDs:** own anchor, offset and width; one instance each; repeated
  triggers restart their timeout; hovering keeps them.
- **Output:** `focused` pins the stack to the focused window's output until
  the stack is empty (the compositor decides without a focused window); OSDs
  open where the focus is. Surfaces on a removed output are reopened.

## 4. Features (as built)

### 4.1 Notification spec

- Methods `Notify`, `CloseNotification`, `GetCapabilities`,
  `GetServerInformation`; signals `NotificationClosed` (1 expired,
  2 dismissed, 3 closed by the sender, 4 undefined: replaced or skipped) and
  `ActionInvoked`.
- `replaces_id` updates in place; `expire_timeout` -1 / 0 / ms, overridable
  with `ignore_dbus_timeout`.
- Capabilities: `actions`, `body`, `body-hyperlinks`, `body-markup`,
  `icon-static`, `sound`, `x-dunst-stack-tag`.
- Hints: `urgency`, `category`, `desktop-entry`, `image-data` (also
  `image_data`, `icon_data`), `image-path` (also `image_path`), `sound-file`,
  `sound-name`, `suppress-sound`, `transient`, `resident`, `value`,
  `x-dunst-stack-tag`, `x-canonical-private-synchronous`.

### 4.2 Display

- Icon (left, right or off), summary (bold, with "(N)" for duplicates), body,
  progress bar, action buttons (`default` runs on click instead).
- Markup: `<b> <i> <u> <a href> <img alt> <br>` and entities; unknown tags are
  dropped, text that isn't a tag stays text. Links are clickable.
- Icons: `image-data` → `image-path` → `app_icon` → `desktop-entry` icon →
  rule `default_icon`; theme lookup with GTK's theme or `icon_theme`, hicolor
  fallback; raster images are scaled to `icon_size` by durst (R7).
- Duplicates increase a counter; stack tags replace the older notification.
- Sorting by urgency and/or arrival (`sort_by_urgency`, `newest_first`).

### 4.3 Mouse

Per button (`left`, `middle`, `right`, `scroll_up`, `scroll_down`) a list of
`none`, `close_current`, `close_all`, `do_action`, `open_url`. Defaults as in
dunst: left closes, middle invokes the default action and closes, right
closes all.

### 4.4 Modes & DND

A set of active modes, empty at start. Rules match `mode` / `not_mode`;
`defer = true` holds a notification (not shown, not counted as waiting, no
timer, no arrival side effects). Changing modes or the config runs all rules
again on the notifications as originally received.

### 4.5 Rules

Evaluated in order; each sees the notification as earlier rules left it.

| Group | Keys |
|---|---|
| Matchers | `app_name`, `summary`, `body`, `icon`, `category`, `desktop_entry`, `stack_tag` (regexes, each also `not_…`), `urgency`, `not_urgency`, `transient`, `has_actions`, `mode`, `not_mode`, `fullscreen_active` |
| Content | `set_summary`, `set_body` (templates: `{app_name} {summary} {body} {category} {urgency}`), `hide_body`, `set_urgency`, `set_category`, `set_stack_tag`, `set_icon`, `set_transient` |
| Display | `style`, `default_icon`, `icon_position`, `timeout` |
| Visibility | `skip_display`, `history_ignore`, `defer`, `fullscreen = show \| delay \| pushback` |
| Actions | `default_action`, `auto_invoke` |
| Side effects | `script` + `script_on = receive \| action \| close` (`DURST_*` variables), `sound`, `mute_sound` |

### 4.6 History

`[history] length` (default 20) expired or dismissed notifications, except
transient or `history_ignore`d ones; `history pop` shows the newest again
(sticky by default), without running the rules again.

### 4.7 Environment awareness

- Idle (`ext-idle-notify-v1`, `idle_threshold`) and lock (logind
  `LockedHint`) pause all timers.
- Fullscreen (focused window, wlr-foreign-toplevel) drives the rules'
  fullscreen policy and the `fullscreen_active` matcher.
- The focused window's output drives `output = "focused"`.

### 4.8 Volume OSD

PipeWire thread following the `default` metadata, node `Props` and device
`Route`s; percent on the cubic scale of pavucontrol/wpctl. OSD: device name,
slider (up to `max_volume`), percent, mute button (fixed width), scroll by
`step`; shown on `durstctl volume` and (configurable) on external changes;
`--mic` for the default source.

### 4.9 Media OSD

MPRIS players from startup and `NameOwnerChanged`; the active player is the
one that most recently started playing, else the last active. OSD: cover
(`file://` or `http(s)://`, cached, placeholder while loading), title,
artist, previous / play-pause / next (one width); shown on a change of the
active song and on `durstctl media`.

### 4.10 CLI – `durstctl`

```
durstctl notif   list [--json] | count [--waiting|--held] | close [ID] | close-all
                 action [ID] [KEY]
durstctl history list [--json] | pop | clear | count
durstctl mode    list [--all] | set <M>... | enable <M> | disable <M> | toggle <M>
durstctl volume  get [--mic] [--json] | set <N|+N|-N>[%] [--mic] | up | down [--mic]
                 mute [on|off|toggle] [--mic]
durstctl media   status [--json] | play | pause | toggle | next | prev
durstctl osd     show <volume|mic|media>
durstctl reload
durstctl info [--json]     # version, config, counts, modes, idle, locked,
                           # fullscreen, outputs, focused output
```

Exit codes: 0 ok, 1 durst not running, 2 invalid argument, 3 nothing to act
on, 4 invalid config, 5 the running durst speaks another interface version. Man pages and shell completions are generated by the
build.

### 4.11 Control D-Bus interface

`org.durst_notification.Durst1` at `/org/durst_notification/Durst` on the bus
name `org.durst_notification.Durst`: the methods behind every CLI command,
properties `DisplayedCount`, `WaitingCount`, `HeldCount`, `HistoryCount` and
`ActiveModes` with `PropertiesChanged`, errors `NotFound`, `InvalidConfig`,
`InvalidArgument` (prefix `org.durst_notification.Error.`).

### 4.12 Configuration

`contrib/config.toml` lists every option with its default and is parsed by
a unit test, so it can't drift from the code. Sections: `[general]`,
`[history]`, `[mouse]`, `[style]`, `[urgency.low|normal|critical]`
(`timeout`, `style`), `[sound]`, `[osd.volume]`, `[osd.media]`, `[[rule]]`.

## 5. Milestones

| | Milestone | Status |
|---|---|---|
| M0 | Foundation: zbus, single daemon, TOML, iced 0.14 | ✅ 2026-10-01 |
| M1 | Notification parity with dunst | ✅ 2026-10-02 (daily-driver test running) |
| M2 | `durstctl`, control interface, history | ✅ 2026-10-02 |
| M3 | Rules & modes | ✅ 2026-10-02 |
| M4 | Idle, lock, fullscreen, `output = "all"` | ✅ 2026-10-02 |
| M5 | Hot reload, systemd, D-Bus activation, package | ✅ 2026-10-02 |
| M6 | Volume OSD (PipeWire) | ✅ 2026-10-02 |
| M7 | Media OSD (MPRIS) | ✅ 2026-10-02 |
| — | Fixes from daily use: endless config reloads; focused output didn't follow a newly connected monitor | ✅ 2026-10-03 |
| M8 | Polish and the missing parts (section 8) | open |

Notes on deviations, per milestone:

- **M0:** `winit-core`/`winit-common` pinned to `0.31.0-beta.2`
  (`iced_exdevtools` 0.19.1 doesn't build against beta.3); iced_layershell
  without default features (its theme detection blocks startup).
- **M1:** icon lookup switched from the unmaintained `linicon` (no hicolor
  fallback) to `freedesktop-icons`; raster icons pre-scaled (R7).
- **M2:** fixed R8 (changes to surfaces not created yet were dropped).
- **M3:** rewrite actions are named `set_*` (the plan's `icon`/`category`
  collided with the matchers); `format` became `set_summary`/`set_body`;
  sound via a player command instead of `rodio`; per-rule anchor/output not
  done.
- **M4:** lock state from logind's real session path (signals aren't sent
  for `session/auto`).
- **M5:** dev profile with line tables only (debug builds were ~450 MB each
  and filled the disk).
- **M6:** a node info update only carries what changed; applying it blindly
  lost the node's name after every volume change. The mute button has a
  fixed width so the slider doesn't jump.
- **M7:** the OSD ignores repeated metadata (e.g. the cover arriving later)
  and players found at startup.
- **Daily use (2026-10-03):** inotify also reports reads, so every reload
  triggered the next one; `LastOutput` turned out to be "durst's last
  clicked surface, else the first output", replaced by the focused window's
  output (R4).

## 6. Testing

- **Unit (55 tests):** core logic with injected time (timers, sorting,
  duplicates, stack tags, held entries, history, rules, modes, media players,
  layout), config parsing and errors, markup, hint and metadata parsing, pod
  encoding, sound and cover helpers.
- **Visual (35 scenarios):** `scripts/visual/run.sh` runs durst in an
  isolated headless sway with a private D-Bus session, a fake logind, a
  private PipeWire with a null sink and source, and fake MPRIS players; mouse
  input via a virtual pointer. Scenarios send notifications, click, drag,
  scroll, change volumes, switch focus and outputs, and verify with
  screenshot measurements (box geometry, padding, pixel colors), D-Bus
  signals, `durstctl`, `pw-dump` and the players' call logs.
  `scripts/visual/run-all.sh` runs all; `REPEAT=N` catches races.
- **CI:** `.github/workflows/rust.yml` runs fmt, clippy, tests, all visual
  scenarios and the package build via nix; passes on GitHub (PR #1 in the
  fork).
- **Manual:** `scripts/notify-test.sh`; daily use on sway.

## 7. Risks

| # | Risk | Status |
|---|---|---|
| R1 | Surface heights must be known before positioning | Resolved: predicted with iced's `Paragraph`; every view element exists in `view` and `height`, checked by padding measurements |
| R2 | iced_layershell API churn | Versions pinned; happened once (winit-core beta) |
| R3 | Compositors without wlr-foreign-toplevel (e.g. GNOME) | Fullscreen and focused output degrade gracefully (compositor decides); GNOME/KDE run their own daemons anyway |
| R4 | Choosing the focused output | Resolved 2026-10-03 (see M-notes) |
| R5 | PipeWire complexity | Resolved in M6; hardware route writes untested (section 8) |
| R6 | Slider floods PipeWire | Resolved: one update per 30 ms |
| R7 | iced_tiny_skia misplaces scaled raster images | Worked around by pre-scaling; with HiDPI output scaling it can still shift, not reported upstream yet |
| R8 | iced_layershell drops changes for surfaces not created yet | Resolved: changes wait for the `Opened` event |
| R9 | A running daemon and a newer `durstctl` disagree on the interface (seen: "Signature mismatch" from `durstctl info`) | Resolved: `durst-proto` fingerprints its interface definitions at build time, durst publishes it (`InterfaceHash`), durstctl compares on errors and asks to restart durst (exit 5); checked with a daemon built from before the change |

## 8. Missing parts

### Not verified on real systems

1. **Volume through a hardware route.** Reading matched `wpctl` on real
   hardware; *writing* through a device route is only tested on the
   sandbox's null sink (node props path). Needs one check on a real sound
   card.
2. **Real media players** (Spotify, Firefox, mpv): only the fake player is
   tested.
3. **Other compositors** (Hyprland, niri, river): only sway is tested.
4. **HiDPI** output scaling (layout and R7).
5. **Remote Nix builder:** the package build fails on the configured remote
   builder (likely its disk); it builds locally and in CI.

### Planned but not built

6. **`font` option:** the style has `font_size` only; the font family is
   the system's sans-serif.
7. **Long bodies:** no `max_lines`/ellipsis; a very long body makes a
   notification taller than the screen.
8. **Many actions:** all buttons share one row and get narrower; labels are
   clipped.
9. **Icon size range** (min/max instead of one fixed `icon_size`) and
   `icon_position = "top"`.
10. **App name** is not shown in notifications (only used by rules).
11. **Volume OSD icon** reflecting level and mute.
12. **Per-rule `anchor`/`output`** (needs one stack per anchor).
13. **`history_pop` mouse action.**
14. **`ActivationToken` signal** (xdg-activation, so actions can focus the
    app) and the `hlcolor` hint.
16. **Size limit for `image-data`** hints (covers have 10 MB, image hints
    none).
17. **Markup fuzzing** (`cargo fuzz` target).

### Decided against in the planning (could be revisited)

- Player selection and seeking in the media OSD; per-application volume and
  output device switching; persistent history; a status-bar subscribe
  stream; dunstctl-compatible commands; home-manager module; animations.

### Housekeeping

- Merge into `master` as two PRs: `iced-layers-prototype` (the 23 commits of
  the original iced-layers prototype), then `iced-layers-rework` on top (the
  14 commits of this rework; each milestone commit was checked with build,
  clippy, unit tests and the visual scenarios).
- Daily-driver test of M1's "done when": replace mako/dunst for a week.

## 9. Next steps (suggested order)

1. Finish the daily-driver test, fix what comes up (each fix with a
   scenario).
2. Verify items 1–2 on the real system (with explicit consent for the
   volume change).
3. Item 7 (robustness), then 6, 10, 11 (looks).
4. PR to `master` of durst-notification/durst (conflicts in 5 files).
