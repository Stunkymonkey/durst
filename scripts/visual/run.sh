#!/usr/bin/env bash
# Visual test harness: runs durst inside an isolated headless sway with a
# private D-Bus session, executes a scenario, takes a screenshot with grim and
# prints the measured notification boxes as JSON.
#
# Nothing touches the real session: the running notification daemon, the real
# compositor and the screen stay untouched.
#
# usage: scripts/visual/run.sh <scenario.sh> [out.png]
#   Env: DURST_BIN_DIR (durst, durstctl and the tools/, default target/debug),
#   DURST_BIN (default $DURST_BIN_DIR/durst), DURST_ARGS, WAIT (seconds
#   after which any wait below gives up, default 10; raise it on slow
#   machines), RESOLUTION (default 1280x720), KEEP_LOGS=1, DURST_TEST_FONTS
#   (a directory with the only fonts durst sees; the devshell sets it to
#   DejaVu, so that text has the same size on every machine).
#
# durst works asynchronously, but a scenario never sleeps to let it catch up:
# it waits for what it expects (`await`, `wait_for`, `snap EXPR`), and
# `verify` is retried until it passes or WAIT is over. A `sleep` in a
# scenario is only for time that has to pass, such as a timeout.
#
# A scenario is sourced inside the sandbox. It runs notify-send/gdbus calls and
# may use these helpers:
#   notify ARGS...      notify-send that prints the notification id
#   click X Y [BUTTON]  click on the screen (BUTTON: left, right, middle)
#   pointer X Y         move the pointer
#   scroll DY           scroll at the pointer, positive is down
#                       (input returns once the compositor has handled it)
#   signals             print the notification and control interface
#                       signals so far, one per line: MEMBER ARGS...
#   signaled LINE       whether `signals` printed LINE
#   wait_for CMD...     retry CMD until it succeeds; fails the run after WAIT
#   await VALUE CMD...  print CMD's output once it is VALUE (after WAIT, the
#                       last output, for verify to report)
#   await_match RE CMD... the same with a regular expression (whole output)
#   snap [EXPR]         screenshot + measure once the screen is still and
#                       EXPR (as for `check`) holds; `geom I KEY` then prints
#                       boxes[I][KEY] (x, y, w, h, bottom, right)
#   pixel X Y           "R G B" of the last screenshot (snap or final)
#   durstctl ...        the CLI built next to durst
#   state KEY           a field of `durstctl info --json` (idle, locked, ...)
#   lock / unlock       lock state of the fake logind durst watches
#   logind_stop / logind_start, pipewire_stop / pipewire_start: restart them
#   pw_volume NODE      "PERCENT MUTED" of test-sink / test-source, read
#                       from the private PipeWire itself
#   pw_set NODE PERCENT change a volume like another program would
#   press X Y / release mouse button held down, e.g. to drag
#   player_start NAME   a fake MPRIS player org.mpris.MediaPlayer2.NAME;
#   player NAME CMD     send it "status Playing" or "track ID|TITLE|ARTIST|URL";
#   player_calls NAME   the methods called on it so far, one line
#   player_stop NAME    and gone again
#   serve DIR           serve DIR over HTTP on 127.0.0.1, prints the base URL
#   window_on OUTPUT    open a black window on OUTPUT (which gets the focus)
#   fullscreen_window   open a black window and make it fullscreen;
#   leave_fullscreen    and close it again (it would disturb measurements)
# A line `# outputs: N` in the scenario gives sway N outputs side by side
# (HEADLESS-1, HEADLESS-2, ...); screenshots then cover all of them.
# A line `# activation` doesn't start durst: the bus starts it on demand from
# the service files in contrib/dbus, after the environment was handed over
# with dbus-update-activation-environment, like in a real session.
#   status CMD...       run CMD (stderr dropped) and print its exit code;
#                       scenarios run with `set -e`, so use this for
#                       commands that are expected to fail
# $DURST_TEST_DIR is a scratch directory, also visible to durst.
# A file `<scenario>.toml` next to the scenario is used as durst's config.
# The scenario may define `verify`, called after the screenshot with $MEASURE
# pointing to the measurement JSON; a non-zero return fails the run. Until it
# passes, the screenshot is taken again and verify retried, for up to WAIT:
# verify describes the final state, not a moment.
# `check EXPR` evaluates a Python expression over `boxes`, `gaps` and `size`.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../.." && pwd)"
bin="${DURST_BIN_DIR:-$root/target/debug}"
scenario="$(realpath "$1")"
out="$(realpath -m "${2:-$root/target/visual/$(basename "$scenario" .sh).png}")"
mkdir -p "$(dirname "$out")"
res="${RESOLUTION:-1280x720}"
outputs="$(sed -nE 's/^# outputs: *([0-9]+).*/\1/p' "$scenario" | head -1)"
outputs="${outputs:-1}"
activation="$(grep -c '^# activation' "$scenario" || true)"

