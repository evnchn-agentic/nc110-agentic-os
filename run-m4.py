#!/usr/bin/env python3
"""M4 proof: full-screen apps over the M3 path (QEMU, throwaway sshd on 127.0.0.1:2223, tools/sshd_config.m4 which
passes the pty-req TERM through to the shell). Typed with the monitor's `sendkey`:
`echo TERM=$TERM` (must print xterm), `/opt/homebrew/bin/htop` (~3 s, then Down x3, then F10), and
`/usr/bin/vi /tmp/nc110-m4.txt` (insert two lines, Esc, Up arrow, append " +up", :wq; the file is checked on the host).
Writes proof/m4-serial.log, m4-sshd.log, m4-net.pcap and m4-{htop,htop-down,htop-exit,vi,vi-exit}.png.
One QEMU (-m 256) and one sshd at a time; both are always killed on exit.
"""
import os, socket, subprocess, sys, time

ROOT = os.path.dirname(os.path.abspath(__file__))
PROOF = os.path.join(ROOT, "proof")
KERNEL = os.path.join(ROOT, "target/x86_64-unknown-none/release/nc110-kernel")
SERIAL, SSHD_LOG, PCAP, PNG_HTOP, PNG_HTOP_DOWN, PNG_HTOP_EXIT, PNG_VI, PNG_VI_EXIT = (os.path.join(PROOF, "m4-" + n) for n in
    ("serial.log", "sshd.log", "net.pcap", "htop.png", "htop-down.png", "htop-exit.png", "vi.png", "vi-exit.png"))
MON = os.path.join(PROOF, ".monitor.sock")
SSHD_CFG = os.path.join(ROOT, "tools/sshd_config.m4")
PIDFILE = os.path.join(ROOT, "test-keys/sshd-m4.pid")

KEYS = {" ": "spc", "-": "minus", "\n": "ret", "\b": "backspace", "$": "shift-4", "(": "shift-9", ")": "shift-0", "*": "shift-8", "%": "shift-5",
        "'": "apostrophe", "\\": "backslash", "[": "bracket_left", ":": "shift-semicolon", "/": "slash", ".": "dot",
        "=": "equal", "+": "shift-equal", "_": "shift-minus"}


def serial():
    try:
        return open(SERIAL, "rb").read().decode(errors="replace")
    except FileNotFoundError:
        return ""


def wait_for(pred, what, timeout):
    t0 = time.time()
    while time.time() - t0 < timeout:
        if pred(serial()):
            print(f"  ok: {what} ({time.time() - t0:.1f}s)")
            return True
        time.sleep(0.2)
    print(f"  TIMEOUT waiting for {what}")
    return False


for p in (SERIAL, SSHD_LOG, PCAP, PNG_HTOP, PNG_HTOP_DOWN, PNG_HTOP_EXIT, PNG_VI, PNG_VI_EXIT, MON, "/tmp/nc110-m4.txt"):
    if os.path.exists(p):
        os.unlink(p)

os.makedirs("/tmp/nc110-m4-home", exist_ok=True)
sshd = subprocess.Popen(["/usr/sbin/sshd", "-D", "-e", "-f", SSHD_CFG], stderr=open(SSHD_LOG, "wb"))
time.sleep(1)
cmd = ["qemu-system-x86_64", "-machine", "pc", "-cpu", "max", "-m", "256", "-no-reboot",
       "-display", "none", "-vga", "std", "-serial", f"file:{SERIAL}", "-monitor", f"unix:{MON},server,nowait",
       "-netdev", "user,id=n0", "-device", "rtl8139,netdev=n0,mac=52:54:00:4e:43:11",
       "-object", f"filter-dump,id=f0,netdev=n0,file={PCAP}",
       "-kernel", KERNEL, "-initrd", os.path.join(ROOT, "test-keys/client.key")]
print("+", " ".join(cmd))
q = subprocess.Popen(cmd)
ok = False
try:
    time.sleep(1)
    mon = socket.socket(socket.AF_UNIX)
    mon.connect(MON)
    mon.setblocking(False)

    def hmp(line):
        mon.sendall((line + "\n").encode())
        time.sleep(0.08)
        try:
            mon.recv(65536)
        except BlockingIOError:
            pass

    def typ(s):
        print(f"  typing {s!r}")
        for ch in s:
            hmp("sendkey " + KEYS.get(ch, "shift-" + ch.lower() if ch.isupper() else ch))

    prompts = lambda s: s.count("nc110$ ")
    alt_on = lambda s: s.count("^[[?1049h")
    def shot(path):
        time.sleep(0.6)
        hmp(f"screendump {path} -f png")
        time.sleep(0.4)
        print(f"  screendump {os.path.relpath(path, ROOT)}")
    def key(k, n=1):
        for _ in range(n):
            print(f"  key {k}")
            hmp("sendkey " + k)
    steps = [wait_for(lambda s: prompts(s) >= 1, "shell prompt", 60)]
    if all(steps):
        typ("echo TERM=$TERM\n")
        steps.append(wait_for(lambda s: "\r\nTERM=xterm\r\n" in s and prompts(s) >= 2, "pty-req TERM reaches the shell", 20))
        # htop: alternate screen, colours, scroll region; arrows in DECCKM mode; F10 quits (q is the fallback).
        typ("/opt/homebrew/bin/htop\n")
        steps.append(wait_for(lambda s: alt_on(s) >= 1, "htop entered the alternate screen", 20))
        time.sleep(3)
        shot(PNG_HTOP)
        key("down", 3)
        shot(PNG_HTOP_DOWN)
        key("f10")
        if not wait_for(lambda s: prompts(s) >= 3, "htop quit on F10", 8):
            steps.append(False)
            key("q")
            wait_for(lambda s: prompts(s) >= 3, "htop quit on q (fallback)", 8)
        shot(PNG_HTOP_EXIT)
        # vi: insert two lines, Up arrow back to line 1, append to it, :wq; the file is checked on the host.
        typ("/usr/bin/vi /tmp/nc110-m4.txt\n")
        steps.append(wait_for(lambda s: alt_on(s) >= 2, "vi entered the alternate screen", 20))
        time.sleep(1.5)
        typ("ihello from nc110 m4\nsecond line")
        key("esc")
        time.sleep(0.6)
        key("up")
        typ("A +up")
        key("esc")
        time.sleep(0.6)
        shot(PNG_VI)
        typ(":wq\n")
        steps.append(wait_for(lambda s: prompts(s) >= 4, "vi :wq, prompt back", 20))
        got = open("/tmp/nc110-m4.txt").read() if os.path.exists("/tmp/nc110-m4.txt") else None
        print(f"  /tmp/nc110-m4.txt = {got!r}")
        steps.append(got == "hello from nc110 m4 +up\nsecond line\n")
        shot(PNG_VI_EXIT)
        typ("exit\n")
        steps.append(wait_for(lambda s: "DONE" in s, "session closed, DONE", 20))
    ok = all(steps)
    if q.poll() is None:
        hmp("quit")
        q.wait(timeout=10)
finally:
    if q.poll() is None:
        q.kill()
    sshd.terminate()
    sshd.wait(timeout=5)
    for p in (MON, PIDFILE):
        if os.path.exists(p):
            os.unlink(p)
print("RESULT:", "PASS" if ok else "FAIL")
sys.exit(0 if ok else 1)
