# Fullscreen policies: "pushback" hides even shown notifications while a
# fullscreen window has the focus, "delay" holds ones arriving meanwhile,
# the default shows them. Leaving fullscreen brings everything back.
notify "Pushed back" >/dev/null
fullscreen_window
fullscreen=$(state fullscreen)
notify "Delayed" >/dev/null
notify "Shown" >/dev/null
during=$(durstctl notif count)
# drawn above the fullscreen window: the top border of the only box
snap
border=$(pixel 1070 21)
held=$(durstctl notif count --held)
body=$(durstctl notif list --json | python3 -c "import json,sys; print([n['body'] for n in json.load(sys.stdin) if n['summary']=='Shown'][0])")
leave_fullscreen
after=$(state fullscreen)

verify() {
    echo "fullscreen=$fullscreen during=$during held=$held body='$body' after=$after border=$border"
    [[ $fullscreen == True && $during == 1 && $held == 2 && $after == False ]] &&
    [[ $border == "137 180 250" ]] &&
    [[ $body == "arrived during fullscreen" ]] &&
    check 'len(boxes) == 3 and gaps == [8, 8]'
}
