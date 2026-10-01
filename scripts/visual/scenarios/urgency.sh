# Sorting by urgency (critical first) and the per-urgency style: critical
# notifications get the built-in red border, the others the default blue.
notify -u low "Low" >/dev/null
notify -u normal "Normal" >/dev/null
notify -u critical "Critical" >/dev/null

border() { pixel "$(( $(python3 -c "import json;b=json.load(open('$MEASURE'))['boxes'][$1];print(b['x']+b['w']//2)") ))" "$(python3 -c "import json;print(json.load(open('$MEASURE'))['boxes'][$1]['y']+1)")"; }
verify() {
    check 'len(boxes) == 3' &&
    echo "borders: $(border 0) / $(border 1) / $(border 2)" &&
    [[ "$(border 0)" == "243 139 168" ]] &&
    [[ "$(border 1)" == "137 180 250" ]] &&
    [[ "$(border 2)" == "137 180 250" ]]
}
