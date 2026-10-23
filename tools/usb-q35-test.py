#!/usr/bin/env python3
"""q35 check of the generic USB primitives over bmctl: `usb` enumerates a UHCI usb-kbd into dev 0, `usb ctl` reads its
device descriptor again, `usb in` reads a HID report off the interrupt endpoint after a QMP key press (data toggle
carried across two transfers), and a bad endpoint errors cleanly. Exit 1 on any failure."""
import json, os, socket, subprocess, sys, time
L, sock = "/tmp/usb-q35.log", f"/tmp/usb-q35-{os.getpid()}.qmp"
if os.path.exists(L): os.unlink(L)
env = dict(os.environ, NC110_KEY="test-keys/client.key")
q = subprocess.Popen(["qemu-system-x86_64", "-machine", "q35", "-m", "128", "-display", "none", "-no-reboot",
    "-global", "ICH9-LPC.noreboot=false", "-serial", f"file:{L}", "-qmp", f"unix:{sock},server=on,wait=off",
    "-kernel", "target/attended/x86_64-unknown-none/release/nc110-kernel", "-initrd", "test-keys/client.key",
    "-append", "bmctl nic=mmio", "-netdev", "user,id=n0,hostfwd=udp:127.0.0.1:15516-:5515", "-device", "rtl8139,netdev=n0",
    "-device", "ich9-usb-uhci1,id=u", "-device", "usb-kbd,bus=u.0"])
fail = 0
def check(ok, what):
    global fail
    print(("ok   " if ok else "FAIL ") + what); fail |= not ok
def bm(*a):
    return subprocess.run(["tools/.venv/bin/python", "tools/bmctl.py", "--host", "127.0.0.1", "--port", "15516", *a],
        capture_output=True, text=True, env=env, timeout=30).stdout.strip()
for _ in range(80):
    if os.path.exists(L) and "bmctl: listening" in open(L, errors="replace").read(): break
    time.sleep(0.5)
s = socket.socket(socket.AF_UNIX); s.connect(sock); f = s.makefile("rw"); f.readline()
def qmp(c):
    f.write(json.dumps(c) + "\n"); f.flush()
    while True:
        r = json.loads(f.readline())
        if "return" in r or "error" in r: return r
qmp({"execute": "qmp_capabilities"})
r = bm("usb"); print(r)
check("0627:0001" in r and "-> dev 0" in r, "usb-kbd enumerated as dev 0")
r = bm("usb", "ctl", "0", "8006000100001200"); print("ctl:", r)
check(r.startswith("ok 18:") and "27 06 01 00" in r, "usb ctl GET_DESCRIPTOR(device) returns 18 bytes with VID:PID")
def key(k, down): qmp({"execute": "input-send-event", "arguments": {"events": [{"type": "key", "data": {"down": down, "key": {"type": "qcode", "data": k}}}]}})
key("a", True); time.sleep(0.2)
r = bm("usb", "in", "0", "81", "8"); print("in 1:", r)
check(r.startswith("ok 8:") and " 04" in r, "usb in 81: HID report with usage 0x04 ('a') after a key press")
key("a", False); time.sleep(0.2)
r = bm("usb", "in", "0", "81", "8"); print("in 2:", r)
check(r == "ok 8: 00 00 00 00 00 00 00 00", "second usb in: key-up report (QEMU UHCI does not check toggles; real hardware will)")
r = bm("usb", "in", "0", "82", "8"); print("bad ep:", r)
check(r.startswith("err"), "unknown endpoint errors")
r = bm("usb", "out", "0", "01", "\u20ac0"); print("non-ASCII hex:", r)
check(r == "err ep / data hex", "usb out with non-ASCII hex is refused, no panic (review 1021)")
r = bm("ping"); check(r.startswith("pong"), "kernel still serving bmctl: " + r)
q.kill()
if os.path.exists(sock): os.unlink(sock)
print("RESULT:", "PASS" if not fail else "FAIL"); sys.exit(fail)
