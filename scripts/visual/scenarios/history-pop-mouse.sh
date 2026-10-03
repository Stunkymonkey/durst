# The history_pop mouse action (here on scroll up) shows the last closed
# notification again, like `durstctl history pop`.
gone=$(notify -a test "Closed earlier")
durstctl notif close "$gone"
notify -a test "Still here" >/dev/null
snap
# scroll up (negative is up) over the notification that is shown
printf 'move %s %s\n' "$(( $(geom 0 x) + 50 ))" "$(( $(geom 0 y) + 15 ))" >&3
scroll -1
sleep 0.3
shown=$(durstctl notif list --json | python3 -c "import json,sys; print(sorted(n['summary'] for n in json.load(sys.stdin)))")

verify() {
    echo "shown: $shown"
    [[ $shown == "['Closed earlier', 'Still here']" ]] &&
    check 'len(boxes) == 2'
}
