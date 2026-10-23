# outputs: 2
# output = "focused": the stack opens on the output of the focused window and
# stays there while it is shown; once empty, the next one follows the focus,
# also to an output connected later.
shown() { snap "len([b for b in boxes if b['w'] == 380]) == $1"; }
on() { python3 -c "import json; print(sorted(b['x'] // 1280 + 1 for b in json.load(open('$DURST_TEST_DIR/snap.json'))['boxes'] if b['w'] == 380))"; }
window_on HEADLESS-2
focus1=$(state focused_output)
notify "Opens on 2" >/dev/null
shown 1; first=$(on)
window_on HEADLESS-1
notify "Joins the stack on 2" >/dev/null
shown 2; together=$(on)
durstctl notif close-all
notify "Follows the focus to 1" >/dev/null
shown 1; followed=$(on)
durstctl notif close-all
# a monitor is connected and gets the focus
swaymsg -q create_output
swaymsg -q output HEADLESS-3 resolution 1280x720 position 2560 0
window_on HEADLESS-3
focus3=$(state focused_output)
notify "On the new monitor" >/dev/null
shown 1; hotplugged=$(on)

verify() {
    echo "focus1=$focus1 first=$first together=$together followed=$followed focus3=$focus3 hotplugged=$hotplugged"
    [[ $focus1 == HEADLESS-2 && $first == "[2]" && $together == "[2, 2]" ]] &&
    [[ $followed == "[1]" && $focus3 == HEADLESS-3 && $hotplugged == "[3]" ]]
}
