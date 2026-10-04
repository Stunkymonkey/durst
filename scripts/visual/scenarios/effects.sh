# Scripts on receive/action/close with DURST_* variables; sounds from hints
# (sound theme lookup with name fallback), from rules, suppressed, muted and
# silent while held. Sound names are made up so that the system's sound
# theme can't match them.
mkdir -p "$DURST_TEST_DIR/share/sounds/freedesktop/stereo"
touch "$DURST_TEST_DIR/share/sounds/freedesktop/stereo/durst-test.oga"

scripted=$(send "Scripted" "a body" '["ok", "OK"]')
sleep 0.2
durstctl notif action "$scripted" ok
notify -h string:sound-name:durst-test-new-instant "Hinted" >/dev/null
notify -h string:sound-name:durst-test -h boolean:suppress-sound:true "Suppressed" >/dev/null
notify "Ruled sound" >/dev/null
notify -h string:sound-name:durst-test "Muted" >/dev/null
durstctl mode enable dnd >/dev/null
notify -h string:sound-name:durst-test "Held quietly" >/dev/null

verify() {
    echo "sounds: $(paste -sd' ' "$DURST_TEST_DIR/sounds")"
    echo "action: $(cat "$DURST_TEST_DIR/action") close: $(cat "$DURST_TEST_DIR/close")"
    grep -qx "DURST_ID=$scripted" "$DURST_TEST_DIR/receive.env" &&
    grep -qx "DURST_SUMMARY=Scripted" "$DURST_TEST_DIR/receive.env" &&
    grep -qx "DURST_BODY=a body" "$DURST_TEST_DIR/receive.env" &&
    grep -qx "DURST_EVENT=receive" "$DURST_TEST_DIR/receive.env" &&
    [[ "$(cat "$DURST_TEST_DIR/action")" == ok && "$(cat "$DURST_TEST_DIR/close")" == dismissed ]] &&
    [[ "$(cat "$DURST_TEST_DIR/sounds")" == "$(printf '%s\n' "$DURST_TEST_DIR/share/sounds/freedesktop/stereo/durst-test.oga" /dev/null)" ]]
}
