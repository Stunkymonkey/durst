# The OSD with the mouse: clicking the slider sets the volume there,
# dragging it too (throttled), scrolling steps it, the button mutes, and it
# stays while hovered.
durstctl osd show volume
# the slider's rail: the longest stretch of rail colors inside the box (not
# the border, which has the same blue); at 100 % it is all blue
rail() {
    snap
    read -r left right row < <(python3 - "$DURST_TEST_DIR/snap.png" "$DURST_TEST_DIR/snap.json" <<'PY'
import json, sys
from PIL import Image
img = Image.open(sys.argv[1]).convert("RGB")
b = json.load(open(sys.argv[2]))["boxes"][0]
inner = range(b["x"] + 6, b["x"] + b["w"] - 6)
colors = {(137, 180, 250), (69, 71, 90)}

def longest_run(y):
    """The longest stretch of rail colors in row y; single pixels of text
    can have the same color by chance."""
    best, start = (0, 0), None
    for x in list(inner) + [None]:
        if x is not None and img.getpixel((x, y)) in colors:
            start = x if start is None else start
        elif start is not None:
            best = max(best, (x - start, start))
            start = None
    return best

(length, start), y = max(
    (longest_run(y), y) for y in range(b["y"] + 6, b["y"] + b["h"] - 6)
)
print(start, start + length - 1, y)
PY
    )
}
rail
first_rail="$left..$right"
click $(( left + (right - left) / 4 )) "$row"
sleep 0.3
clicked=$(pw_volume test-sink)
pointer $(( left + 5 )) $(( row - 20 ))
scroll 10
sleep 0.3
scrolled=$(pw_volume test-sink)
# the mute button: right end of the control row
click $(( $(geom 0 right) - 14 - 15 )) "$row"
sleep 0.3
muted=$(pw_volume test-sink)
# the slider keeps its size when muted (the button has a fixed width)
rail
muted_rail="$left..$right"
# drag from 50 % to 75 %, in steps like a hand would
press $(( left + (right - left) / 2 )) "$row"
for f in 55 60 65 70 75; do pointer $(( left + (right - left) * f / 100 )) "$row"; done
release $(( left + (right - left) * 3 / 4 )) "$row"
sleep 0.3
dragged=$(pw_volume test-sink)
pointer $(( left + 5 )) $(( row - 20 ))
sleep 3                      # longer than the 2 s timeout, but hovered
snap
hovered=$(python3 -c "import json;print(len(json.load(open('$DURST_TEST_DIR/snap.json'))['boxes']))")
pointer 10 10
sleep 2.5

verify() {
    echo "rail=$first_rail muted_rail=$muted_rail @$row clicked=$clicked scrolled=$scrolled muted=$muted dragged=$dragged hovered=$hovered"
    read -r c _ <<<"$clicked"
    read -r d _ <<<"$dragged"
    (( c >= 23 && c <= 27 && d >= 73 && d <= 77 )) &&
    # same slider width muted or not (±2 px of anti-aliased rail ends)
    python3 -c "import sys; a, b = (tuple(map(int, r.split('..'))) for r in sys.argv[1:]); sys.exit(not all(abs(x - y) <= 2 for x, y in zip(a, b)))" "$first_rail" "$muted_rail" &&
    [[ $scrolled == "$(( c - 5 )) False" && $muted == "$(( c - 5 )) True" && $hovered == 1 ]] &&
    check 'len(boxes) == 0'
}
