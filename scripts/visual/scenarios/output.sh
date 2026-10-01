# A named output and another anchor: bottom-left, stacking upwards.
notify "First" >/dev/null
notify "Second" "with a body" >/dev/null

verify() {
    check 'len(boxes) == 2' &&
    check 'all(b["x"] == 20 for b in boxes)' &&
    check 'boxes[1]["y"] + boxes[1]["h"] == size[1] - 20' &&
    check 'gaps == [8]'
}
