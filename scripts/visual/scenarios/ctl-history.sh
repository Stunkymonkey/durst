# History: expired and dismissed notifications are kept, transient ones and
# those closed by the sender are not; pop shows one again (sticky).
expired=$(notify -t 300 "Expired")
dismissed=$(notify "Dismissed")
transient=$(notify -h boolean:transient:true "Transient")
app=$(notify "Closed by the app")
sleep 0.6
durstctl notif close "$dismissed"
durstctl notif close "$transient"
gdbus call --session --dest org.freedesktop.Notifications \
    --object-path /org/freedesktop/Notifications \
    --method org.freedesktop.Notifications.CloseNotification "$app" >/dev/null
history_ids=$(durstctl history list --json | python3 -c "import json,sys; print(*[n['id'] for n in json.load(sys.stdin)])")
popped=$(durstctl history pop)
after_pop=$(durstctl history count)
durstctl history clear
cleared=$(durstctl history count)
pop_empty=$(status durstctl history pop)
sleep 1   # the popped notification is sticky and must not expire

verify() {
    echo "history=[$history_ids] popped=$popped after_pop=$after_pop cleared=$cleared pop_empty=$pop_empty"
    [[ $history_ids == "$dismissed $expired" && $popped == "$dismissed" ]] &&
    [[ $after_pop == 1 && $cleared == 0 && $pop_empty == 3 ]] &&
    check 'len(boxes) == 1'
}
