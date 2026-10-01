# Action buttons send ActionInvoked and close the notification; the middle
# button invokes the default action; resident notifications stay open.
a=$(send "Buttons" "pick one" '["default", "Open", "yes", "Yes", "no", "No"]')
b=$(send "Default only" "middle click me" '["default", "Open"]')
c=$(send "Resident" "stays after the action" '["ok", "OK"]' '{"resident": <true>}')

# the "No" button: right half of the button row at the bottom of box 0
snap
click "$(( $(geom 0 x) + $(geom 0 w) * 3 / 4 ))" "$(( $(geom 0 bottom) - 26 ))"
snap
click "$(( $(geom 0 x) + 100 ))" "$(( $(geom 0 y) + 20 ))" middle
snap
click "$(( $(geom 0 x) + $(geom 0 w) / 2 ))" "$(( $(geom 0 bottom) - 26 ))"

verify() {
    signals
    signals | grep -qx "ActionInvoked $a \"no\"" &&
    signals | grep -qx "NotificationClosed $a 2" &&
    ! signals | grep -q "ActionInvoked $a \"default\"" &&
    signals | grep -qx "ActionInvoked $b \"default\"" &&
    signals | grep -qx "NotificationClosed $b 2" &&
    signals | grep -qx "ActionInvoked $c \"ok\"" &&
    ! signals | grep -q "NotificationClosed $c " &&
    check 'len(boxes) == 1'
}
