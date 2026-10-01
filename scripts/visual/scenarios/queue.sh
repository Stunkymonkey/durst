# max_visible = 2: four notifications show two plus a "+2 more" indicator;
# closing one moves the next one in and the indicator says "+1 more".
first=$(notify "One")
notify "Two" >/dev/null
notify "Three" >/dev/null
notify "Four" >/dev/null
snap
echo "before: $(cat "$DURST_TEST_DIR/snap.json")"
[[ $(python3 -c "import json;print(len(json.load(open('$DURST_TEST_DIR/snap.json'))['boxes']))") == 3 ]] || echo "FAIL: expected 3 boxes before"
click 1000 40

verify() {
    signals | grep -qx "NotificationClosed $first 2" &&
    check 'len(boxes) == 3' &&
    check 'all(g == 8 for g in gaps)' &&
    check 'boxes[2]["h"] < boxes[1]["h"] and boxes[2]["w"] == 380'
}
