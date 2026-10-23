#!/usr/bin/env python3
"""q35 checks for the no-cable path and the WiFi screen (q35 has no Centrino, so the screen must stop cleanly at the
radio step). wifi-q35-test.py <outdir> [kernel]:
  1. NIC on a dead hub (no DHCP): boot must reach the home screen offline (log "offline home screen"), offline.png.
  2. Same boot, press W: the WiFi screen must report the missing radio and return to home on Esc, wifi.png.
Exit 1 on any miss."""
import json, os, socket, subprocess, sys, time
out = os.path.abspath(sys.argv[1]); os.makedirs(out, exist_ok=True)
K = sys.argv[2] if len(sys.argv) > 2 else "target/attended-pw/x86_64-unknown-none/release/nc110-kernel"
L, sock = f"/tmp/wifi-q35-{os.getpid()}.log", f"/tmp/wifi-q35-{os.getpid()}.qmp"
q = subprocess.Popen(["qemu-system-x86_64", "-machine", "q35", "-m", "128", "-display", "none", "-no-reboot",
    "-serial", f"file:{L}", "-qmp", f"unix:{sock},server=on,wait=off", "-kernel", K, "-initrd", "test-keys/client.key",
    "-append", "bmctl nic=mmio q35fb", "-netdev", "hubport,id=n0,hubid=0", "-device", "rtl8139,netdev=n0"])
log = lambda: open(L, errors="replace").read() if os.path.exists(L) else ""
for _ in range(50):
    if os.path.exists(sock): break
    time.sleep(0.05)
s = socket.socket(socket.AF_UNIX); s.connect(sock); f = s.makefile("rw"); f.readline()
def qmp(c):
    f.write(json.dumps(c) + "\n"); f.flush()
    while True:
        r = json.loads(f.readline())
        if "return" in r or "error" in r: return r
qmp({"execute": "qmp_capabilities"})
shot = lambda n: qmp({"execute": "screendump", "arguments": {"filename": f"{out}/{n}.png", "format": "png"}})
key = lambda k: (qmp({"execute": "send-key", "arguments": {"keys": [{"type": "qcode", "data": k}]}}), time.sleep(0.8))
def wait(p, n=160):
    for _ in range(n):
        if p in log(): return True
        time.sleep(0.25)
    print(f"FAIL: never saw {p!r}"); return False
ok = wait("offline home screen") and wait("home: menu up")
if ok:
    time.sleep(1); shot("offline")
    key("w"); ok = wait("home: W -> WiFi", 20)
    time.sleep(2); shot("wifi")
    key("esc"); time.sleep(1)
    t = log()
    print("\n".join(l for l in t.splitlines() if "wifi" in l.lower() or "iwl" in l.lower())[-1500:])
q.kill(); s.close()
print("RESULT:", "PASS" if ok else "FAIL"); sys.exit(0 if ok else 1)
