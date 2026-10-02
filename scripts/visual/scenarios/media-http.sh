# Covers over HTTP; a missing one leaves the placeholder.
mkdir -p "$DURST_TEST_DIR/www"
python3 -c "from PIL import Image; Image.new('RGB', (300, 300), (0, 0, 255)).save('$DURST_TEST_DIR/www/blue.png')"
base=$(serve "$DURST_TEST_DIR/www")
player_start test
player test status Playing
player test "track 1|Missing cover|Artist|$base/missing.png"
sleep 0.5
snap
missing=$(pixel $(( $(geom 0 x) + 14 + 36 )) $(( $(geom 0 y) + 14 + 36 )))
player test "track 2|Remote cover|Artist|$base/blue.png"
sleep 0.5

verify() {
    cover=$(pixel $(( $(python3 -c "import json;print(json.load(open('$MEASURE'))['boxes'][0]['x'])") + 14 + 36 )) $(( 20 + 14 + 36 )))
    echo "base=$base missing=$missing cover=$cover"
    [[ $missing == "69 71 90" && $cover == "0 0 255" ]] &&
    check 'len(boxes) == 1'
}
