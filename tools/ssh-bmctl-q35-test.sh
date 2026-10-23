#!/bin/sh
# q35 check of `bmctl ssh` (attended): bmctl answers WHILE the SSH shell is open, keeps serving after it closes, and
# its idle bound ends an open session (the control channel, not a wall clock, bounds it). Needs the throwaway sshd:
# tools/docker-sshd/run.sh sshd_config.hold   (shell held 40 s, then exits). Exit 1 on any failure.
export NC110_KEY=test-keys/client.key
K=target/attended/x86_64-unknown-none/release/nc110-kernel; P=tools/.venv/bin/python; fail=0; L=/tmp/sshbm-q35.log
B="$P tools/bmctl.py --host 127.0.0.1 --port 15515"
boot() {  # $1 extra cmdline
  rm -f $L
  qemu-system-x86_64 -machine q35 -m 128 -display none -no-reboot -global ICH9-LPC.noreboot=false -serial file:$L \
    -kernel $K -initrd test-keys/client.key -append "bmctl ssh nic=mmio $1" -netdev user,id=n0,hostfwd=udp:127.0.0.1:15515-:5515 \
    -device rtl8139,netdev=n0 &
  Q=$!
}
waitfor() {  # $1 pattern $2 seconds
  for i in $(seq $(($2 * 2))); do grep -q "$1" $L 2>/dev/null && return 0; sleep 0.5; done; echo "FAIL timeout waiting for: $1"; fail=1; return 1
}
ok() { echo "ok   $1"; }
bad() { echo "FAIL $1"; fail=1; }
trap 'kill $Q 2>/dev/null' EXIT

echo "== run 1: ping during and after the session"
boot ""
waitfor 'term: VGA now belongs' 60
r=$($B ping 2>&1); case "$r" in pong*) ok "bmctl ping while the shell is open: $r";; *) bad "ping during session: $r";; esac
grep -q 'nc110-ssh-hold' $L && ok "shell output reached the terminal" || bad "no shell output in the log"
grep -q 'home: menu up' $L && bad "session already over at the first ping (test did not overlap)"
waitfor 'home: menu up' 60
r=$($B ping 2>&1); case "$r" in pong*) ok "bmctl ping after the session closed: $r";; *) bad "ping after session: $r";; esac
r=$($B logall 2>&1 | grep -c 'home: menu up'); [ "$r" -ge 1 ] && ok "bmctl log carries the session end" || bad "log lacks session end"
$B reboot >/dev/null 2>&1; sleep 2
kill -0 $Q 2>/dev/null && bad "qemu still running after bmctl reboot" || ok "bmctl reboot -> reset"

echo "== run 2: idle=15, no bmctl traffic: the open session must end via the bmctl idle bound"
boot "idle=15"
waitfor 'term: VGA now belongs' 60
waitfor 'bmctl: idle 15 s -> reset' 30 && ok "idle bound fired during the open session"
grep -q 'home: menu up' $L && bad "home loop entered after idle (should reset)" || ok "no serve after idle"
waitfor 'hard reset via 0xCF9' 20 && ok "reset path taken"
for i in $(seq 70); do kill -0 $Q 2>/dev/null || break; sleep 1; done
kill -0 $Q 2>/dev/null && bad "qemu still running 70 s after idle" || ok "qemu reset after idle"
echo "RESULT: $([ $fail = 0 ] && echo PASS || echo FAIL)"; exit $fail
