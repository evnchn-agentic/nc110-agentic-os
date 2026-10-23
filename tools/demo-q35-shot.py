#!/usr/bin/env python3
"""Screenshot of the demo screen (home key D) in q35 at 1024x600 (q35fb). demo-q35-shot.py <out.png> [kernel] [key: d|g]"""
import json, os, socket, subprocess, sys, time
out = os.path.abspath(sys.argv[1])
K = sys.argv[2] if len(sys.argv) > 2 else "target/attended-pw/x86_64-unknown-none/release/nc110-kernel"
L, sock = f"/tmp/demo-q35-{os.getpid()}.log", f"/tmp/demo-q35-{os.getpid()}.qmp"
q = subprocess.Popen(["qemu-system-x86_64", "-machine", "q35", "-m", "128", "-display", "none", "-no-reboot",
    "-serial", f"file:{L}", "-qmp", f"unix:{sock},server=on,wait=off", "-kernel", K, "-initrd", "test-keys/client.key",
    "-append", "bmctl nic=mmio q35fb", "-netdev", "user,id=n0", "-device", "rtl8139,netdev=n0"])
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
for _ in range(160):
    if "home: menu up" in log(): break
    time.sleep(0.25)
time.sleep(1)
KEY = sys.argv[3] if len(sys.argv) > 3 else "d"
qmp({"execute": "screendump", "arguments": {"filename": out.replace(".png", "-home.png"), "format": "png"}})
qmp({"execute": "send-key", "arguments": {"keys": [{"type": "qcode", "data": KEY}]}})
time.sleep(1)
for k in "hello":
    qmp({"execute": "send-key", "arguments": {"keys": [{"type": "qcode", "data": k}]}}); time.sleep(0.1)
for i in range(40):  # a mouse stroke across the pad
    qmp({"execute": "input-send-event", "arguments": {"events": [{"type": "rel", "data": {"axis": "x", "value": 12}}, {"type": "rel", "data": {"axis": "y", "value": 5 if i < 20 else -5}}]}}); time.sleep(0.05)
time.sleep(2.5)
qmp({"execute": "screendump", "arguments": {"filename": out, "format": "png"}})
q.kill(); s.close(); print("shot", out)
