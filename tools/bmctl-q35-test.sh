#!/bin/sh
# q35 check of bmctl: positive path + wrong key + replay + tamper + other-boot + retry cache. Exit 1 on any failure.
export NC110_KEY=test-keys/client.key  # q35 kernels get the test key module; the default is the agent-OS key
K=target/attended/x86_64-unknown-none/release/nc110-kernel; P=tools/.venv/bin/python; fail=0
qemu-system-x86_64 -machine q35 -m 128 -display none -no-reboot -global ICH9-LPC.noreboot=false -serial file:/tmp/bmctl-q35.log \
  -kernel $K -initrd test-keys/client.key -append "bmctl nic=mmio" -netdev user,id=n0,hostfwd=udp:127.0.0.1:15515-:5515 -device rtl8139,netdev=n0 &
Q=$!; trap 'kill $Q 2>/dev/null' EXIT
for i in $(seq 40); do grep -q 'bmctl: listening' /tmp/bmctl-q35.log 2>/dev/null && break; sleep 0.5; done
B="$P tools/bmctl.py --host 127.0.0.1 --port 15515"
r=$($B ping); echo "1 ping:    $r"; case "$r" in pong*) ;; *) fail=1;; esac
head -c 72 /dev/urandom > /tmp/bmctl-badkey.bin
r=$($B --key /tmp/bmctl-badkey.bin ping 2>&1); echo "2 bad key: $r"; case "$r" in *"wrong key"*) ;; *) fail=1;; esac
$P - <<'PY' || fail=1
import hashlib, os, socket, struct, subprocess, sys
from cryptography.hazmat.primitives.ciphers.aead import ChaCha20Poly1305
km = open("test-keys/client.key", "rb").read(); master = hashlib.sha256(b"nc110-bmctl-v1" + km[8:40]).digest()
s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM); s.settimeout(1.5); ep = ("127.0.0.1", 15515)
def who(sock=s):  # protocol v2: stateless cookie; returns (Nc, boot, session, bucket)
    nc = os.urandom(16); sock.sendto(b"who2" + nc, ep); w = sock.recvfrom(256)[0].split()
    assert w[:3] == [b"nc110", b"v2", b"boot"], w
    return nc, bytes.fromhex(w[3].decode()), int(w[5]), int(w[7])
def aead_for(nc, boot, sid):
    return ChaCha20Poly1305(hashlib.sha256(b"nc110-bmctl-v2 session" + master + boot + struct.pack("<I", sid) + nc).digest())
nc, boot, sid, bk = who(); a = aead_for(nc, boot, sid)
def frame(seq, msg=b"ping", aead=a, bid=boot, sess=sid, n=None, b=None):
    return (struct.pack("<IQ", sess, seq | 1 << 63) + (n or nc) + bytes([bk if b is None else b])
            + aead.encrypt(b"C2K\0" + struct.pack("<Q", seq), msg, bid))
bad = 0
s.sendto(frame(1), ep); r1 = s.recvfrom(2048)[0]
s.sendto(frame(1), ep); r2 = s.recvfrom(2048)[0]
print(("ok  " if r1 == r2 else "FAIL") + " 3 retry of seq 1 -> identical cached reply"); bad |= r1 != r2
s.sendto(frame(2), ep); s.recvfrom(2048)
def silent(p, what, sock=s):
    sock.sendto(p, ep)
    try: sock.recvfrom(2048); print(f"FAIL {what}: got a reply"); return 1
    except socket.timeout: print(f"ok   {what}: no reply"); return 0
bad |= silent(frame(1), "4 replay of an older accepted seq")
t = bytearray(frame(3)); t[29] ^= 1; bad |= silent(bytes(t), "5 tampered ciphertext")
bad |= silent(frame(4, bid=b"\0" * 16), "6 frame for another boot id")
bad |= silent(frame(5, sess=sid + 1000), "7 unknown session (no cookie)")
# 8: a replayed who reply (old boot+session) with a FRESH client nonce: the cookie no longer recomputes, rejected.
n8 = os.urandom(16)
bad |= silent(frame(1, aead=aead_for(n8, boot, sid), sess=sid, n=n8), "8 key from a replayed who reply + new Nc")
# 9: flood of who2 must not evict the live authenticated session (who2 creates nothing)
for _ in range(40):
    try: who()
    except socket.timeout: pass
