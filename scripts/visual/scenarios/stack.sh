# Notifications of different heights: they must stack without overlap, with
# exactly `general.gap` (8) between them, and the predicted height must fit the
# rendered text: top and bottom padding around the content are (nearly) equal.
notify -a short "Short" "One line" >/dev/null
notify -a long "Long body" "$(printf 'This body is long enough to wrap onto several lines. %.0s' 1 2 3 4)" >/dev/null
notify -a summary-only "Only a summary, but a long one that has to wrap as well" >/dev/null
notify -a multiline "Multiline" "$(printf 'first line\nsecond line\nthird line')" >/dev/null

verify() {
    check 'len(boxes) == 4' &&
    check 'all(g == 8 for g in gaps)' &&
    check 'boxes[0]["y"] == 20 and len({b["x"] for b in boxes}) == 1' &&
    check 'len({b["h"] for b in boxes}) > 1' &&
    check 'all(abs(b["pad"][0] - b["pad"][2]) <= 6 for b in boxes)'
}
