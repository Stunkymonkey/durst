# The config file is watched: changes apply without durstctl reload, an
# invalid config shows an error notification (the old config stays), and a
# valid one closes it again. Files are replaced like editors do it. Each
# write is one reload, and reading the file doesn't count as a change (that
# was an endless reload loop).
config="$XDG_CONFIG_HOME/durst/config.toml"
reloads() { grep -c 'config file changed' "$log/durst.log" || true; }
reloaded() { (( $(reloads) >= $1 )); }
# each write returns once durst has reloaded
write() {
    local n
    n=$(( $(reloads) + 1 ))
    printf "$1" >"$config.tmp" && mv "$config.tmp" "$config"
    wait_for reloaded "$n"
}
apps() { durstctl notif list --json | python3 -c "import json,sys; print(sorted(n['app_name'] for n in json.load(sys.stdin)))"; }
notify "Hot reload" >/dev/null
write '[general]\nwidth = 300\n'
snap 'len(boxes) == 1 and boxes[0]["w"] == 300'
narrow=$(geom 0 w)
write '[general]\nwidth = 200\nbogus = 1\n'
broken=$(await "['durst', 'notify-send']" apps)
write '[general]\nwidth = 250\n'
durstctl notif list >/dev/null
sleep 2   # no reload of its own after reading the file
reloads=$(grep -c 'config file changed' "$log/durst.log")

verify() {
    echo "narrow=$narrow broken=$broken reloads=$reloads"
    [[ $narrow == 300 && $broken == "['durst', 'notify-send']" && $reloads == 3 ]] &&
    check 'len(boxes) == 1 and boxes[0]["w"] == 250'
}
