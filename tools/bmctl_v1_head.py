#!/usr/bin/env python3
"""bmctl client for the NC110 kernel's authenticated UDP control port (see src/bmctl.rs).

Usage: tools/.venv/bin/python tools/bmctl.py [--host 192.168.50.120] [--key test-keys/client.key] <command...>
  commands: ping | stats | log [offset] | logall | peek ... | reboot | rng <total> <outfile>  (raw jitter samples)
            rec <ms> | aud | audget <out.wav>  (internal-mic clip, cmdline hda; 16 kHz mono)
            linux <bzImage> [cmdline...]     stage an i386 Linux bzImage and boot it (32-bit protocol, no initrd):
                                             the remote route back to Slot A Linux, e.g. root=/dev/sda2 rootwait
            load <kernel.elf> [cmdline...]   stage over the network, verify, chain-load in RAM (nothing on /boot),
                                             then wait for the new kernel's bmctl (new boot id) and print its ping
The key file is the same 72-byte multiboot key module the kernel gets (magic 8 + ed25519 seed 32 + pub 32);
master = SHA-256("nc110-bmctl-v1" + seed). Handshake: send "who" + fresh random 16-byte Nc; the reply carries boot id,
session and check = SHA-256("nc110-bmctl-v1 who" + master + Nc + boot + session)[:16], verified here; traffic key =
SHA-256("nc110-bmctl-v1 session" + master + boot + session_u32_le + Nc); seq counts from 1 per session. Uses the `cryptography` package's ChaCha20Poly1305,
an implementation independent of the kernel's, so agreement is a real interop check.
"""
import argparse, hashlib, hmac, os, socket, struct, sys, time
from cryptography.hazmat.primitives.ciphers.aead import ChaCha20Poly1305

ap = argparse.ArgumentParser()
ap.add_argument("--host", default="192.168.50.120")
ap.add_argument("--port", type=int, default=5515)
_ak = os.path.expanduser("~/.nc110/agentos.key")  # the agent OS key (outside the repo); test key otherwise
ap.add_argument("--key", default=os.environ.get("NC110_KEY") or (_ak if os.path.exists(_ak) else "test-keys/client.key"))
ap.add_argument("cmd", nargs="+")
a = ap.parse_args()
km = open(a.key, "rb").read()
assert len(km) == 72, "key module must be 72 bytes"
master = hashlib.sha256(b"nc110-bmctl-v1" + km[8:40]).digest()
s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)


def rpc(raw, want=lambda d: True):
    """Send `raw`, return the first reply from the target that `want` accepts. Stale datagrams (late answers to
    an earlier timed-out attempt) are drained before sending and skipped while waiting, so a resend can never
    leave the client one reply behind."""
    host = socket.gethostbyname(a.host)
    for _ in range(3):  # a retry of the same frame gets the kernel's cached reply, nothing runs twice
        s.setblocking(False)
        try:
            while True:
                s.recvfrom(2048)
        except (BlockingIOError, OSError):
            pass
        s.settimeout(TIMEOUT[0])
        s.sendto(raw, (a.host, a.port))
        end = time.monotonic() + TIMEOUT[0]
        try:
            while time.monotonic() < end:
                d, src = s.recvfrom(2048)
                if src[0] == host and want(d):
                    return d
        except socket.timeout:
            continue
    sys.exit(f"no reply from {a.host}:{a.port}")


TIMEOUT = [2.0]


def handshake():
    """Authenticated session setup; returns (aead, boot_id, session). Exits on a bad/forged reply."""
    nc = os.urandom(16)
    who = rpc(b"who" + nc, lambda d: d.startswith(b"nc110 boot ")).decode().split()
    if len(who) != 7 or who[:2] != ["nc110", "boot"] or who[3] != "session" or who[5] != "check" or len(who[2]) != 32:
        sys.exit(f"bad who reply: {who}")
    bid, sess = bytes.fromhex(who[2]), int(who[4])
    want = hashlib.sha256(b"nc110-bmctl-v1 who" + master + nc + bid + struct.pack("<I", sess)).digest()[:16]
    if not hmac.compare_digest(want, bytes.fromhex(who[6])):
        sys.exit("who reply failed its check (wrong key, or a forged/replayed reply)")
    k = hashlib.sha256(b"nc110-bmctl-v1 session" + master + bid + struct.pack("<I", sess) + nc).digest()
    return ChaCha20Poly1305(k), bid, sess


aead, boot, sid = handshake()
SEQ = [0]


def call(cmd):
    return call_raw(cmd.encode())


def call_raw(body):
    SEQ[0] += 1  # per-session counter from 1; a fresh session per run, so (key, nonce) never repeats
    seq = SEQ[0]
    ct = aead.encrypt(b"C2K\0" + struct.pack("<Q", seq), body, boot)
    r = rpc(struct.pack("<IQ", sid, seq) + ct, lambda d: len(d) >= 24 and d[:8] == struct.pack("<Q", seq))
    rseq = struct.unpack("<Q", r[:8])[0]
    if rseq != seq:
        sys.exit(f"reply seq {rseq} != {seq}")
    return aead.decrypt(b"K2C\0" + struct.pack("<Q", seq), r[8:], boot)


