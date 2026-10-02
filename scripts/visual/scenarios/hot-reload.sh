# The config file is watched: changes apply without durstctl reload, an
# invalid config shows an error notification (the old config stays), and a
# valid one closes it again. Files are replaced like editors do it.
config="$XDG_CONFIG_HOME/durst/config.toml"
write() { printf "$1" >"$config.tmp" && mv "$config.tmp" "$config"; sleep 1; }
notify "Hot reload" >/dev/null
write '[general]\nwidth = 300\n'
snap
narrow=$(geom 0 w)
write '[general]\nwidth = 200\nbogus = 1\n'
broken=$(durstctl notif list --json | python3 -c "import json,sys; print(sorted(n['app_name'] for n in json.load(sys.stdin)))")
write '[general]\nwidth = 250\n'

verify() {
    echo "narrow=$narrow broken=$broken"
    [[ $narrow == 300 && $broken == "['durst', 'notify-send']" ]] &&
    check 'len(boxes) == 1 and boxes[0]["w"] == 250'
}
