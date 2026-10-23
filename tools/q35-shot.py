#!/usr/bin/env python3
"""Screenshot the attended kernel in q35: q35-shot.py <out.png> <seconds> [cmdline] (default "bmctl nic=mmio", test key).
Exits QEMU after the shot. UDP 5515 is forwarded to 127.0.0.1:15515 so bmctl can drive it meanwhile."""
import json, os, socket, subprocess, sys, time
out, secs = os.path.abspath(sys.argv[1]), float(sys.argv[2])
cl = sys.argv[3] if len(sys.argv) > 3 else "bmctl nic=mmio"
sock = f"/tmp/q35-shot-{os.getpid()}.qmp"
q = subprocess.Popen(["qemu-system-x86_64", "-machine", "q35", "-m", "128", "-display", "none", "-no-reboot",
    "-global", "ICH9-LPC.noreboot=false", "-serial", "file:/tmp/q35-shot.log", "-qmp", f"unix:{sock},server=on,wait=off",
    "-kernel", "target/attended/x86_64-unknown-none/release/nc110-kernel", "-initrd", "test-keys/client.key", "-append", cl,
    "-netdev", "user,id=n0,hostfwd=udp:127.0.0.1:15515-:5515", "-device", "rtl8139,netdev=n0"])
time.sleep(secs)
s = socket.socket(socket.AF_UNIX); s.connect(sock); f = s.makefile("rw")
f.readline()
for c in ({"execute": "qmp_capabilities"}, {"execute": "screendump", "arguments": {"filename": out, "format": "png"}}):
    f.write(json.dumps(c) + "\n"); f.flush(); print(f.readline().strip())
q.kill(); os.unlink(sock)
