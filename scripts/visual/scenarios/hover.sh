# The timeout pauses while the pointer is over the notification.
id=$(notify -t 1500 "Hover me")
# a surface appearing under a resting pointer gets no enter event, so move
# only once it is mapped
sleep 0.5
pointer 1000 40
sleep 2.5
snap
hovered=$(python3 -c "import json;print(len(json.load(open('$DURST_TEST_DIR/snap.json'))['boxes']))")
pointer 100 600
sleep 1.5

verify() {
    echo "boxes while hovered: $hovered"
    [[ $hovered == 1 ]] &&
    signals | grep -qx "NotificationClosed $id 1" &&
    check 'len(boxes) == 0'
}
