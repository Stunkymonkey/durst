# Many action buttons wrap into a grid of equally wide buttons, as many per
# row as fit with their labels uncut; the height prediction includes the
# extra rows, and a button in the last row works.
short=$(send "Two" "short labels" '["yes", "Yes", "no", "No"]')
many=$(send "Five" "long labels" '["a", "Archive this thread", "b", "Reply to everyone", "c", "Forward to a friend", "d", "Delete the message", "e", "Mark it as unread"]')
snap
# two per row: the fifth button is alone, at the left of the last row
click "$(( $(geom 1 x) + $(geom 1 w) / 4 ))" "$(( $(geom 1 bottom) - 26 ))"

verify() {
    signals
    signals | grep -qx "ActionInvoked $many \"e\"" &&
    ! signals | grep -q "ActionInvoked $short " &&
    check 'len(boxes) == 1'
}
