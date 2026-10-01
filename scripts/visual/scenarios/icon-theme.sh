# Icons by theme name (app_icon) and through the desktop-entry hint, from a
# minimal hicolor theme created inside the sandbox.
share="$DURST_TEST_DIR/share"
mkdir -p "$share/icons/hicolor/48x48/apps" "$share/applications"
printf '[Icon Theme]\nName=Hicolor\nDirectories=48x48/apps\n\n[48x48/apps]\nSize=48\nType=Fixed\n' \
    >"$share/icons/hicolor/index.theme"
python3 -c "from PIL import Image; Image.new('RGB', (48, 48), (0, 0, 255)).save('$share/icons/hicolor/48x48/apps/durst-test.png')"
printf '[Desktop Entry]\nName=Durst Test\nIcon=durst-test\n' >"$share/applications/durst-test.desktop"

notify -i durst-test "app_icon" "by theme name" >/dev/null
send "desktop-entry" "icon from the .desktop file" '[]' '{"desktop-entry": <"durst-test">}' >/dev/null

icon() { pixel "$(( $(python3 -c "import json;print(json.load(open('$MEASURE'))['boxes'][$1]['x'])") + 14 + 24 ))" "$(( $(python3 -c "import json;print(json.load(open('$MEASURE'))['boxes'][$1]['y'])") + 14 + 24 ))"; }
verify() {
    echo "icons: $(icon 0) / $(icon 1)"
    check 'len(boxes) == 2' &&
    [[ "$(icon 0)" == "0 0 255" ]] &&
    [[ "$(icon 1)" == "0 0 255" ]]
}
