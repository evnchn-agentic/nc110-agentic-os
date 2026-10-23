#!/usr/bin/env python3
"""Push the NC110's SSH host list (the home screen's chooser) from ~/.nc110/hosts.json over bmctl.
The kernel keeps hosts in RAM only, so --watch re-pushes whenever the netbook's boot id changes (run as a LaunchAgent).
hosts.json: [{"slot": 0, "name": "placeholder", "host": "192.168.50.95", "port": 2223, "user": "evnchn"}, ...]
Each host's ed25519 key comes from ssh-keyscan on this Mac (trust on first use, LAN) unless "pin" (64 hex) is given;
the kernel refuses to connect to any other key. The NC110's public key must be in the user's authorized_keys there.
  tools/.venv/bin/python tools/nc110-hosts.py [--watch | --cfg]"""
import base64, json, os, subprocess, sys, time

BM = ["tools/.venv/bin/python", "tools/bmctl.py"]
CONF = os.path.expanduser("~/.nc110/hosts.json")


def bm(*a):
    r = subprocess.run(BM + list(a), capture_output=True, text=True, timeout=30)
    return r.returncode, r.stdout.strip()


def pin_of(h):
    if h.get("pin"):
        return h["pin"]
    out = subprocess.run(["ssh-keyscan", "-T", "5", "-t", "ed25519", "-p", str(h["port"]), h["host"]],
                         capture_output=True, text=True).stdout
    for line in out.splitlines():
        f = line.split()
        if len(f) >= 3 and f[1] == "ssh-ed25519":
            blob = base64.b64decode(f[2])
            return blob[-32:].hex()  # string "ssh-ed25519" + 32-byte key; the key is the tail
    raise RuntimeError(f"no ed25519 host key from {h['host']}:{h['port']}")


def push():
    hosts = json.load(open(CONF))
    for h in hosts:
        try:
            rc, out = bm("host", "set", str(h["slot"]), h["name"], h["host"], str(h["port"]), h["user"], pin_of(h))
        except Exception as e:
            rc, out = 1, repr(e)
        print(f"{time.strftime('%H:%M:%S')} slot {h['slot']} {h['name']}: {out}", flush=True)
    print(bm("host", "list")[1], flush=True)


def write_cfg():
    """--cfg: ~/.nc110/agentos.cfg, the boot config module (hosts with pins, plus WireGuard from ~/.nc110/wg.json +
    wg.key). It holds the WireGuard private key: mode 600, outside the repo; the install script copies it to /boot."""
    lines = ["NC110CFG1"]
    for h in json.load(open(CONF)):
        lines.append(f"host {h['slot']} {h['name']} {h['host']} {h['port']} {h['user']} {pin_of(h)}")
    wj = os.path.expanduser("~/.nc110/wg.json")
    if os.path.exists(wj):
        w = json.load(open(wj))
        key = base64.b64decode(open(os.path.expanduser("~/.nc110/wg.key")).read().strip()).hex()
        lines.append(f"wg set {key} {base64.b64decode(w['peer']).hex()} {w['endpoint']} {w['port']} {w['tip']}")
    out = os.path.expanduser("~/.nc110/agentos.cfg")
    fd = os.open(out + ".tmp", os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
    os.write(fd, ("\n".join(lines) + "\n").encode())
    os.close(fd)
    os.replace(out + ".tmp", out)
    print(f"wrote {out}: {len(lines) - 1} lines ({sum(l.startswith('host') for l in lines)} hosts, wg {'yes' if os.path.exists(wj) else 'no'})")


if "--cfg" in sys.argv:
    write_cfg()
    sys.exit(0)
if "--watch" not in sys.argv:
    push()
    sys.exit(0)
last = None
while True:
    try:
        rc, bid = bm("bootid")
        if rc == 0 and bid and bid != last:
            print(f"{time.strftime('%H:%M:%S')} boot {bid[:8]}: pushing hosts", flush=True)
            push()
            last = bid
    except Exception as e:
        print(f"{time.strftime('%H:%M:%S')} watch: {e!r}", flush=True)
    time.sleep(15)
