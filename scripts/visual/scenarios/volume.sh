# durstctl volume: the private PipeWire's sink and source really change
# (read back with pw-dump), the OSD shows at the bottom and fits its content.
before=$(durstctl volume get)
durstctl volume set 50 >/dev/null
sleep 0.3
set50=$(pw_volume test-sink)
durstctl volume up >/dev/null
sleep 0.3
up=$(pw_volume test-sink)
durstctl volume set -15% >/dev/null
sleep 0.3
relative=$(pw_volume test-sink)
durstctl volume mute on >/dev/null
sleep 0.3
muted=$(pw_volume test-sink)
durstctl volume set 30 --mic >/dev/null
sleep 0.3
mic=$(pw_volume test-source)
invalid=$(status durstctl volume set loud)
get=$(durstctl volume get --mic --json | python3 -c "import json,sys; v=json.load(sys.stdin); print(v['percent'], v['muted'], v['description'])")

verify() {
    echo "before='$before' set50=$set50 up=$up relative=$relative muted=$muted mic=$mic invalid=$invalid get='$get'"
    [[ $before == "100  Test Speakers" ]] &&
    [[ $set50 == "50 False" && $up == "55 False" && $relative == "40 False" ]] &&
    [[ $muted == "40 True" && $mic == "30 False" && $invalid == 2 ]] &&
    [[ $get == "30 False Test Microphone" ]] &&
    # the OSD: centered at the bottom, 80 px up, 360 wide, content fits
    check 'len(boxes) == 1 and boxes[0]["w"] == 360 and boxes[0]["x"] == (size[0] - 360) // 2' &&
    check 'boxes[0]["y"] + boxes[0]["h"] == size[1] - 80' &&
    check 'abs(boxes[0]["pad"][0] - boxes[0]["pad"][2]) <= 6'
}
