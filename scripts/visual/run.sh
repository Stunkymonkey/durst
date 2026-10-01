#!/usr/bin/env bash
# Visual test harness: runs durst inside an isolated headless sway with a
# private D-Bus session, executes a scenario, takes a screenshot with grim and
# prints the measured notification boxes as JSON.
#
# Nothing touches the real session: the running notification daemon, the real
# compositor and the screen stay untouched.
#
# usage: scripts/visual/run.sh <scenario.sh> [out.png]
#   scenario.sh is sourced inside the sandbox; notify-send/gdbus talk to durst.
#   Env: DURST_BIN (default target/debug/durst), SETTLE (seconds before the
#   screenshot, default 1.5), RESOLUTION (default 1280x720).
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../.." && pwd)"
scenario="$(realpath "$1")"
out="$(realpath -m "${2:-$root/target/visual/$(basename "$scenario" .sh).png}")"
mkdir -p "$(dirname "$out")"

if [[ -z "${DBUS_SESSION_BUS_ADDRESS_ISOLATED:-}" ]]; then
    # re-exec inside a private session bus
    exec env DBUS_SESSION_BUS_ADDRESS_ISOLATED=1 dbus-run-session -- "$0" "$@"
fi

work="$(mktemp -d)"
chmod 700 "$work"
log="$work/logs"
mkdir -p "$log"
cleanup() {
    [[ -n "${durst_pid:-}" ]] && kill "$durst_pid" 2>/dev/null || true
    [[ -n "${sway_pid:-}" ]] && kill "$sway_pid" 2>/dev/null || true
    wait 2>/dev/null || true
    if [[ "${KEEP_LOGS:-0}" == 1 ]]; then echo "logs: $log" >&2; else rm -rf "$work"; fi
}
trap cleanup EXIT

cat >"$work/sway.conf" <<EOF
output HEADLESS-1 resolution ${RESOLUTION:-1280x720}
default_border none
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
sway_pid=$!

for _ in $(seq 50); do [[ -s "$work/env" ]] && break; sleep 0.1; done
[[ -s "$work/env" ]] || { echo "sway did not start, see $log/sway.log" >&2; KEEP_LOGS=1; exit 1; }
export WAYLAND_DISPLAY="$(sed -n 1p "$work/env")"
export SWAYSOCK="$(sed -n 2p "$work/env")"

# software rendering: no GPU in headless sway, and deterministic pixels
ICED_BACKEND="${ICED_BACKEND:-tiny-skia}" LD_LIBRARY_PATH="$durst_ld" RUST_LOG="${RUST_LOG:-debug}" "${DURST_BIN:-$root/target/debug/durst}" >"$log/durst.log" 2>&1 &
durst_pid=$!

for _ in $(seq 50); do
    busctl --user status org.freedesktop.Notifications >/dev/null 2>&1 && break
    sleep 0.1
done
busctl --user status org.freedesktop.Notifications >/dev/null 2>&1 \
    || { echo "durst did not claim the bus name, see $log/durst.log" >&2; KEEP_LOGS=1; exit 1; }

# shellcheck source=/dev/null
source "$scenario"

sleep "${SETTLE:-1.5}"
grim -o HEADLESS-1 "$out"
echo "screenshot: $out" >&2
python3 "$here/measure.py" "$out"
