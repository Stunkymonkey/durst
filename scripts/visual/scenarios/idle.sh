# Timeouts pause while the user is idle (ext-idle-notify) and resume on input.
id=$(notify -t 2500 "Idle")
sleep 2                     # idle since t=1s: paused with ~1.5s left
idle_state=$(state idle)
sleep 2                     # would have expired at t=2.5s without the pause
snap
while_idle=$(python3 -c "import json;print(len(json.load(open('$DURST_TEST_DIR/snap.json'))['boxes']))")
pointer 100 600             # input: active again, the timer resumes
active_state=$(state idle)
# stay active (the threshold is 1s) until the remaining ~1.5s are over
for i in 1 2 3 4 5; do pointer $((100 + i)) 600; sleep 0.3; done

verify() {
    echo "idle=$idle_state boxes_while_idle=$while_idle active_idle=$active_state"
    [[ $idle_state == True && $while_idle == 1 && $active_state == False ]] &&
    signals | grep -qx "NotificationClosed $id 1" &&
    check 'len(boxes) == 0'
}
