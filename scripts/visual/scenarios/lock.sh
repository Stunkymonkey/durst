# Timeouts pause while the session is locked (logind LockedHint).
id=$(notify -t 1500 "Locked")
lock
locked=$(state locked)
sleep 2.5
snap
while_locked=$(python3 -c "import json;print(len(json.load(open('$DURST_TEST_DIR/snap.json'))['boxes']))")
unlock
unlocked=$(state locked)
sleep 2

verify() {
    echo "locked=$locked boxes_while_locked=$while_locked unlocked=$unlocked"
    [[ $locked == True && $while_locked == 1 && $unlocked == False ]] &&
    signals | grep -qx "NotificationClosed $id 1" &&
    check 'len(boxes) == 0'
}
