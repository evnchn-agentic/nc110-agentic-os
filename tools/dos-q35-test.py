"""q35 check (from the 2026-10-21 control review): a `who` flood from a host WITHOUT the key must not lock the key holder
out of bmctl or let the idle bound reset the board. Usage: dos-q35-test.py [kernel] [who|junk|none]. Exit 1 on failure:
under a flood, fewer than 80% of authenticated pings got through, or the kernel reset."""
import hashlib, os, socket, struct, subprocess, sys, threading, time
from cryptography.hazmat.primitives.ciphers.aead import ChaCha20Poly1305

KERNEL = sys.argv[1] if len(sys.argv) > 1 else "target/attended/x86_64-unknown-none/release/nc110-kernel"
KEY = "test-keys/client.key"
PORT = 25515
LOG = f"/tmp/dos-q35-{os.getpid()}.log"
mode = sys.argv[2] if len(sys.argv) > 2 else "who"
IDLE = 20
open(LOG, "w").close()
q = subprocess.Popen(["qemu-system-x86_64", "-machine", "q35", "-m", "128", "-display", "none", "-no-reboot",
    "-global", "ICH9-LPC.noreboot=false", "-serial", f"file:{LOG}", "-kernel", KERNEL, "-initrd", KEY,
    "-append", f"bmctl nic=mmio idle={IDLE}", "-netdev", f"user,id=n0,hostfwd=udp:127.0.0.1:{PORT}-:5515", "-device", "rtl8139,netdev=n0"])
for _ in range(80):
    if "bmctl: listening" in open(LOG, errors="replace").read():
        break
    time.sleep(0.5)
else:
    q.kill(); sys.exit("never listened")
t_listen = time.time()
km = open(KEY, "rb").read()
master = hashlib.sha256(b"nc110-bmctl-v1" + km[8:40]).digest()
ep = ("127.0.0.1", PORT)


def legit_ping():
    """One full bmctl.py-style run: who (3 tries, 1 s) then ping (3 tries). True on an authenticated pong."""
    s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM); s.settimeout(1.0)
    nc = os.urandom(16)
    w = None
    for _ in range(3):
        s.sendto(b"who" + nc, ep)
        try:
            w = s.recvfrom(256)[0].split(); break
        except socket.timeout:
            pass
    if not w:
        return False
    boot, sid = bytes.fromhex(w[2].decode()), int(w[4])
    a = ChaCha20Poly1305(hashlib.sha256(b"nc110-bmctl-v1 session" + master + boot + struct.pack("<I", sid) + nc).digest())
    f = struct.pack("<IQ", sid, 1) + a.encrypt(b"C2K\0" + struct.pack("<Q", 1), b"ping", boot)
    for _ in range(3):
        s.sendto(f, ep)
        try:
            r = s.recvfrom(2048)[0]
            return a.decrypt(b"K2C\0" + r[:8], r[8:], boot).startswith(b"pong")
        except socket.timeout:
            pass
    return False


stop = threading.Event()
sent = [0]


def flood():
    s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM); s.setblocking(False)
    while not stop.is_set():
        p = b"who" + os.urandom(16) if mode == "who" else os.urandom(28)
        try:
            s.sendto(p, ep); sent[0] += 1
        except BlockingIOError:
            pass
        if mode == "who":
            time.sleep(0.002) if sent[0] % 5 == 0 else None  # a few hundred pps of 19-byte datagrams


print("baseline (no flood):", legit_ping())
if mode != "none":
    threading.Thread(target=flood, daemon=True).start()
ok = tries = 0
t0 = time.time()
while time.time() - t0 < IDLE + 25 and q.poll() is None:
    tries += 1; ok += legit_ping(); time.sleep(2)
stop.set()
alive = q.poll() is None
print(f"mode={mode}: legit authenticated pings {ok}/{tries} during {time.time() - t0:.0f} s, flood packets {sent[0]}, "
      f"qemu {'still running' if alive else 'EXITED (reset) after %.0f s' % (time.time() - t_listen)}")
log = open(LOG, errors="replace").read()
for l in log.splitlines():
    if "idle" in l and "reset" in l or "hard reset" in l:
        print("serial:", l.strip())
if alive:
    q.kill()
pps = sent[0] / (time.time() - t0)
print(f"flood rate {pps:.0f} pps")
good = alive and ok >= 0.8 * tries and (mode == "none" or pps >= 300)  # a flood that never ran is not a pass
print("RESULT:", "PASS" if good else "FAIL"); sys.exit(0 if good else 1)
