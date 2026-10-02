# Another program changes the volume: the OSD shows the new value.
pw_set test-sink 80
osd=$(python3 -c "print(1)")
value=$(durstctl volume get)

verify() {
    echo "value='$value'"
    [[ $value == "80  Test Speakers" ]] &&
    check 'len(boxes) == 1 and boxes[0]["w"] == 360'
}
