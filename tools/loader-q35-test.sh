#!/bin/sh
# q35 check of the in-RAM chain loader: 5 refusals (the kernel must stay up), then gen1 -> gen2 -> gen3.
export NC110_KEY=test-keys/client.key  # q35 kernels get the test key module; the default is the agent-OS key
K=target/attended/x86_64-unknown-none/release/nc110-kernel; P=tools/.venv/bin/python; fail=0; T=$(mktemp -d)
$P - "$K" "$T" <<'PY'
import struct, sys
img = bytearray(open(sys.argv[1], "rb").read()); d = sys.argv[2]
o = next(i for i in range(0, 8192, 4) if struct.unpack_from("<I", img, i)[0] == 0x1BADB002)
a = bytearray(img); fl = struct.unpack_from("<I", a, o + 4)[0] & ~(1 << 16)
struct.pack_into("<II", a, o + 4, fl, (-(0x1BADB002 + fl)) & 0xffffffff); open(f"{d}/nokludge.elf", "wb").write(a)
b = bytearray(img); struct.pack_into("<I", b, o + 24, 0x0400_1000); open(f"{d}/overlap.elf", "wb").write(b)
PY
qemu-system-x86_64 -machine q35 -m 128 -display none -no-reboot -global ICH9-LPC.noreboot=false -serial file:/tmp/loader-q35.log \
  -kernel $K -initrd test-keys/client.key -append "bmctl gen=1" -netdev user,id=n0,hostfwd=udp:127.0.0.1:15515-:5515 -device rtl8139,netdev=n0 &
Q=$!; trap 'kill $Q 2>/dev/null; rm -rf $T' EXIT
for i in $(seq 40); do grep -q 'bmctl: listening' /tmp/loader-q35.log 2>/dev/null && break; sleep 0.5; done
B="$P tools/bmctl.py --host 127.0.0.1 --port 15515"
neg() { r=$(env $2 $B load $3 bmctl x 2>&1 | grep -E 'err|jumping' | tail -1); alive=$($B ping 2>&1)
  case "$r" in *"$4"*) case "$alive" in pong*) echo "ok   $1: '$r', still up";; *) echo "FAIL $1: kernel gone"; fail=1;; esac;;
  *) echo "FAIL $1: got '$r'"; fail=1;; esac; }
neg "n1 wrong sha256"     "BMCTL_TEST_SHA=$(printf '0%.0s' $(seq 64))" "$K" "sha256 mismatch"
neg "n2 truncated image"  "BMCTL_TEST_SEND=4000" "$K" "incomplete image"
neg "n3 no kludge flag"   "X=1" "$T/nokludge.elf" "lacks the address kludge"
neg "n4 bss into staging" "X=1" "$T/overlap.elf" "target range"
neg "n5 out-of-order"     "BMCTL_TEST_SWAP=1" "$K" "out of order"
for g in 2 3; do
  r=$($B load $K bmctl gen=$g 2>&1 | tail -1); l=$($B logall | grep -c "cmdline: bmctl gen=$g")
  k=$($B logall | grep -c 'keymod: client key from module 0 at 0x9000')
  case "$r" in "new kernel up"*) [ "$l" = 1 ] && [ "$k" = 1 ] && echo "ok   hop to gen=$g: $r" || { echo "FAIL hop gen=$g log=$l key=$k"; fail=1; };;
  *) echo "FAIL hop gen=$g: $r"; fail=1;; esac
done
kill $Q 2>/dev/null; wait $Q 2>/dev/null
# n6: the client stages half an image and vanishes (no lcom). The kernel must reset by itself: idle timeout
# (idle=10 here, 600 s by default) -> final log -> 60 s of petting -> 0xCF9. With -no-reboot QEMU then exits.
qemu-system-x86_64 -machine q35 -m 128 -display none -no-reboot -global ICH9-LPC.noreboot=false -serial file:/tmp/loader-q35-stall.log \
  -kernel $K -initrd test-keys/client.key -append "bmctl idle=10" -netdev user,id=n0,hostfwd=udp:127.0.0.1:15515-:5515 -device rtl8139,netdev=n0 &
Q=$!
for i in $(seq 40); do grep -q 'bmctl: listening' /tmp/loader-q35-stall.log 2>/dev/null && break; sleep 0.5; done
for i in $(seq 30); do $B ping >/dev/null 2>&1 && break; sleep 1; done
t0=$(date +%s); ab=$(BMCTL_TEST_SEND=300000 BMCTL_TEST_ABANDON=1 $B load $K bmctl x 2>&1)
case "$ab" in *"staged 300000 B"*"abandoned"*) ;; *) echo "FAIL n6: staging did not happen before abandoning: $ab"; fail=1;; esac
for i in $(seq 120); do kill -0 $Q 2>/dev/null || break; sleep 1; done
dt=$(( $(date +%s) - t0 ))
if kill -0 $Q 2>/dev/null; then echo "FAIL n6 stalled transfer: still running after ${dt} s"; fail=1
elif grep -q 'bmctl: idle 10 s -> reset' /tmp/loader-q35-stall.log && grep -q 'hard reset via 0xCF9' /tmp/loader-q35-stall.log; then
  echo "ok   n6 client vanished mid-transfer: idle reset + 0xCF9 after ${dt} s, no kick"
else echo "FAIL n6: exited after ${dt} s without the idle/0xCF9 lines"; fail=1; fi
echo "RESULT: $([ $fail = 0 ] && echo PASS || echo FAIL)"; exit $fail
