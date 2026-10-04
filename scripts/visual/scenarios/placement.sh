# outputs: 2
# Rules with anchor/output give notifications stacks of their own; a
# notification replaced into another stack moves there.
window_on HEADLESS-1
notify "Main 1" >/dev/null
notify -a low "Low 1" >/dev/null
moved=$(notify "Main 2")
notify -a two "Two 1" >/dev/null
notify -a low "Low 2" >/dev/null
snap
before=$(python3 -c "import json;print(sorted((b['x'], b['y']) for b in json.load(open('$DURST_TEST_DIR/snap.json'))['boxes'] if b['w'] == 380))")
# replaced with the app name of the rule for the second output
notify -a two -r "$moved" "Main 2" >/dev/null

verify() {
    echo "before=$before"
    # all one line high: h; the bottom-left stack grows upwards (the
    # window's cursor is a box of its own)
    check '(lambda n: all(b["h"] == n[0]["h"] for b in n) and
        sorted((b["x"], b["y"]) for b in n) == (lambda h:
        [(20, 692 - 2 * h), (20, 700 - h), (880, 20), (2160, 20), (2160, 28 + h)]
        )(n[0]["h"]))([b for b in boxes if b["w"] == 380])' &&
    python3 -c "
import json
h = [b for b in json.load(open('$MEASURE'))['boxes'] if b['w'] == 380][0]['h']
assert $before == [(20, 692 - 2 * h), (20, 700 - h), (880, 20), (880, 28 + h), (2160, 20)], 'before'"
}