s.sendto(frame(7), ep)
try: s.recvfrom(2048); print("ok   9 live session survives a who2 flood")
except socket.timeout: print("FAIL 9 live session evicted by who2 flood"); bad = 1
# 10: a forged first frame on a fresh cookie neither consumes nor corrupts it: the real first frame still works
nc2, boot2, sid2, bk2 = who(); a2 = aead_for(nc2, boot2, sid2)
t = bytearray(frame(1, aead=a2, sess=sid2, n=nc2, b=bk2)); t[40] ^= 1; s.sendto(bytes(t), ep)
try: s.recvfrom(2048); print("FAIL 10 forged first frame got a reply"); bad = 1
except socket.timeout: pass
f10 = frame(1, aead=a2, sess=sid2, n=nc2, b=bk2); s.sendto(f10, ep)
try: s.recvfrom(2048); print("ok   10 forged first frame ignored, real first frame then promotes the session")
except socket.timeout: print("FAIL 10 cookie session lost after a forged frame"); bad = 1
# 11: a cookie is bound to the endpoint: the same frame from another source port is rejected
s2 = socket.socket(socket.AF_INET, socket.SOCK_DGRAM); s2.settimeout(1.5)
nc3, b3, sid3, bk3 = who(); bad |= silent(frame(1, aead=aead_for(nc3, b3, sid3), sess=sid3, n=nc3, b=bk3), "11 cookie used from another port", s2)
# 12: evict session 10 and push 70 newer promotions past it (more than a 64-entry ring held: Codex MUST-FIX), then
# replay its first frame within the cookie window: never promoted twice
for _ in range(70):
    n_, b_, s_, k_ = who(); s.sendto(frame(1, aead=aead_for(n_, b_, s_), sess=s_, n=n_, b=k_), ep); s.recvfrom(2048)
bad |= silent(f10, "12 replayed first frame after its session was evicted")
# 13: an old (v1) client fails loudly against this kernel
r = subprocess.run(["tools/.venv/bin/python", "tools/bmctl_v1_head.py", "--host", "127.0.0.1", "--port", "15515", "ping"],
    capture_output=True, text=True, env=dict(os.environ, NC110_KEY="test-keys/client.key"))
ok13 = r.returncode != 0 and "v2-only" in r.stderr + r.stdout
print(("ok  " if ok13 else "FAIL") + " 13 v1 client exits naming v2-only: " + (r.stderr + r.stdout).strip()[-90:]); bad |= not ok13
sys.exit(bad)
PY
chk() { r=$($B $2); rc=$?; echo "$1: $r"; [ $rc = 0 ] || { echo "   ^ FAIL (client exit $rc)"; fail=1; }; case "$r" in $3) ;; *) echo "   ^ FAIL (want $3)"; fail=1;; esac; }
chk "p1 mem BIOS data area" "peek mem 0x400 16"        "????????????????????????????????"
printf '%s' "$r" | grep -Eqx '[0-9a-f]{32}' || { echo "   ^ FAIL (not 32 hex chars)"; fail=1; }
chk "p2 mem kernel image"   "peek mem 0x100000 16"     "err refused: kernel image*"
chk "p2b mem straddles image start" "peek mem 0xffff8 16" "err refused: kernel image*"
chk "p3 mem LAPIC (not RAM)" "peek mem 0xfee00000 4"   "err refused: not inside*"
chk "p3b mem 4 GiB boundary" "peek mem 0xfffffff8 16"  "err refused: not inside*"
chk "p4 mem len too big"    "peek mem 0x400 4096"      "err len*"
chk "p4b mem len zero"      "peek mem 0x400 0"         "err len*"
chk "p5 mem wraps"          "peek mem 0xffffffffffffffff 16" "err overflow"
chk "p6 nic regs 0..8"      "peek nic 0x0 8"           "12005452 00005634 "
chk "p7 nic past window"    "peek nic 0xfc 8"          "err nic*"
chk "p7b nic misaligned"    "peek nic 0x2 4"           "err nic*"
chk "p8 io TCO1_CNT"        "peek io 0x668 2"          "0x*"
chk "p8b io TCO1_CNT wrong width" "peek io 0x668 4"    "err io*"
chk "p9 io i8042 data"      "peek io 0x60 1"           "err io*"
chk "p9b io LV2 (C2 entry)" "peek io 0x614 1"          "err io*"
chk "p9c io wrap"           "peek io 0xffffffffffffffff 1" "err io*"
r=$($B stats); echo "8 stats:   $r"
r=$($B logall | grep -c .); echo "9 log:     $r lines"; [ "$r" -gt 5 ] || fail=1
r=$($B sleep 2); echo "9b sleep: $r"; case "$r" in "err sleep disabled"*) ;; *) fail=1;; esac
echo "10 reboot: $($B reboot)"; sleep 2
if kill -0 $Q 2>/dev/null; then echo "FAIL qemu still running after reboot"; fail=1; else echo "ok   qemu exited after reboot"; fi
echo "RESULT: $([ $fail = 0 ] && echo PASS || echo FAIL)"; exit $fail
