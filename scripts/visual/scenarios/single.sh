# One plain notification at the default position (top-right, offset 20, width 380).
notify -a test "Summary" "Body text" >/dev/null

verify() {
    check 'len(boxes) == 1' &&
    check 'boxes[0]["w"] == 380 and boxes[0]["y"] == 20' &&
    check 'boxes[0]["x"] + boxes[0]["w"] == size[0] - 20'
}
