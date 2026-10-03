# The volume OSD shows the icon theme's icon for the level: audio-volume-*
# for the speaker, microphone-sensitivity-* for the microphone; each test
# icon has its own color.
share="$DURST_TEST_DIR/share"
mkdir -p "$share/icons/hicolor/48x48/status"
printf '[Icon Theme]\nName=Hicolor\nDirectories=48x48/status\n\n[48x48/status]\nSize=48\nType=Fixed\n' \
    >"$share/icons/hicolor/index.theme"
icon() { python3 -c "from PIL import Image; Image.new('RGB', (48, 48), $2).save('$share/icons/hicolor/48x48/status/$1.png')"; }
icon audio-volume-high '(255, 0, 0)'
icon audio-volume-low '(255, 255, 0)'
icon audio-volume-muted '(0, 255, 0)'
icon microphone-sensitivity-medium '(0, 0, 255)'

# the icon's center: inset 14, icon 48
center() { snap; pixel "$(( $(geom 0 x) + 14 + 24 ))" "$(( $(geom 0 y) + $(geom 0 h) / 2 ))"; }
durstctl volume set 100 >/dev/null
high=$(center)
durstctl volume set 20 >/dev/null
low=$(center)
durstctl volume mute on >/dev/null
muted=$(center)
durstctl volume set 50 --mic >/dev/null
mic=$(center)

verify() {
    echo "high: $high, low: $low, muted: $muted, mic: $mic"
    [[ $high == "255 0 0" && $low == "255 255 0" && $muted == "0 255 0" && $mic == "0 0 255" ]] &&
    # the height fits the content next to the icon
    check 'abs(boxes[0]["pad"][0] - boxes[0]["pad"][2]) <= 6'
}
