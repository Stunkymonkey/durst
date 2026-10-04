# An invoked action comes with an xdg-activation token: ActivationToken is
# emitted before ActionInvoked, which comes before NotificationClosed. The
# app (fake-window, on another workspace) activates its window with it. The
# token has no input serial (see wayland.rs), so sway marks the window
# urgent instead of focusing it.
mkfifo "$DURST_TEST_DIR/app"
LD_LIBRARY_PATH="$durst_ld" "$root/target/debug/fake-window" durst-app \
    <"$DURST_TEST_DIR/app" >"$DURST_TEST_DIR/app.log" 2>&1 &
exec 5>"$DURST_TEST_DIR/app"
wait_for grep -q mapped "$DURST_TEST_DIR/app.log"
# out of the screenshot's way, on another workspace: activation switches there
swaymsg -q '[app_id=durst-app] move to workspace 2'
window_on HEADLESS-1                    # takes the focus from the app
urgent() { swaymsg -t get_tree | python3 -c '
import json, sys
def walk(n):
    if n.get("app_id") == "durst-app": return n.get("urgent")
    for c in n.get("nodes", []) + n.get("floating_nodes", []):
        r = walk(c)
        if r is not None: return r
print(walk(json.load(sys.stdin)))'; }
before=$(urgent)

id=$(send "Open me" "the app should come to the front" '["open", "Open"]' '{"desktop-entry": <"durst-app">}')
sleep 0.3
snap
click "$(( $(geom -1 x) + $(geom -1 w) / 2 ))" "$(( $(geom -1 bottom) - 26 ))"
for _ in $(seq 20); do signals | grep -q "^ActionInvoked $id " && break; sleep 0.1; done
token=$(signals | sed -nE "s/^ActivationToken $id \"(.*)\"$/\1/p")
echo "$token" >&5
sleep 0.5
after=$(urgent)
# the screenshot only needs the notifications gone
swaymsg -q '[app_id=durst-app] kill'; swaymsg -q '[app_id="^durst-w"] kill'
sleep 0.3

verify() {
    signals
    echo "app urgent before: $before, after: $after, token: '$token'"
    [[ $before == False && -n $token && $after == True ]] &&
    [[ "$(signals | grep -E "^(ActivationToken|ActionInvoked|NotificationClosed) $id " | cut -d' ' -f1 | paste -sd' ')" \
        == "ActivationToken ActionInvoked NotificationClosed" ]]
}
