# A do-not-disturb mode: normal notifications are held back while it is
# active, critical ones still show; turning it off releases the held ones.
durstctl mode enable dnd >/dev/null
held_id=$(notify "Normal during dnd")
notify -u critical "Critical during dnd" >/dev/null
snap
during=$(python3 -c "import json;print(len(json.load(open('$DURST_TEST_DIR/snap.json'))['boxes']))")
held=$(durstctl notif count --held)
states=$(durstctl notif list --json | python3 -c "import json,sys; print(*sorted(n['state'] for n in json.load(sys.stdin)))")
active=$(durstctl mode list)
all=$(durstctl mode list --all | paste -sd,)
durstctl mode toggle dnd >/dev/null
after=$(durstctl mode list | wc -l)

verify() {
    echo "during=$during held=$held states=[$states] active=$active all=$all after=$after"
    [[ $during == 1 && $held == 1 && $states == "displayed held" ]] &&
    [[ $active == dnd && $all == "* dnd" && $after == 0 ]] &&
    ! signals | grep -q "NotificationClosed $held_id " &&
    check 'len(boxes) == 2 and gaps == [8]'
}
