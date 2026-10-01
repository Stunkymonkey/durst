# Markup, progress bar and icons from image-path and image-data. Every box
# must fit its content (balanced padding), icons sit at the left edge.
python3 -c "from PIL import Image; Image.new('RGB', (64, 64), (255, 0, 0)).save('$DURST_TEST_DIR/red.png')"
notify -i "$DURST_TEST_DIR/red.png" "image-path" "a red square" >/dev/null
green=$(python3 -c "print(', '.join(['byte 0, byte 200, byte 0, byte 255'] * 4))")
send "image-data" "a green square" '[]' "{\"image-data\": <(2, 2, 8, true, 8, 4, [$green])>}" >/dev/null
notify "Markup" "<b>bold</b>, <i>italic</i>, <u>underline</u>, <a href='https://example.org'>a link</a> &amp; an entity, then enough text to wrap onto a second line" >/dev/null
notify -h int:value:60 "Progress" "60%" >/dev/null

icon() { pixel "$(( $(python3 -c "import json;print(json.load(open('$MEASURE'))['boxes'][$1]['x'])") + 14 + 24 ))" "$(( $(python3 -c "import json;print(json.load(open('$MEASURE'))['boxes'][$1]['y'])") + 14 + 24 ))"; }
verify() {
    echo "icons: $(icon 0) / $(icon 1)"
    check 'len(boxes) == 4' &&
    check 'all(abs(b["pad"][0] - b["pad"][2]) <= 6 for b in boxes)' &&
    check 'boxes[0]["pad"][3] == 14 and boxes[1]["pad"][3] == 14' &&
    [[ "$(icon 0)" == "255 0 0" ]] &&
    [[ "$(icon 1)" == "0 200 0" ]]
}
