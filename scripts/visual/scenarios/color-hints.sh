# dunst's color hints restyle a notification (bgcolor, frcolor, hlcolor for
# the progress bar); a rule's style wins over them; invalid ones are ignored.
notify -a hinted -h string:bgcolor:#ff0000 -h string:frcolor:#00ff00 \
    -h string:hlcolor:#ffff00 -h int:value:100 "Hinted" >/dev/null
notify -a ruled -h string:bgcolor:#ff0000 "Ruled" >/dev/null
notify -a invalid -h string:bgcolor:red "Invalid" >/dev/null
snap
inside() { pixel "$(( $(geom "$1" right) - 20 ))" "$(( $(geom "$1" y) + 8 ))"; }
hinted=$(inside 0)
frame=$(pixel "$(( $(geom 0 x) + 50 ))" "$(geom 0 y)")
bar=$(pixel "$(( $(geom 0 x) + 50 ))" "$(( $(geom 0 bottom) - 17 ))")
ruled=$(inside 1)
invalid=$(inside 2)

verify() {
    echo "hinted: $hinted, frame: $frame, bar: $bar, ruled: $ruled, invalid: $invalid"
    [[ $hinted == "255 0 0" && $frame == "0 255 0" && $bar == "255 255 0" ]] &&
    [[ $ruled == "0 0 255" && $invalid != "255 0 0" ]]
}
