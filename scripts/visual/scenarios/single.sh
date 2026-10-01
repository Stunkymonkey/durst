# One plain notification. Sent in the background because the current daemon
# blocks the Notify reply until the window closes.
notify-send -a test "Summary" "Body text" &
