#!/usr/bin/env bash
# Runs every scenario in scripts/visual/scenarios and prints a summary.
# Screenshots and measurements land in target/visual/.
# REPEAT=N runs each scenario N times, to catch flaky (racy) behavior.
set -uo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
cargo build --quiet --workspace || exit 1
failed=()
for scenario in "$here"/scenarios/*.sh; do
    name="$(basename "$scenario" .sh)"
    for run in $(seq "${REPEAT:-1}"); do
        log="${TMPDIR:-/tmp}/durst-visual-$name.log"
        if ! timeout 120 "$here/run.sh" "$scenario" >"$log" 2>&1; then
            echo "FAIL $name (run $run)"
            sed 's/^/     /' "$log" | grep -v dbus-daemon
            failed+=("$name")
            continue 2
        fi
    done
    echo "ok   $name"
done
[[ ${#failed[@]} -eq 0 ]]
