# outputs: 2
# output = "all": every notification on every output; closing one copy
# closes the notification everywhere.
notify "Everywhere" >/dev/null
second=$(notify "Second")
outputs=$(state outputs)
snap
before=$(python3 -c "import json;print(len(json.load(open('$DURST_TEST_DIR/snap.json'))['boxes']))")
# the second notification's copy on HEADLESS-2
click $(( 1280 + 1000 )) $(( 20 + 47 + 8 + 20 ))

verify() {
    echo "outputs=$outputs before=$before"
    [[ $outputs == "['HEADLESS-1', 'HEADLESS-2']" && $before == 4 ]] &&
    signals | grep -qx "NotificationClosed $second 2" &&
    check 'sorted(b["x"] for b in boxes) == [880, 2160]' &&
    check 'all(b["y"] == 20 for b in boxes)'
}