cmd = " ".join(a.cmd)
if a.cmd[0] in ("load", "linux"):
    img = open(a.cmd[1], "rb").read()
    cl = " ".join(a.cmd[2:])
    sha = os.environ.get("BMCTL_TEST_SHA") or hashlib.sha256(img).hexdigest()  # test hooks: wrong digest,
    if os.environ.get("BMCTL_TEST_SEND"):                                      # truncated send,
        img_send = img[: int(os.environ["BMCTL_TEST_SEND"])]
    else:
        img_send = img
    r = call(f"lbeg {len(img)} {sha}").decode()
    print(r)
    if not r.startswith("ok"):
        sys.exit(1)
    t0 = time.time()
    offs = list(range(0, len(img_send), 1000))
    if os.environ.get("BMCTL_TEST_SWAP") and len(offs) > 2:                    # out-of-order chunks
        offs[1], offs[2] = offs[2], offs[1]
    lat = []
    for off in offs:
        t1 = time.monotonic()
        r = call_raw(b"ldat" + struct.pack("<I", off) + img_send[off:off + 1000]).decode()
        lat.append(time.monotonic() - t1)
        if not r.startswith("ok"):
            sys.exit(f"ldat at {off}: {r}")
    if os.environ.get("BMCTL_TIMING"):
        lat.sort()
        q = lambda f: lat[min(len(lat) - 1, int(f * len(lat)))] * 1000
        print(f"ldat latency ms: p10 {q(.1):.1f} p50 {q(.5):.1f} p90 {q(.9):.1f} p99 {q(.99):.1f} max {lat[-1]*1000:.0f}, "
              f">1 s: {sum(x > 1 for x in lat)}")
    print(f"staged {len(img_send)} B in {time.time() - t0:.1f} s")
    if os.environ.get("BMCTL_TEST_ABANDON"):                                   # client dies mid-transfer
        sys.exit("abandoned (test hook): no lcom")
    if a.cmd[0] == "linux":
        r = call(f"lcomlinux {cl}".strip()).decode()
        print(r)
        sys.exit(0 if r.startswith("ok") else 1)  # Linux answers on SSH, not bmctl: the caller checks that
    try:
        r = call(f"lcom {cl}".strip()).decode()
    except SystemExit:
        r = "ok (lcom reply lost; the kernel may already have jumped)"
    print(r)
    if not r.startswith("ok"):
        sys.exit(1)
    old = boot
    deadline = time.monotonic() + 60
    while time.monotonic() < deadline:  # wait for an AUTHENTICATED answer from a kernel with a new boot id
        time.sleep(1)
        try:
            TIMEOUT[0] = 1.0
            aead, boot, sid = handshake()
            SEQ[0] = 0
            pong = call("ping").decode() if boot != old else None
        except (OSError, ValueError, SystemExit):  # rpc() and handshake() signal failure with SystemExit
            continue
        if pong:
            print(f"new kernel up: boot {boot.hex()} after {time.time() - t0:.1f} s; {pong}")
            sys.exit(0)
    sys.exit("no new kernel answered (authenticated) within 60 s")
if a.cmd[0] == "rng":
    total, out = int(a.cmd[1]), bytearray()
    if total <= 0:
        sys.exit("rng: total must be > 0")
    while len(out) < total:
        k = min(1200, total - len(out))
        chunk = call(f"rng {k}")
        if len(chunk) != k:  # an older kernel answers 'err unknown command': never let text become "samples"
            sys.exit(f"rng: asked for {k} samples, got {len(chunk)} bytes: {chunk[:60]!r}")
        out += chunk
    open(a.cmd[2], "wb").write(out)
    print(f"wrote {len(out)} raw samples to {a.cmd[2]}")
    sys.exit(0)
if a.cmd[0] == "audget":  # audget <out.wav>: pull the finished clip (16 kHz mono s16le) and write a WAV
    import wave
    st = call("aud").decode().split()
    if st[:1] != ["clip"] or st[5] != "0":
        sys.exit(f"aud: no finished clip: {' '.join(st)}")
    total, out = int(st[3]), bytearray()
    while len(out) < total:
        chunk = call(f"aud {len(out)}")
        if not chunk:
            sys.exit(f"aud: short read at {len(out)} of {total}")
        out += chunk
    with wave.open(a.cmd[1], "wb") as w:
        w.setnchannels(1); w.setsampwidth(2); w.setframerate(16000); w.writeframes(bytes(out))
    print(f"clip {st[1]}: {total // 2} samples ({total / 32000:.1f} s), peak {st[7]} -> {a.cmd[1]}")
    sys.exit(0)
if a.cmd[0] == "acpiget":  # acpiget <SIG> <file>: the whole ACPI table (e.g. DSDT for iasl -d)
    info = call(f"acpi {a.cmd[1]}").decode()
    if not info.split()[-2:-1] == ["length"]:
        sys.exit(info)
    total, out = int(info.split()[-1]), bytearray()
    while len(out) < total:
        chunk = call(f"acpi {a.cmd[1]} {len(out)}")
        if not chunk:
            sys.exit(f"short read at {len(out)}")
        out += chunk
    open(a.cmd[2], "wb").write(out)
    print(f"{info} -> {a.cmd[2]}")
    sys.exit(0)
if cmd == "screen":  # the netbook's screen as text (rows from the console's shadow grid)
    cols, rows = map(int, call("screen").decode().split("x"))
    for r in range(rows):
        print(call(f"screen {r}").decode(errors="replace").rstrip())
    sys.exit(0)
if cmd == "bootid":  # this boot's id from the handshake (changes on every reboot / chain-load)
    print(boot.hex())
    sys.exit(0)
if cmd == "logall":
    off, out = 0, b""
    while True:
        chunk = call(f"log {off}")
        if not chunk:
            break
        out += chunk
        off += len(chunk)
    sys.stdout.write(out.decode(errors="replace"))
else:
    print(call(cmd).decode(errors="replace"))
