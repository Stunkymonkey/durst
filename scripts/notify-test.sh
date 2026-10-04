#!/usr/bin/env bash
# Sends a battery of notifications to whatever daemon owns
# org.freedesktop.Notifications, for manual testing.
set -euo pipefail
n() { notify-send "$@"; sleep "${DELAY:-0.3}"; }

n -a test "Plain" "Summary and body"
n -a test "Only a summary"
n -a test -u low "Low urgency" "expires after the low timeout"
n -a test -u critical "Critical" "never expires"
n -a test -t 2000 "Custom timeout" "2 seconds"
n -a test -i dialog-information "Theme icon" "dialog-information"
n -a test "Long" "$(printf 'A long body that wraps over several lines. %.0s' 1 2 3 4 5)"
n -a test "Multiline" "$(printf 'one\ntwo\nthree')"
n -a test -h int:value:42 "Progress" "42%"
n -a test -A yes=Yes -A no=No "Actions" "two actions"
id=$(notify-send -p -a test "Replace" "first version")
sleep 1
notify-send -r "$id" -a test "Replace" "second version, longer: $(printf 'more text %.0s' 1 2 3 4 5 6)"
n -a test -h string:x-dunst-stack-tag:volume "Stack tag" "1"
n -a test -h string:x-dunst-stack-tag:volume "Stack tag" "2"
n -a test "Markup" "<b>bold</b> <i>italic</i> <u>underline</u> <a href=\"https://example.org\">link</a>"
