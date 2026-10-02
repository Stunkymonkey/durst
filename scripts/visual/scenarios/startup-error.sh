# A broken config at startup: durst runs with the defaults and shows the
# error as a critical notification (red border, never expires).
notify "Still works" >/dev/null
summary=$(durstctl notif list --json | python3 -c "import json,sys; print([n['summary'] for n in json.load(sys.stdin) if n['app_name']=='durst'][0])")

verify() {
    echo "summary=$summary"
    [[ $summary == "Invalid config, durst uses the defaults" ]] &&
    check 'len(boxes) == 2 and all(b["w"] == 380 for b in boxes)' &&
    [[ "$(pixel 1070 21)" == "243 139 168" ]]
}
