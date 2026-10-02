# The media OSD: shown on a track change with title, artist and cover (from a
# file URL); its buttons and durstctl media control the player.
python3 -c "
from PIL import Image
Image.new('RGB', (40, 40), (255, 0, 0)).save('$DURST_TEST_DIR/red.png')
Image.new('RGB', (40, 40), (0, 255, 0)).save('$DURST_TEST_DIR/green.png')"
none=$(status durstctl media status)
player_start test
player test status Playing
player test "track 1|First Song|First Artist|file://$DURST_TEST_DIR/red.png"
first=$(durstctl media status --json | python3 -c "import json,sys; m=json.load(sys.stdin); print(m['player'], m['status'], m['title'], m['artist'], sep='|')")
snap
first_cover=$(pixel $(( $(geom 0 x) + 14 + 36 )) $(( $(geom 0 y) + 14 + 36 )))
player test "track 2|Second Song|Second Artist|file://$DURST_TEST_DIR/green.png"
snap
second_cover=$(pixel $(( $(geom 0 x) + 14 + 36 )) $(( $(geom 0 y) + 14 + 36 )))
# the three buttons: runs of the button color in their row
read -r prev_x play_x next_x < <(python3 - "$DURST_TEST_DIR/snap.png" "$DURST_TEST_DIR/snap.json" <<'PY'
import json, sys
from PIL import Image
img = Image.open(sys.argv[1]).convert("RGB")
b = json.load(open(sys.argv[2]))["boxes"][0]
y = b["y"] + b["h"] - 1 - 14 - 3
runs, start = [], None
for x in range(b["x"], b["x"] + b["w"] + 1):
    inside = x < b["x"] + b["w"] and img.getpixel((x, y)) == (49, 50, 68)
    if inside and start is None:
        start = x
    elif not inside and start is not None:
        if x - start > 20:
            runs.append((start + x) // 2)
        start = None
print(*runs)
PY
)
button_y=$(( $(geom 0 bottom) - 14 - 10 ))
click "$next_x" "$button_y"
click "$play_x" "$button_y"
click "$prev_x" "$button_y"
durstctl media next
durstctl media toggle
calls=$(player_calls test)

verify() {
    echo "none=$none first=$first covers=$first_cover/$second_cover buttons=$prev_x,$play_x,$next_x calls=$calls"
    [[ $none == 3 && $first == "Fake test|playing|First Song|First Artist" ]] &&
    [[ $first_cover == "255 0 0" && $second_cover == "0 255 0" ]] &&
    [[ $calls == "Next PlayPause Previous Next PlayPause" ]] &&
    # top center, 420 wide; the cover starts at the inset, and the buttons
    # end there too (measure.py doesn't count their dark background as
    # content: the label's bottom is up to padding + descender higher)
    check 'len(boxes) == 1 and boxes[0]["w"] == 420 and boxes[0]["x"] == (size[0] - 420) // 2 and boxes[0]["y"] == 20' &&
    check 'boxes[0]["pad"][0] == 14 and 14 <= boxes[0]["pad"][2] <= 26'
}
