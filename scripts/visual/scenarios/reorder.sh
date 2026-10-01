# Many notifications at once, critical ones overtaking the others: surfaces
# get moved (and one closed) before they exist. Every one must end up in its
# place (no overlap, gaps exactly 8) and none may be left behind.
pids=()
for i in 1 2 3; do
    notify -u low "Low $i" >/dev/null & pids+=($!)
    notify -u normal "Normal $i" >/dev/null & pids+=($!)
done
notify -u critical "Critical 1" >/dev/null & pids+=($!)
notify -u critical "Critical 2" >/dev/null & pids+=($!)
notify -u critical -t 1 "Expires at once" >/dev/null & pids+=($!)
# only our jobs: a plain `wait` would wait for the harness's sway and durst
wait "${pids[@]}"

verify() {
    check 'len(boxes) == 8' &&
    check 'all(g == 8 for g in gaps)' &&
    check 'boxes[0]["y"] == 20'
}
