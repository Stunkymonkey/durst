# The OSD buttons show the icon theme's icons, symbolic ones in the button's
# text color: play/pause follows the player, the mute button the state. Each
# test icon fills another part of its square, so the shape tells which one
# is shown. "Next" only exists without -symbolic, which is the fallback.
share="$DURST_TEST_DIR/share"
dir="$share/icons/hicolor/scalable/actions"
mkdir -p "$dir"
printf '[Icon Theme]\nName=Hicolor\nDirectories=scalable/actions\n\n[scalable/actions]\nSize=16\nMinSize=8\nMaxSize=512\nType=Scalable\n' \
    >"$share/icons/hicolor/index.theme"
# icon NAME X Y W H: a square icon with that rectangle filled (of 16x16)
icon() {
    printf '<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 16 16"><rect x="%s" y="%s" width="%s" height="%s" fill="#000"/></svg>\n' \
        "$2" "$3" "$4" "$5" >"$dir/$1.svg"
}
icon media-skip-backward-symbolic 0 0 16 8         # top half
icon media-playback-start-symbolic 0 0 8 16        # left half
icon media-playback-pause-symbolic 8 0 8 16        # right half
icon media-skip-forward 0 8 16 8                   # bottom half
icon audio-volume-high-symbolic 0 0 8 16           # left half
icon audio-volume-muted-symbolic 8 0 8 16          # right half

# shape X Y: the quarters of the icon centered at X Y in the text color,
# e.g. "TL TR" for the top half
shape() {
    local on=() part name dx dy
    for part in TL:-4:-4 TR:4:-4 BL:-4:4 BR:4:4; do
        IFS=: read -r name dx dy <<<"$part"
        [[ "$(pixel $(( $1 + dx )) $(( $2 + dy )))" == "205 214 244" ]] && on+=("$name")
    done
    echo "${on[*]}"
}
# the buttons of the box's last row: "CENTER_X WIDTH" each, from the runs of
# the button color just above its bottom edge
buttons() {
    python3 - "$DURST_TEST_DIR/snap.png" "$DURST_TEST_DIR/snap.json" <<'PY'
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
            runs.append(f"{(start + x) // 2} {x - start}")
        start = None
print(*runs, sep="\n")
PY
}
# narrower than with text labels (the widest is "Previous")
narrow() { (( $1 < 60 )) && echo narrow || echo "$1 wide"; }
# media: the three buttons' shapes and widths, on a new screenshot
media() {
    snap 'len(boxes) == 1'
    local row out=() center width
    row=$(( $(geom 0 bottom) - 14 - 14 ))
    while read -r center width; do
        out+=("$(shape "$center" "$row"), $(narrow "$width")")
    done < <(buttons)
    printf '%s; ' "${out[@]}"
}

player_start test
player test "track 1|Song|Artist|"
player test status Playing
playing=$(await "TL TR, narrow; TR BR, narrow; BL BR, narrow; " media)
player test status Paused
paused=$(await "TL TR, narrow; TL BL, narrow; BL BR, narrow; " media)
player_stop test
snap 'len(boxes) == 0'    # the media OSD has timed out

# volume: the mute button is the last one, at the right end
mute() {
    snap 'len(boxes) == 1'
    local center width
    read -r center width < <(buttons | tail -1)
    echo "$(shape "$center" $(( $(geom 0 bottom) - 14 - 14 ))), $(narrow "$width")"
}
durstctl volume set 50 >/dev/null
unmuted=$(await "TL BL, narrow" mute)
durstctl volume mute on >/dev/null
muted=$(await "TR BR, narrow" mute)

verify() {
    echo "playing=[$playing] paused=[$paused] unmuted=[$unmuted] muted=[$muted]"
    [[ $playing == "TL TR, narrow; TR BR, narrow; BL BR, narrow; " ]] &&
    [[ $paused == "TL TR, narrow; TL BL, narrow; BL BR, narrow; " ]] &&
    [[ $unmuted == "TL BL, narrow" && $muted == "TR BR, narrow" ]]
}
