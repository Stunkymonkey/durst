# PipeWire and logind restart: durst notices both going away and reconnects
# by itself (backoff from 1 s), without a restart of durst.
before=$(durstctl volume get)
pipewire_stop
gone=$(await 3 status durstctl volume get)
pipewire_start
back=$(await "100  Test Speakers" durstctl volume get)
durstctl volume set 40 >/dev/null
set_after=$(await "40 False" pw_volume test-sink)

lock
locked_before=$(state locked)
logind_stop
logind_start                 # a new logind starts unlocked
lock                         # returns once durst has it, after reconnecting
relocked=$(state locked)

verify() {
    echo "before='$before' gone=$gone back='$back' set_after='$set_after' locked_before=$locked_before relocked=$relocked"
    [[ $before == "100  Test Speakers" && $gone == 3 && $back == "100  Test Speakers" ]] &&
    [[ $set_after == "40 False" && $locked_before == True && $relocked == True ]]
}
