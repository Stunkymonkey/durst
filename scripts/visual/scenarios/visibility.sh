# skip_display, history_ignore, timeout, auto_invoke and default_action.
skipped=$(notify "Skipped")
nohist=$(notify "No history")
durstctl notif close "$nohist"
quick=$(notify -t 0 "Quick")
auto=$(send "Auto" "" '["go", "Go"]')
middle=$(send "Middle" "" '["first", "First", "second", "Second"]')
sleep 0.6
snap
click "$(( $(geom 0 x) + 100 ))" "$(( $(geom 0 y) + 20 ))" middle
history=$(durstctl history list --json | python3 -c "import json,sys; print(*sorted(n['id'] for n in json.load(sys.stdin)))")

verify() {
    echo "history=[$history]"
    signals | grep -qx "NotificationClosed $skipped 4" &&
    signals | grep -qx "NotificationClosed $quick 1" &&
    signals | grep -qx "ActionInvoked $auto \"go\"" &&
    signals | grep -qx "NotificationClosed $auto 2" &&
    signals | grep -qx "ActionInvoked $middle \"second\"" &&
    [[ $history == "$(printf '%s\n' "$skipped" "$quick" "$auto" "$middle" | sort -n | paste -sd' ')" ]] &&
    check 'len(boxes) == 0'
}
