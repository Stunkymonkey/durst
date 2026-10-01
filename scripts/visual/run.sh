#!/usr/bin/env bash
# Visual test harness: runs durst inside an isolated headless sway with a
# private D-Bus session, executes a scenario, takes a screenshot with grim and
# prints the measured notification boxes as JSON.
#
# Nothing touches the real session: the running notification daemon, the real
# compositor and the screen stay untouched.
#
# usage: scripts/visual/run.sh <scenario.sh> [out.png]
#   Env: DURST_BIN (default target/debug/durst), DURST_ARGS, SETTLE (seconds
#   before the screenshot, default 1), RESOLUTION (default 1280x720),
#   KEEP_LOGS=1.
#
# A scenario is sourced inside the sandbox. It runs notify-send/gdbus calls and
# may use these helpers:
#   notify ARGS...      notify-send that prints the notification id
#   click X Y [BUTTON]  click on the screen (BUTTON: left, right, middle)
#   signals             print the org.freedesktop.Notifications signals so far
#   wait_for CMD...     retry CMD for up to 3s
# It may define `verify`, called after the screenshot with $MEASURE pointing
# to the measurement JSON; a non-zero return fails the run. `check EXPR`
# evaluates a Python expression over `boxes`, `gaps` and `size`.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../.." && pwd)"
scenario="$(realpath "$1")"
out="$(realpath -m "${2:-$root/target/visual/$(basename "$scenario" .sh).png}")"
mkdir -p "$(dirname "$out")"
res="${RESOLUTION:-1280x720}"

if [[ -z "${DURST_VISUAL_ISOLATED:-}" ]]; then
    # re-exec inside a private session bus
    exec env DURST_VISUAL_ISOLATED=1 dbus-run-session --config-file="$here/session.conf" -- "$0" "$@"
fi

work="$(mktemp -d)"
chmod 700 "$work"
log="$work/logs"
mkdir -p "$log"
cleanup() {
    jobs -p | xargs -r kill 2>/dev/null || true
    wait 2>/dev/null || true
    if [[ "${KEEP_LOGS:-0}" == 1 ]]; then echo "logs: $log" >&2; else rm -rf "$work"; fi
}
trap cleanup EXIT

cat >"$work/sway.conf" <<EOF
output HEADLESS-1 resolution $res
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
WLR_BACKENDS=headless WLR_RENDERER=pixman WLR_LIBINPUT_NO_DEVICES=1 \
    sway -c "$work/sway.conf" >"$log/sway.log" 2>&1 &

for _ in $(seq 50); do [[ -s "$work/env" ]] && break; sleep 0.1; done
[[ -s "$work/env" ]] || { echo "sway did not start, see $log/sway.log" >&2; KEEP_LOGS=1; exit 1; }
export WAYLAND_DISPLAY="$(sed -n 1p "$work/env")"
export SWAYSOCK="$(sed -n 2p "$work/env")"

# headless sway has no input devices: a virtual pointer that lives for the
# whole run, created before durst so its seat already has a pointer
mkfifo "$work/pointer"
LD_LIBRARY_PATH="$durst_ld" "${VPOINTER_BIN:-$root/target/debug/vpointer}" \
    "${res%x*}" "${res#*x}" <"$work/pointer" >"$log/vpointer.log" 2>&1 &
exec 3>"$work/pointer"
wait_for() {
    for _ in $(seq 30); do "$@" >/dev/null 2>&1 && return 0; sleep 0.1; done
    "$@"
}
wait_for sh -c "swaymsg -t get_seats -r | grep -q '\"capabilities\": [1-9]'" \
    || { echo "virtual pointer missing, see $log/vpointer.log" >&2; KEEP_LOGS=1; exit 1; }

dbus-monitor --session "type='signal',interface='org.freedesktop.Notifications'" \
    >"$work/signals" 2>/dev/null &

# software rendering: no GPU in headless sway, and deterministic pixels
# shellcheck disable=SC2086
ICED_BACKEND="${ICED_BACKEND:-tiny-skia}" LD_LIBRARY_PATH="$durst_ld" RUST_LOG="${RUST_LOG:-durst=debug}" \
    "${DURST_BIN:-$root/target/debug/durst}" ${DURST_ARGS:-} >"$log/durst.log" 2>&1 &

wait_for busctl --user status org.freedesktop.Notifications \
    || { echo "durst did not claim the bus name, see $log/durst.log" >&2; KEEP_LOGS=1; exit 1; }

notify() { notify-send -p "$@"; }
# one line per signal: "<member> <args...>"
signals() {
    awk '/^signal/ { if (line) print line; match($0, /member=[A-Za-z]+/); line = substr($0, RSTART + 7, RLENGTH - 7); next }
         /^ +(uint32|string)/ { $1 = ""; line = line $0 }
         END { if (line) print line }' "$work/signals"
}
click() {
    printf 'move %s %s\nclick %s\n' "$1" "$2" "${3:-left}" >&3
    sleep 0.1
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

sleep "${SETTLE:-1}"
grim -o HEADLESS-1 "$out"
export MEASURE="${out%.png}.json"
python3 "$here/measure.py" "$out" >"$MEASURE"
echo "screenshot: $out" >&2
cat "$MEASURE"

if declare -F verify >/dev/null; then
    verify || { echo "verify failed" >&2; KEEP_LOGS=1; exit 1; }
fi
