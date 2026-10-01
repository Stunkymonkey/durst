# Notifications of different heights. Once the stack layout exists (M1), the
# boxes must not overlap and every entry in "gaps" must equal general.gap.
notify-send -a short "Short" "One line" &
notify-send -a long "Long body" "$(printf 'This body is long enough to wrap onto several lines. %.0s' 1 2 3 4)" &
notify-send -a progress -h int:value:60 "Progress" "60 percent" &
notify-send -a actions -A yes=Yes -A no=No "Actions" "With buttons" &
