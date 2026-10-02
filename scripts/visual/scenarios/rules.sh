# Restyle, rewrite, urgency and icon placement by rules.
share="$DURST_TEST_DIR/share"
mkdir -p "$share/icons/hicolor/48x48/apps"
printf '[Icon Theme]\nName=Hicolor\nDirectories=48x48/apps\n\n[48x48/apps]\nSize=48\nType=Fixed\n' \
    >"$share/icons/hicolor/index.theme"
python3 -c "
from PIL import Image
Image.new('RGB', (48, 48), (0, 0, 255)).save('$share/icons/hicolor/48x48/apps/durst-test.png')
Image.new('RGB', (48, 48), (0, 255, 0)).save('$DURST_TEST_DIR/green.png')"

notify -a red "Red border" >/dev/null
notify -a secret "Secret" "password 1234" >/dev/null
notify -a promote "Promoted" >/dev/null
notify -a iconright -i "$DURST_TEST_DIR/green.png" "Icon right" "with a body" >/dev/null
notify -a noicon -i "$DURST_TEST_DIR/green.png" "No icon" >/dev/null
notify -a fallback "Fallback icon" "from default_icon" >/dev/null
secret=$(durstctl notif list --json | python3 -c "
import json,sys; n=[n for n in json.load(sys.stdin) if n['app_name']=='secret'][0]; print(repr((n['summary'], n['body'])))")

# at I EXPR: evaluate EXPR with b = boxes[I]
at() { python3 -c "import json,sys;b=json.load(open('$MEASURE'))['boxes'][int(sys.argv[1])];print(eval(sys.argv[2]))" "$1" "$2"; }
verify() {
    echo "secret=$secret"
    # order: the promoted (critical) one first, then by arrival
    check 'len(boxes) == 6' &&
    [[ $secret == "('[secret] Secret', '')" ]] &&
    [[ "$(pixel "$(at 0 "b['x']+b['w']//2")" "$(at 0 "b['y']+1")")" == "243 139 168" ]] &&
    [[ "$(pixel "$(at 1 "b['x']+b['w']//2")" "$(at 1 "b['y']+1")")" == "255 0 0" ]] &&
    [[ "$(pixel "$(at 3 "b['x']+b['w']-14-24")" "$(at 3 "b['y']+14+24")")" == "0 255 0" ]] &&
    [[ "$(pixel "$(at 5 "b['x']+14+24")" "$(at 5 "b['y']+14+24")")" == "0 0 255" ]] &&
    check 'boxes[3]["pad"][3] < 20 and boxes[4]["pad"][3] < 20 and boxes[4]["h"] == boxes[0]["h"]'
}
