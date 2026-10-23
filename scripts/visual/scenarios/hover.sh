# The timeout pauses while the pointer is over the notification.
# 5 s: enough for a slow machine to see the notification before hovering.
id=$(notify -t 5000 "Hover me")
# a surface appearing under a resting pointer gets no enter event, so move
# only once it is mapped
snap 'len(boxes) == 1'
pointer 1000 40
sleep 6                     # longer than the timeout, but hovered
snap 'len(boxes) == 1'
hovered=$(python3 -c "import json;print(len(json.load(open('$DURST_TEST_DIR/snap.json'))['boxes']))")
pointer 100 600

verify() {
    echo "boxes while hovered: $hovered"
    [[ $hovered == 1 ]] &&
    signals | grep -qx "NotificationClosed $id 1" &&
    check 'len(boxes) == 0'
}