if [[ -z "${DURST_VISUAL_ISOLATED:-}" ]]; then
    bus_config="$here/session.conf"
    if [[ $activation -gt 0 ]]; then
        # the real service files, with a wrapper as the binary that logs
        act="$(mktemp -d)"
        mkdir -p "$act/services" "$act/bin"
        printf '#!/bin/sh\nexec %q "$@" >>"$DURST_TEST_DIR/logs/durst.log" 2>&1\n' \
            "${DURST_BIN:-$bin/durst}" >"$act/bin/durst"
        chmod +x "$act/bin/durst"
        for f in "$root"/contrib/dbus/*.service; do
            sed "s|@bindir@|$act/bin|" "$f" >"$act/services/$(basename "$f")"
        done
        sed "s|</busconfig>|  <servicedir>$act/services</servicedir>\n</busconfig>|" \
            "$here/session.conf" >"$act/session.conf"
        bus_config="$act/session.conf"
    fi
    # re-exec inside a private session bus
    exec env DURST_VISUAL_ISOLATED=1 DURST_ACTIVATION_DIR="${act:-}" \
        dbus-run-session --config-file="$bus_config" -- "$0" "$@"
fi

work="$(mktemp -d)"
chmod 700 "$work"
log="$work/logs"
mkdir -p "$log"
cleanup() {
    if [[ $activation -gt 0 ]]; then
        # started by the bus, not by us
        busctl --user status org.freedesktop.Notifications 2>/dev/null \
            | sed -n 's/^PID=//p' | xargs -r kill 2>/dev/null || true
        rm -rf "${DURST_ACTIVATION_DIR:-/nonexistent}"
    fi
    jobs -p | xargs -r kill 2>/dev/null || true
    wait 2>/dev/null || true
    if [[ "${KEEP_LOGS:-0}" == 1 ]]; then echo "logs: $log" >&2; else rm -rf "$work"; fi
}
trap cleanup EXIT

for i in $(seq "$outputs"); do
    echo "output HEADLESS-$i resolution $res position $(( (i - 1) * ${res%x*} )) 0" >>"$work/sway.conf"
done
cat >>"$work/sway.conf" <<EOF
default_border none
xwayland disable
exec sh -c 'printf "%s\n%s\n" "\$WAYLAND_DISPLAY" "\$SWAYSOCK" > $work/env'
EOF

export XDG_RUNTIME_DIR="$work"
unset WAYLAND_DISPLAY DISPLAY SWAYSOCK
# the nix devshell's LD_LIBRARY_PATH is only meant for durst; system tools
# (sway, grim) break with its older libwayland
durst_ld="${LD_LIBRARY_PATH:-}"
unset LD_LIBRARY_PATH
WLR_BACKENDS=headless WLR_HEADLESS_OUTPUTS="$outputs" WLR_RENDERER=pixman WLR_LIBINPUT_NO_DEVICES=1 \
    sway -c "$work/sway.conf" >"$log/sway.log" 2>&1 &

for _ in $(seq 50); do [[ -s "$work/env" ]] && break; sleep 0.1; done
[[ -s "$work/env" ]] || { echo "sway did not start, see $log/sway.log" >&2; KEEP_LOGS=1; exit 1; }
export WAYLAND_DISPLAY="$(sed -n 1p "$work/env")"
export SWAYSOCK="$(sed -n 2p "$work/env")"

# headless sway has no input devices: a virtual pointer that lives for the
# whole run, created before durst so its seat already has a pointer
mkfifo "$work/pointer" "$work/pointer-ack"
# opened read-write, so that vpointer's open doesn't wait for a reader
exec 6<>"$work/pointer-ack"
LD_LIBRARY_PATH="$durst_ld" "$bin/vpointer" \
    "$(( ${res%x*} * outputs ))" "${res#*x}" <"$work/pointer" >"$work/pointer-ack" 2>"$log/vpointer.log" &
exec 3>"$work/pointer"
WAIT="${WAIT:-10}"
wait_for() {
    local deadline=$((SECONDS + WAIT))
    until "$@" >/dev/null 2>&1; do
        if ((SECONDS >= deadline)); then
            echo "gave up waiting for: $*" >&2
            return 1
        fi
        sleep 0.1
    done
}
# await VALUE CMD...: CMD's output once it is VALUE, else the last one
await() {
    local want=$1 got deadline=$((SECONDS + WAIT))
    shift
    while got=$("$@" 2>/dev/null) || true; [[ $got != "$want" ]] && ((SECONDS < deadline)); do
        sleep 0.1
    done
    printf '%s\n' "$got"
}
await_match() {
    local re=$1 got deadline=$((SECONDS + WAIT))
    shift
    while got=$("$@" 2>/dev/null) || true; ! [[ $got =~ ^($re)$ ]] && ((SECONDS < deadline)); do
        sleep 0.1
    done
    printf '%s\n' "$got"
}
# vpointer CMD...: send commands and wait until the compositor has them
vpointer() {
    printf '%s\n' "$@" >&3
    local _cmd _ack
    for _cmd in "$@"; do
        read -r -t "$WAIT" _ack <&6 \
            || { echo "vpointer does not answer, see $log/vpointer.log" >&2; return 1; }
    done
}
wait_for sh -c "swaymsg -t get_seats -r | grep -q '\"capabilities\": [1-9]'" \
    || { echo "virtual pointer missing, see $log/vpointer.log" >&2; KEEP_LOGS=1; exit 1; }

# a fake logind on the private bus: durst never sees the real lock state
logind_start() {
    rm -f "$work/logind"
    mkfifo "$work/logind"
    LD_LIBRARY_PATH="$durst_ld" "$bin/fake-logind" <"$work/logind" >>"$log/logind.log" 2>&1 &
    logind_pid=$!
    exec 4>"$work/logind"
    wait_for busctl --user status org.freedesktop.login1 \
        || { echo "fake logind missing, see $log/logind.log" >&2; KEEP_LOGS=1; exit 1; }
}
logind_stop() {
    exec 4>&-
    kill "$logind_pid"
    wait_for sh -c '! busctl --user status org.freedesktop.login1'
}
logind_start
export DURST_LOGIND_BUS_ADDRESS="$DBUS_SESSION_BUS_ADDRESS"

# a private PipeWire with a null sink and source (scripts/visual/pipewire.conf);
# its socket is in the sandbox's XDG_RUNTIME_DIR, the real audio is untouched
pipewire_start() {
    pipewire -c "$here/pipewire.conf" >>"$log/pipewire.log" 2>&1 &
    pipewire_pid=$!
    wait_for test -S "$XDG_RUNTIME_DIR/pipewire-0" \
        || { echo "pipewire missing, see $log/pipewire.log" >&2; KEEP_LOGS=1; exit 1; }
    wait_for pw-metadata -n default 0 default.audio.sink '{ "name": "test-sink" }'
    pw-metadata -n default 0 default.audio.source '{ "name": "test-source" }' >/dev/null
}
pipewire_stop() {
    kill "$pipewire_pid"
    wait_for sh -c "! test -S '$XDG_RUNTIME_DIR/pipewire-0'"
}
pipewire_start

dbus-monitor --session "type='signal',interface='org.freedesktop.Notifications'" \
    "type='signal',path='/org/durst_notification/Durst'" \
    >"$work/signals" 2>/dev/null &

export DURST_TEST_DIR="$work"
# scenarios can provide icon themes, sounds and .desktop files in
# $DURST_TEST_DIR/share; the system's (other icon themes) stay out
mkdir -p "$work/share"
export XDG_DATA_DIRS="$work/share"
# text sizes depend on the font, and the default font differs between
# machines: durst only sees the fonts in DURST_TEST_FONTS
if [[ -n "${DURST_TEST_FONTS:-}" ]]; then
    printf '<?xml version="1.0"?>\n<fontconfig>\n  <dir>%s</dir>\n  <cachedir>%s</cachedir>\n</fontconfig>\n' \
        "$DURST_TEST_FONTS" "$work/fontconfig" >"$work/fonts.conf"
    export FONTCONFIG_FILE="$work/fonts.conf"
else
    echo "warning: DURST_TEST_FONTS is not set, sizes depend on this machine's fonts" >&2
fi
# never read the real user's durst or GTK config; the directory exists so
# durst can watch it for a config file appearing
export XDG_CONFIG_HOME="$work/config"
mkdir -p "$XDG_CONFIG_HOME/durst"
# durstctl for the scenarios
export PATH="$bin:$PATH"
config="${scenario%.sh}.toml"
if [[ -f "$config" ]]; then DURST_ARGS="-c $config ${DURST_ARGS:-}"; fi

# software rendering: no GPU in headless sway, and deterministic pixels
export ICED_BACKEND="${ICED_BACKEND:-tiny-skia}" RUST_LOG="${RUST_LOG:-durst=debug}"
if [[ $activation -gt 0 ]]; then
    # what a session's startup does (e.g. in the sway config), so that
    # services started by the bus find the display
    LD_LIBRARY_PATH="$durst_ld" dbus-update-activation-environment --all
else
    # shellcheck disable=SC2086
    LD_LIBRARY_PATH="$durst_ld" "${DURST_BIN:-$bin/durst}" ${DURST_ARGS:-} \
        >"$log/durst.log" 2>&1 &
    wait_for busctl --user status org.freedesktop.Notifications \
        || { echo "durst did not claim the bus name, see $log/durst.log" >&2; KEEP_LOGS=1; exit 1; }
    # connected to the private PipeWire as well
    wait_for "$bin/durstctl" volume get \
        || { echo "durst has no volume, see $log/durst.log" >&2; KEEP_LOGS=1; exit 1; }
fi

notify() { notify-send -p "$@"; }
status() { "$@" 2>/dev/null && echo 0 || echo $?; }
# send SUMMARY BODY [ACTIONS] [HINTS] [TIMEOUT] via gdbus (non-blocking, unlike
# notify-send -A); ACTIONS/HINTS in GVariant text format, prints the id
send() {
    local actions="${3:-[]}" hints='@a{sv} {}' timeout="${5:--1}"
    [[ -n "${4:-}" ]] && hints="$4"
    gdbus call --session --dest org.freedesktop.Notifications \
        --object-path /org/freedesktop/Notifications \
        --method org.freedesktop.Notifications.Notify -- \
        test 0 "" "$1" "$2" "$actions" "$hints" "$timeout" | sed -E 's/.*uint32 ([0-9]+).*/\1/'
}
# one line per signal: "<member> <args...>"
signals() {
    awk '/^signal/ { if (line) print line; match($0, /member=[A-Za-z]+/); line = substr($0, RSTART + 7, RLENGTH - 7); next }
         /^ +(uint32|string)/ { $1 = ""; line = line $0 }
         /^ +variant +(uint32|string)/ { $1 = ""; $2 = ""; line = line " " $0 }
         END { if (line) print line }' "$work/signals"
}
signaled() { signals | grep -qxF -- "$1"; }
click() { vpointer "move $1 $2" "click ${3:-left}"; }
press() { vpointer "move $1 $2" "press left"; }
release() { vpointer "move $1 $2" "release left"; }
pw_node() {
    pw-dump | python3 -c "
import json, sys
for o in json.load(sys.stdin):
    i = o.get('info') or {}
    if (i.get('props') or {}).get('node.name') == sys.argv[1]:
        props = [p for p in (i.get('params') or {}).get('Props', []) if 'channelVolumes' in p]
        print(o['id'], round(max(props[0]['channelVolumes']) ** (1 / 3) * 100), props[0]['mute'])
" "$1"
}
pw_volume() { pw_node "$1" | cut -d' ' -f2-; }
pw_set() {
    local id linear
    id=$(pw_node "$1" | cut -d' ' -f1)
    linear=$(python3 -c "print(($2 / 100) ** 3)")
    pw-cli set-param "$id" Props "{ channelVolumes: [ $linear, $linear ] }" >/dev/null
}
declare -A player_fds player_pids
player_start() {
    mkfifo "$work/player-$1"
    "$bin/fake-player" "$1" <"$work/player-$1" >"$work/player-$1.calls" 2>>"$log/players.log" &
    player_pids[$1]=$!
    local fd
    exec {fd}>"$work/player-$1"
    player_fds[$1]=$fd
    wait_for busctl --user status "org.mpris.MediaPlayer2.$1"
}
player() {
    local name=$1
    shift
    echo "$*" >&"${player_fds[$name]}"
}
player_calls() { paste -sd' ' "$work/player-$1.calls"; }
player_stop() {
    kill "${player_pids[$1]}"
    wait_for sh -c "! busctl --user status org.mpris.MediaPlayer2.$1"
}
serve() {
    python3 -u -m http.server --bind 127.0.0.1 --directory "$1" 0 >"$work/http.log" 2>&1 &
    wait_for grep -q 'port' "$work/http.log"
    echo "http://127.0.0.1:$(sed -nE 's/.*port ([0-9]+).*/\1/p' "$work/http.log" | head -1)"
}
pointer() { vpointer "move $1 $2"; }
scroll() { vpointer "scroll $1"; }
screenshot() {
    if [[ $outputs == 1 ]]; then grim -o HEADLESS-1 "$1"; else grim "$1"; fi
}
# a screenshot once the screen is still: twice the same in a row
still_screenshot() {
    local prev="$work/still.png" deadline=$((SECONDS + WAIT))
    screenshot "$prev"
    while sleep 0.1; screenshot "$1"; ! cmp -s "$prev" "$1"; do
        ((SECONDS < deadline)) || { echo "the screen does not stand still" >&2; return; }
        mv "$1" "$prev"
    done
}
# holds EXPR JSON: whether EXPR holds for a measurement
holds() {
    python3 -c '
import json, sys
m = json.load(open(sys.argv[2]))
boxes, gaps, size = m["boxes"], m["gaps"], m["size"]
sys.exit(0 if eval(sys.argv[1]) else 1)' "$1" "$2"
}
state() {
    durstctl info --json | python3 -c "import json,sys; print(json.load(sys.stdin)[sys.argv[1]])" "$1"
}
# black windows: black text (foot may print warnings into the window) and
# a hidden cursor (unfocused, foot draws it hollow in white whatever its
# color)
printf '#!/bin/sh\nprintf "\\033[?25l"\nexec sleep 600\n' >"$work/blank"
chmod +x "$work/blank"
is_state() { [[ "$(state "$1")" == "$2" ]]; }
# these return once durst has noticed (if it does at all)
lock() { echo lock >&4; await True state locked >/dev/null; }
unlock() { echo unlock >&4; await False state locked >/dev/null; }
window_on() {
    local id="durst-w$RANDOM"
    swaymsg -q focus output "$1"
    swaymsg -q exec "foot --app-id $id -o colors-dark.background=000000 \
        -o colors-dark.foreground=000000 -o tweak.font-monospace-warn=no $work/blank"
    wait_for sh -c "swaymsg -t get_tree | grep -q '\"app_id\": \"$id\"'"
    wait_for is_state focused_output "$1"
}
fullscreen_window() {
    swaymsg -q exec "foot --app-id durst-test -o colors-dark.background=000000 \
        -o colors-dark.foreground=000000 -o tweak.font-monospace-warn=no $work/blank"
    wait_for sh -c "swaymsg -t get_tree | grep -q '\"app_id\": \"durst-test\"'"
    swaymsg -q '[app_id=durst-test] fullscreen enable'
    wait_for is_state fullscreen True
}
leave_fullscreen() {
    swaymsg -q '[app_id=durst-test] fullscreen disable'
    wait_for is_state fullscreen False
    # gone, so it doesn't disturb the screenshot
    swaymsg -q '[app_id=durst-test] kill'
    wait_for sh -c "! swaymsg -t get_tree | grep -q '\"app_id\": \"durst-test\"'"
}
last_png=""
snap() {
    local expr="${1:-True}" deadline=$((SECONDS + WAIT))
    last_png="$work/snap.png"
    while still_screenshot "$last_png"
        python3 "$here/measure.py" "$last_png" >"$work/snap.json"
        ! holds "$expr" "$work/snap.json"; do
        ((SECONDS < deadline)) || { echo "snap: gave up waiting for $expr" >&2; return; }
        sleep 0.1
    done
}
geom() {
    python3 -c "import json,sys; b=json.load(open('$work/snap.json'))['boxes'][int(sys.argv[1])]
b['bottom'] = b['y'] + b['h'] - 1; b['right'] = b['x'] + b['w'] - 1
print(b[sys.argv[2]])" "$1" "$2"
}
pixel() {
    python3 -c "from PIL import Image; import sys
print(*Image.open('$last_png').convert('RGB').getpixel((int(sys.argv[1]), int(sys.argv[2]))))" "$1" "$2"
}
check() {
    python3 - "$1" <<EOF
import json, sys
m = json.load(open("$MEASURE"))
boxes, gaps, size = m["boxes"], m["gaps"], m["size"]
ok = eval(sys.argv[1])
print(("PASS" if ok else "FAIL") + ": " + sys.argv[1])
sys.exit(0 if ok else 1)
EOF
}

# shellcheck source=/dev/null
source "$scenario"

export MEASURE="${out%.png}.json"
final() {
    still_screenshot "$out"
    last_png="$out"
    python3 "$here/measure.py" "$out" >"$MEASURE"
}
final
if declare -F verify >/dev/null; then
    # retried on a new screenshot; only the last attempt's output is shown
    deadline=$((SECONDS + WAIT))
    until verify >"$work/verify.out" 2>&1; do
        if ((SECONDS >= deadline)); then
            cat "$work/verify.out" "$MEASURE"
            echo "screenshot: $out" >&2
            echo "verify failed" >&2
            KEEP_LOGS=1
            exit 1
        fi
        sleep 0.2
        final
    done
    cat "$work/verify.out"
fi
echo "screenshot: $out" >&2
cat "$MEASURE"
