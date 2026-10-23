#!/usr/bin/env python3
"""q35 check of the touchpad/mouse path (QEMU PS/2 mouse driven over QMP): the pointer moves, a left click on the
home screen's S chip opens the SSH session, and inside it the remote's ?1002/?1006 mouse tracking gets SGR reports
(press, drag, release, wheel). Needs: tools/docker-sshd/run.sh sshd_config.mouse (the remote turns tracking on, then
dumps the 24 bytes it receives with od). Screenshots to argv[1]. Exit 1 on any failure."""
import json, os, socket, subprocess, sys, time
out = sys.argv[1] if len(sys.argv) > 1 else "/tmp/mouse-q35"
os.makedirs(out, exist_ok=True)
L, sock = "/tmp/mouse-q35.log", f"/tmp/mouse-q35-{os.getpid()}.qmp"
if os.path.exists(L): os.unlink(L)
q = subprocess.Popen(["qemu-system-x86_64", "-machine", "q35", "-m", "128", "-display", "none", "-no-reboot",
    "-global", "ICH9-LPC.noreboot=false", "-serial", f"file:{L}", "-qmp", f"unix:{sock},server=on,wait=off",
    "-kernel", "target/attended/x86_64-unknown-none/release/nc110-kernel", "-initrd", "test-keys/client.key",
    "-append", "bmctl nic=mmio", "-netdev", "user,id=n0", "-device", "rtl8139,netdev=n0"])
fail = 0
def log(): return open(L, errors="replace").read() if os.path.exists(L) else ""
def wait(p, secs):
    t = time.time()
    while time.time() - t < secs:
        if p in log(): return True
        time.sleep(0.3)
    return False
def check(ok, what):
    global fail
    print(("ok   " if ok else "FAIL ") + what); fail |= not ok
time.sleep(1)
s = socket.socket(socket.AF_UNIX); s.connect(sock); f = s.makefile("rw"); f.readline()
def qmp(c):
    f.write(json.dumps(c) + "\n"); f.flush()
    while True:
        r = json.loads(f.readline())
        if "return" in r or "error" in r: return r
qmp({"execute": "qmp_capabilities"})
def ev(*events): qmp({"execute": "input-send-event", "arguments": {"events": list(events)}}); time.sleep(0.15)
def move(dx, dy):  # small steps: PS/2 deltas are 9-bit
    while dx or dy:
        sx, sy = max(-60, min(60, dx)), max(-60, min(60, dy))
        ev({"type": "rel", "data": {"axis": "x", "value": sx}}, {"type": "rel", "data": {"axis": "y", "value": sy}})
        dx, dy = dx - sx, dy - sy
def btn(b, down): ev({"type": "btn", "data": {"down": down, "button": b}})
def shot(n): qmp({"execute": "screendump", "arguments": {"filename": f"{os.path.abspath(out)}/{n}.png", "format": "png"}})
check(wait("home: menu up", 40), "home menu up")
check("touchpad/mouse on" in log(), "PS/2 AUX mouse initialised: " + next((l.strip() for l in log().splitlines() if "touchpad/mouse" in l), "-"))
move(-400, 400)          # into the bottom-left corner (clamped)
move(81, -89)            # col 13, row 17: the S item (home layout 2026-10-21)
time.sleep(0.5); shot("1-pointer-on-S")
btn("left", True); btn("left", False)
check(wait("home: SSH session -> ", 5), "left click on the S item opened a session")
check(wait("MOUSE-READY", 30), "remote turned mouse tracking on")
time.sleep(1)
btn("left", True); move(24, 0); btn("left", False)      # press, drag 4 columns, release
ev({"type": "btn", "data": {"down": True, "button": "wheel-up"}}, {"type": "btn", "data": {"down": False, "button": "wheel-up"}})
time.sleep(3); shot("2-session")
lg = log()
got = lg[lg.find("MOUSE-READY"):]
print("remote received:", " ".join(l.strip() for l in got.splitlines()[1:4]))
flat = got.replace(" ", "").replace("\n", "")
check("033[<0;14;18M" in flat, "press reported at the click point (SGR 0;14;18 M)")
check("033[<32;18;18M" in flat, "drag reported 4 columns right with the button held (SGR 32;18;18 M)")
q.kill()
if os.path.exists(sock): os.unlink(sock)
print("RESULT:", "PASS" if not fail else "FAIL"); sys.exit(fail)
