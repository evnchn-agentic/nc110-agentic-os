#!/usr/bin/env python3
"""q35 flood check on raw Ethernet (control review 2026-10-21): QEMU's dgram netdev hands this script every frame, so it
plays the whole LAN (ARP for any address, a DHCP server, the key holder at 10.9.0.10, and a flooder that can SPOOF
source addresses, which QEMU's user networking cannot). A `who` + `who2` flood runs at <pps> from one address
(single) or from random addresses (spoof) while the key holder pings once a second for <secs>; the run passes if at
least 80% of its authenticated pings succeed and the kernel does not reset. Speaks protocol v2 and v1, like bmctl.py.
Usage: flood-q35-test.py [kernel] [pps] [single|spoof] [secs]. Exit 1 on failure."""
import hashlib, hmac, os, random, socket, struct, subprocess, sys, threading, time
from cryptography.hazmat.primitives.ciphers.aead import ChaCha20Poly1305

K = sys.argv[1] if len(sys.argv) > 1 else "target/attended/x86_64-unknown-none/release/nc110-kernel"
PPS = int(sys.argv[2]) if len(sys.argv) > 2 else 5000
MODE = sys.argv[3] if len(sys.argv) > 3 else "spoof"
SECS = int(sys.argv[4]) if len(sys.argv) > 4 else 40
KIND = os.environ.get("FLOOD_KIND", "both")  # who | who2 | both (alternating)
QP = 31000 + os.getpid() % 1000; MP = QP + 1000
GW_MAC, GW, BOARD, CLIENT = bytes.fromhex("020000000001"), bytes([10, 9, 0, 1]), bytes([10, 9, 0, 2]), bytes([10, 9, 0, 10])
km = open("test-keys/client.key", "rb").read(); master = hashlib.sha256(b"nc110-bmctl-v1" + km[8:40]).digest()
L = f"/tmp/flood-q35-{os.getpid()}.log"
net = socket.socket(socket.AF_INET, socket.SOCK_DGRAM); net.bind(("127.0.0.1", MP))
net.setsockopt(socket.SOL_SOCKET, socket.SO_RCVBUF, 1 << 22)
board_mac = [b"\xff" * 6]
inbox, inbox_cv = [], threading.Condition()
stat = {"replies_to_flood": 0, "arp": 0, "sent": 0}


def csum(b):
    if len(b) % 2: b += b"\0"
    s = sum(struct.unpack(f"!{len(b) // 2}H", b)); s = (s >> 16) + (s & 0xFFFF); s += s >> 16
    return ~s & 0xFFFF


def udp_frame(src, dst, sport, dport, payload, dmac=None):
    u = struct.pack("!HHHH", sport, dport, 8 + len(payload), 0) + payload
    c = csum(src + dst + struct.pack("!BBH", 0, 17, len(u)) + u) or 0xFFFF
    u = u[:6] + struct.pack("!H", c) + u[8:]
    ip = struct.pack("!BBHHHBBH4s4s", 0x45, 0, 20 + len(u), 0, 0x4000, 64, 17, 0, src, dst)
    ip = ip[:10] + struct.pack("!H", csum(ip)) + ip[12:]
    f = (dmac or board_mac[0]) + GW_MAC + b"\x08\x00" + ip + u
    return f + b"\0" * max(0, 60 - len(f))


def tx(f):
    net.sendto(f, ("127.0.0.1", QP))


def dhcp_reply(req, mtype):
    xid, chaddr = req[4:8], req[28:44]
    b = struct.pack("!BBBB", 2, 1, 6, 0) + xid + b"\0" * 8 + BOARD + GW + b"\0" * 4 + chaddr + b"\0" * 192
    b += b"\x63\x82\x53\x63" + bytes([53, 1, mtype, 54, 4]) + GW + bytes([51, 4, 0, 1, 81, 128, 1, 4, 255, 255, 255, 0, 3, 4]) + GW + b"\xff"
    tx(udp_frame(GW, b"\xff" * 4, 67, 68, b, dmac=b"\xff" * 6))


def rx():
    while True:
        f, _ = net.recvfrom(4096)
        et = f[12:14]
        if et == b"\x08\x06" and f[20:22] == b"\x00\x01":  # ARP request: we are every address but the board
            board_mac[0] = f[22:28]; tip = f[38:42]
            if tip != BOARD:
                tx(f[6:12] + GW_MAC + b"\x08\x06" + f[14:20] + b"\x00\x02" + GW_MAC + tip + f[22:28] + f[28:32])
                stat["arp"] += 1
        elif et == b"\x08\x00" and f[23] == 17:
            ihl = (f[14] & 15) * 4; u = f[14 + ihl:]; sport, dport = struct.unpack("!HH", u[:4]); dst = f[30:34]
            if sport == 68 and dport == 67:
                board_mac[0] = f[6:12]; opts = u[8 + 240:]; i = 0; mt = 0
                while i < len(opts) and opts[i] != 255:
                    if opts[i] == 53: mt = opts[i + 2]
                    i += 2 + opts[i + 1] if opts[i] else 1
                if mt in (1, 3): dhcp_reply(u[8:], 2 if mt == 1 else 5)
            elif sport == 5515 and dst == CLIENT:
                with inbox_cv: inbox.append(u[8:struct.unpack("!H", u[4:6])[0]]); inbox_cv.notify_all()
            elif sport == 5515:
                stat["replies_to_flood"] += 1


