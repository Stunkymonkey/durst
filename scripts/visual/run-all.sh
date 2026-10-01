#!/usr/bin/env bash
# Runs every scenario in scripts/visual/scenarios and prints a summary.
# Screenshots and measurements land in target/visual/.
set -uo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
cargo build --quiet --workspace || exit 1
failed=()
for scenario in "$here"/scenarios/*.sh; do
    name="$(basename "$scenario" .sh)"
    if "$here/run.sh" "$scenario" >"${TMPDIR:-/tmp}/durst-visual-$name.log" 2>&1; then
        echo "ok   $name"
    else
        echo "FAIL $name"
        sed 's/^/     /' "${TMPDIR:-/tmp}/durst-visual-$name.log" | grep -v dbus-daemon
        failed+=("$name")
    fi
done
[[ ${#failed[@]} -eq 0 ]]
