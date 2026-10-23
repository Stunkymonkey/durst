# Identical notifications merge into one with a counter; the replaced ids are
# closed with reason 4. Same for a shared stack tag, without counter.
a=$(notify -a dup "Same" "same body")
b=$(notify -a dup "Same" "same body")
c=$(notify -a dup "Same" "same body")
notify -a dup "Same" "different body" >/dev/null
v1=$(notify -h string:x-dunst-stack-tag:volume "Volume" "10%")
v2=$(notify -h string:x-dunst-stack-tag:volume "Volume" "20%")

verify() {
    signals | grep -qx "NotificationClosed $a 4" &&
    signals | grep -qx "NotificationClosed $b 4" &&
    signals | grep -qx "NotificationClosed $v1 4" &&
    ! signals | grep -q "NotificationClosed $c " &&
    ! signals | grep -q "NotificationClosed $v2 " &&
    check 'len(boxes) == 3'
}
