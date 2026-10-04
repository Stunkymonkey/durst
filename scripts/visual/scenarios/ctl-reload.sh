# durstctl reload: a new width applies to the displayed notification; an
# invalid config is rejected (exit 4), the old one stays active and durst
# shows the error as a critical notification.
config="$XDG_CONFIG_HOME/durst/config.toml"
mkdir -p "$(dirname "$config")"
notify "Reload me" "the box gets narrower" >/dev/null
snap
before=$(geom 0 w)
printf '[general]\nwidth = 300\n' >"$config"
reload_ok=$(status durstctl reload)
printf '[general]\nwidht = 200\n' >"$config"
reload_bad=$(status durstctl reload)
durstctl reload 2>"$DURST_TEST_DIR/reload.err" || true
info=$(durstctl info --json | python3 -c "import json,sys; i=json.load(sys.stdin); print(i['displayed'], i['config_path'])")

verify() {
    echo "before=$before reload_ok=$reload_ok reload_bad=$reload_bad info=$info"
    echo "error: $(cat "$DURST_TEST_DIR/reload.err")"
    [[ $before == 380 && $reload_ok == 0 && $reload_bad == 4 ]] &&
    grep -q widht "$DURST_TEST_DIR/reload.err" &&
    [[ $info == "2 $config" ]] &&
    check 'len(boxes) == 2 and all(b["w"] == 300 and b["x"] == size[0] - 20 - 300 for b in boxes)' &&
    [[ "$(pixel 1110 21)" == "243 139 168" ]]
}
