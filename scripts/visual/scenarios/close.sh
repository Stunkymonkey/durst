# Closing: by click (reason 2), by CloseNotification (reason 3) and by
# expiry (reason 1). The remaining notification moves up into the free space.
clicked=$(notify "Click me")
closed=$(notify "Closed via D-Bus")
expired=$(notify -t 300 "Expires")
kept=$(notify "Stays")
# the first one is on top whether the expiring one is still there or not
snap 'len(boxes) >= 3'
click 1000 40
gdbus call --session --dest org.freedesktop.Notifications \
    --object-path /org/freedesktop/Notifications \
    --method org.freedesktop.Notifications.CloseNotification "$closed" >/dev/null

verify() {
    signals
    signals | grep -qx "NotificationClosed $clicked 2" &&
    signals | grep -qx "NotificationClosed $closed 3" &&
    signals | grep -qx "NotificationClosed $expired 1" &&
    ! signals | grep -q "NotificationClosed $kept " &&
    check 'len(boxes) == 1 and boxes[0]["y"] == 20'
}
