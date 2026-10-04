# Text options: a rule's `font` changes the font (same text, other width), a
# body longer than `max_lines` (default 10) is cut to exactly the height of a
# 10-line body, and `show_app_name` adds a line above the summary.
notify -a plain "Wwwwwwwwwwwwww" >/dev/null
notify -a mono "Wwwwwwwwwwwwww" >/dev/null
notify -a ten "Lines" "$(seq -f 'line %g' 10)" >/dev/null
notify -a long "Lines" "$(seq -f 'line %g' 60)" >/dev/null
notify -a named "Wwwwwwwwwwwwww" >/dev/null

verify() {
    check 'len(boxes) == 5' &&
    # the same summary in another font has another width
    check 'abs(boxes[1]["pad"][1] - boxes[0]["pad"][1]) > 20' &&
    # cut to 10 lines: as tall as the 10-line body, content centered
    check 'boxes[3]["h"] == boxes[2]["h"]' &&
    check 'abs(boxes[3]["pad"][0] - boxes[3]["pad"][2]) <= 6' &&
    # the app name line makes it taller than the same summary alone
    check 'boxes[4]["h"] > boxes[0]["h"] + 10' &&
    check 'abs(boxes[4]["pad"][0] - boxes[4]["pad"][2]) <= 6'
}
