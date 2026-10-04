# PipeWire and logind restart: durst notices both going away and reconnects
# by itself (backoff from 1 s), without a restart of durst.
until_ok() { for _ in $(seq 40); do "$@" >/dev/null 2>&1 && return 0; sleep 0.25; done; return 1; }
before=$(durstctl volume get)
pipewire_stop
sleep 0.5
gone=$(status durstctl volume get)
pipewire_start
until_ok durstctl volume get && back=$(durstctl volume get) || back="never"
durstctl volume set 40 >/dev/null
sleep 0.3
set_after=$(pw_volume test-sink)

lock
locked_before=$(state locked)
logind_stop
logind_start                 # a new logind starts unlocked
sleep 0.5
lock
until_ok sh -c '[ "$(durstctl info --json | python3 -c "import json,sys; print(json.load(sys.stdin)[\"locked\"])")" = True ]' \
    && relocked=True || relocked=False

verify() {
    echo "before='$before' gone=$gone back='$back' set_after='$set_after' locked_before=$locked_before relocked=$relocked"
    [[ $before == "100  Test Speakers" && $gone == 3 && $back == "100  Test Speakers" ]] &&
    [[ $set_after == "40 False" && $locked_before == True && $relocked == True ]]
}