def recv(timeout, want):
    end = time.time() + timeout
    with inbox_cv:
        while True:
            for i, d in enumerate(inbox):
                if want(d): return inbox.pop(i)
            left = end - time.time()
            if left <= 0: return None
            inbox_cv.wait(left)


CPORT = 40000 + random.randrange(20000)
send = lambda p: tx(udp_frame(CLIENT, BOARD, CPORT, 5515, p))


def session():
    """bmctl.py's handshake over this rig: who2 + who together, first verified reply wins."""
    nc2, nc1 = os.urandom(16), os.urandom(16)
    for _ in range(3):
        with inbox_cv: inbox.clear()
        send(b"who2" + nc2); send(b"who" + nc1)
        end = time.time() + 1
        while time.time() < end:
            d = recv(end - time.time(), lambda d: d.startswith(b"nc110 "))
            if not d: break
            w = d.decode(errors="replace").split()
            try:
                if w[:3] == ["nc110", "v2", "boot"] and len(w) == 10:
                    bid, sid, bk = bytes.fromhex(w[3]), int(w[5]), int(w[7])
                    if hmac.compare_digest(hashlib.sha256(b"nc110-bmctl-v2 who" + master + nc2 + bid + struct.pack("<IB", sid, bk)).digest()[:16], bytes.fromhex(w[9])):
                        k = hashlib.sha256(b"nc110-bmctl-v2 session" + master + bid + struct.pack("<I", sid) + nc2).digest()
                        return ChaCha20Poly1305(k), bid, struct.pack("<I", sid), (nc2, bk)
                elif w[:2] == ["nc110", "boot"] and len(w) == 7 and len(w[2]) == 32:
                    bid, sid = bytes.fromhex(w[2]), int(w[4])
                    if hmac.compare_digest(hashlib.sha256(b"nc110-bmctl-v1 who" + master + nc1 + bid + struct.pack("<I", sid)).digest()[:16], bytes.fromhex(w[6])):
                        k = hashlib.sha256(b"nc110-bmctl-v1 session" + master + bid + struct.pack("<I", sid) + nc1).digest()
                        return ChaCha20Poly1305(k), bid, struct.pack("<I", sid), None
            except ValueError:
                pass
    return None


def call(msg):
    s = session()
    if not s: return None
    a, bid, sid, v2 = s
    head = sid + (struct.pack("<Q", 1 | 1 << 63) + v2[0] + bytes([v2[1]]) if v2 else struct.pack("<Q", 1))
    f = head + a.encrypt(b"C2K\0" + struct.pack("<Q", 1), msg, bid)
    for _ in range(3):
        send(f)
        d = recv(1, lambda d: d[:8] == struct.pack("<Q", 1))
        if d:
            try: return a.decrypt(b"K2C\0" + d[:8], d[8:], bid).decode()
            except Exception: return None
    return None


stop = threading.Event()


def flood():
    t0, n = time.time(), 0
    while not stop.is_set():
        due = int((time.time() - t0) * PPS)
        while n < due:
            src = bytes([10, 9, random.randrange(1, 255), random.randrange(1, 255)]) if MODE == "spoof" else bytes([10, 9, 0, 66])
            if src in (BOARD, CLIENT, GW): src = bytes([10, 9, 0, 77])
            p = {"who": b"who", "who2": b"who2"}.get(KIND, b"who2" if n % 2 else b"who") + os.urandom(16)
            tx(udp_frame(src, BOARD, 1024 + n % 60000, 5515, p)); n += 1
        stat["sent"] = n
        time.sleep(0.002)


threading.Thread(target=rx, daemon=True).start()
q = subprocess.Popen(["qemu-system-x86_64", "-machine", "q35", "-m", "128", "-display", "none", "-no-reboot",
    "-global", "ICH9-LPC.noreboot=false", "-serial", f"file:{L}", "-kernel", K, "-initrd", "test-keys/client.key",
    "-append", "bmctl nic=mmio idle=30", "-netdev", f"dgram,id=n0,local.type=inet,local.host=127.0.0.1,local.port={QP},"
    f"remote.type=inet,remote.host=127.0.0.1,remote.port={MP}", "-device", "rtl8139,netdev=n0,mac=52:54:00:12:34:56"])
log = lambda: open(L, errors="replace").read() if os.path.exists(L) else ""
for _ in range(120):
    if "bmctl: listening" in log(): break
    time.sleep(0.25)
else:
    q.kill(); sys.exit("FAIL: bmctl never listened (DHCP over the rig?)")
base = call(b"ping"); print("baseline:", base)
threading.Thread(target=flood, daemon=True).start()
ok = tries = 0; t0 = time.time()
while time.time() - t0 < SECS and q.poll() is None:
    tries += 1; r = call(b"ping"); ok += bool(r and r.startswith("pong")); time.sleep(1)
el = time.time() - t0; stop.set(); time.sleep(0.3)
st = call(b"stats")
alive = q.poll() is None; q.kill()
pps = stat["sent"] / el
print(f"{MODE} flood {pps:.0f} pps for {el:.0f} s: legit pings {ok}/{tries}; kernel {'up' if alive else 'RESET'}; "
      f"replies to flood sources {stat['replies_to_flood']}, ARP answered {stat['arp']}")
print("stats:", st)
good = bool(base) and alive and tries and ok >= 0.8 * tries and pps >= 0.8 * PPS
print("RESULT:", "PASS" if good else "FAIL"); sys.exit(0 if good else 1)
