# The history_pop mouse action (here on scroll up) shows the last closed
# notification again, like `durstctl history pop`.
gone=$(notify -a test "Closed earlier")
durstctl notif close "$gone"
notify -a test "Still here" >/dev/null
snap 'len(boxes) == 1'
# scroll up (negative is up) over the notification that is shown
pointer "$(( $(geom 0 x) + 50 ))" "$(( $(geom 0 y) + 15 ))"
scroll -1
summaries() { durstctl notif list --json | python3 -c "import json,sys; print(sorted(n['summary'] for n in json.load(sys.stdin)))"; }
shown=$(await "['Closed earlier', 'Still here']" summaries)

verify() {
    echo "shown: $shown"
    [[ $shown == "['Closed earlier', 'Still here']" ]] &&
    check 'len(boxes) == 2'
}
