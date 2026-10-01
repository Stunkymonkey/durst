# durstctl notif: list, count, close (by id and newest), action, exit codes.
a=$(notify "First")
b=$(notify "Second")
c=$(send "Third" "with actions" '["default", "Open", "x", "X"]')
list_lines=$(durstctl notif list | wc -l)
json_ids=$(durstctl notif list --json | python3 -c "import json,sys; print(*[n['id'] for n in json.load(sys.stdin)])")
count=$(durstctl notif count)
durstctl notif close "$a"
durstctl notif action "$c" x
durstctl notif close            # the newest displayed: b
close_missing=$(status durstctl notif close 999)
action_none=$(status durstctl notif action)
bad_args=$(status durstctl notif bogus)

verify() {
    echo "lines=$list_lines ids=[$json_ids] count=$count exits: missing=$close_missing none=$action_none args=$bad_args"
    [[ $list_lines == 3 && $json_ids == "$a $b $c" && $count == 3 ]] &&
    [[ $close_missing == 3 && $action_none == 3 && $bad_args == 2 ]] &&
    signals | grep -qx "NotificationClosed $a 2" &&
    signals | grep -qx "ActionInvoked $c \"x\"" &&
    signals | grep -qx "NotificationClosed $c 2" &&
    signals | grep -qx "NotificationClosed $b 2" &&
    signals | grep -qE 'PropertiesChanged "org.durst_notification.Durst1" "DisplayedCount" +3$' &&
    signals | grep -qE 'PropertiesChanged "org.durst_notification.Durst1" "DisplayedCount" +0$' &&
    check 'len(boxes) == 0'
}
