# activation
# D-Bus activation with the service files from contrib/dbus: the first
# notification starts durst, which then shows it; so does durstctl.
before=$(status busctl --user status org.freedesktop.Notifications)
id=$(notify "Started on demand")
pid=$(busctl --user status org.freedesktop.Notifications | sed -n 's/^PID=//p')
control_pid=$(busctl --user status org.durst_notification.Durst | sed -n 's/^PID=//p')

verify() {
    echo "before=$before id=$id pid=$pid control_pid=$control_pid"
    [[ $before != 0 && $id == 1 && -n $pid && $pid == "$control_pid" ]] &&
    check 'len(boxes) == 1 and boxes[0]["y"] == 20'
}
