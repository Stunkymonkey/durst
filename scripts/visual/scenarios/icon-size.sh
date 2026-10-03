# Raster icons keep their size within min_icon_size..=max_icon_size, and
# icon_position = "top" puts the icon centered above the text.
python3 -c "
from PIL import Image
Image.new('RGB', (16, 16), (255, 0, 0)).save('$DURST_TEST_DIR/small.png')
Image.new('RGB', (256, 128), (0, 255, 0)).save('$DURST_TEST_DIR/wide.png')
Image.new('RGB', (40, 40), (0, 0, 255)).save('$DURST_TEST_DIR/mid.png')
Image.new('RGB', (48, 48), (255, 255, 0)).save('$DURST_TEST_DIR/top.png')"
notify -i "$DURST_TEST_DIR/small.png" "Small" "scaled up to 32" >/dev/null
notify -i "$DURST_TEST_DIR/wide.png" "Wide" "scaled down to 64x32" >/dev/null
notify -i "$DURST_TEST_DIR/mid.png" "Mid" "kept at 40" >/dev/null
notify -a top -i "$DURST_TEST_DIR/top.png" "Top" "icon above the text" >/dev/null

# px I DX DY: the pixel at (DX, DY) from the content corner of box I
px() {
    local b
    b=$(python3 -c "import json;b=json.load(open('$MEASURE'))['boxes'][$1];print(b['x'], b['y'], b['w'])")
    set -- "$@" $b
    pixel "$(( $4 + 14 + $2 ))" "$(( $5 + 14 + $3 ))"
}
# center I DY: the pixel at the horizontal center of box I
center() {
    local b
    b=$(python3 -c "import json;b=json.load(open('$MEASURE'))['boxes'][$1];print(b['x']+b['w']//2, b['y'])")
    set -- "$@" $b
    pixel "$3" "$(( $4 + 14 + $2 ))"
}
verify() {
    echo "small: $(px 0 30 16)/$(px 0 34 16) wide: $(px 1 62 30)/$(px 1 30 34) mid: $(px 2 38 38)/$(px 2 42 20) top: $(center 3 24)/$(px 3 4 24)"
    check 'len(boxes) == 4' &&
    [[ "$(px 0 30 16)" == "255 0 0" && "$(px 0 34 16)" != "255 0 0" ]] &&
    [[ "$(px 1 62 30)" == "0 255 0" && "$(px 1 30 34)" != "0 255 0" ]] &&
    [[ "$(px 2 38 38)" == "0 0 255" && "$(px 2 42 20)" != "0 0 255" ]] &&
    [[ "$(center 3 24)" == "255 255 0" && "$(px 3 4 24)" != "255 255 0" ]] &&
    # the text sits below the icon, at the left edge
    check 'boxes[3]["h"] >= 28 + 48 + 8 + 2 * 15 and boxes[3]["pad"][3] == 14'
}
