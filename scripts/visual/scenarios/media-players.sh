# Several players: the one that most recently started playing is the one
# durstctl media and the OSD refer to; when it pauses or quits, the other.
player_start a
player a "track 1|Song A|Artist A|"
player a status Playing
player_start b
player b "track 2|Song B|Artist B|"
first=$(durstctl media status --json | python3 -c "import json,sys; print(json.load(sys.stdin)['title'])")
player b status Playing
second=$(durstctl media status --json | python3 -c "import json,sys; print(json.load(sys.stdin)['title'])")
durstctl media next
player b status Paused
third=$(durstctl media status --json | python3 -c "import json,sys; print(json.load(sys.stdin)['title'])")
player_stop a
fourth=$(durstctl media status --json | python3 -c "import json,sys; print(json.load(sys.stdin)['title'])")

verify() {
    echo "first=$first second=$second third=$third fourth=$fourth calls a=[$(player_calls a)] b=[$(player_calls b)]"
    [[ $first == "Song A" && $second == "Song B" && $third == "Song A" && $fourth == "Song B" ]] &&
    [[ "$(player_calls a)" == "" && "$(player_calls b)" == "Next" ]] &&
    # the OSD shows b's song without a cover: a placeholder keeps the place
    check 'len(boxes) == 1 and boxes[0]["w"] == 420'
}
