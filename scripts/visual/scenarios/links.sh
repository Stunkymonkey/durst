# Clicking a link opens it without closing the notification; the open_url
# mouse action (right button here) opens the first link of the body.
send "" "<a href='https://one.example'>link one</a> and <a href='https://two.example'>two</a>" >/dev/null
snap 'len(boxes) == 1'
# the body starts at the content inset (border 2 + padding 12)
click "$(( $(geom 0 x) + 30 ))" "$(( $(geom 0 y) + 14 + 9 ))"
wait_for test -s "$DURST_TEST_DIR/opened"   # one click at a time
click "$(( $(geom 0 x) + 300 ))" "$(( $(geom 0 y) + 20 ))" right

verify() {
    echo "opened: $(cat "$DURST_TEST_DIR/opened" 2>/dev/null | paste -sd' ')"
    [[ "$(cat "$DURST_TEST_DIR/opened")" == "$(printf 'https://one.example\nhttps://one.example')" ]] &&
    check 'len(boxes) == 1'
}
