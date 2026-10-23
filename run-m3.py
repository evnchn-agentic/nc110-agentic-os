#!/usr/bin/env python3
"""M3 proof: boot the kernel in QEMU, let it SSH into a throwaway sshd (127.0.0.1:2223, interactive /bin/sh via
ForceCommand, tools/sshd_config.m3), then TYPE into the guest's PS/2 keyboard with the monitor's `sendkey`.

Typed: `echo nc110-m3-okxy` + 2 Backspace, `echo $((6*7))` (shifted keys), `sleep 30` + Ctrl-C, printf with CSI D/K, 80 cols + SGR wrap,
`seq 1 30` (scroll), `uname -a`, `exit`.
Writes proof/m3-serial.log, m3-sshd.log, m3-screen.png (after uname output), m3-screen-exit.png, m3-net.pcap.
One QEMU (-m 256) and one sshd at a time; both are always killed on exit.
MODE=peerkill: after the prompt, SIGKILL sshd's per-connection child (peer vanishes without an SSH close);
               the kernel must notice the TCP close and end the session (proof/m3-peerkill-*).
MODE=cad:      after the prompt, press Ctrl-Alt-Del; the kernel must hard-reset (QEMU exits: -no-reboot).
"""
import os, socket, subprocess, sys, time

ROOT = os.path.dirname(os.path.abspath(__file__))
PROOF = os.path.join(ROOT, "proof")
KERNEL = os.path.join(ROOT, "target/x86_64-unknown-none/release/nc110-kernel")
MODE = os.environ.get("MODE", "")
SERIAL, SSHD_LOG, PNG0, PNG, PNG2, PCAP = (os.path.join(PROOF, "m3-" + (MODE + "-" if MODE else "") + n) for n in
    ("serial.log", "sshd.log", "screen-early.png", "screen.png", "screen-exit.png", "net.pcap"))
MON = os.path.join(PROOF, ".monitor.sock")
SSHD_CFG = os.path.join(ROOT, "tools/sshd_config.m3")
PIDFILE = os.path.join(ROOT, "test-keys/sshd-m3.pid")

KEYS = {" ": "spc", "-": "minus", "\n": "ret", "\b": "backspace", "$": "shift-4", "(": "shift-9", ")": "shift-0", "*": "shift-8", "%": "shift-5",
        "'": "apostrophe", "\\": "backslash", "[": "bracket_left"}


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


for p in (SERIAL, SSHD_LOG, PNG0, PNG, PNG2, PCAP, MON):
    if os.path.exists(p):
        os.unlink(p)

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
    steps = [
        wait_for(lambda s: prompts(s) >= 1, "shell prompt", 60),
    ]
    if all(steps) and MODE == "peerkill":
        time.sleep(0.5)
        def descendants(pid):
            out = subprocess.run(["pgrep", "-P", str(pid)], capture_output=True, text=True).stdout.split()
            return [d for c in out for d in [int(c), *descendants(int(c))]]
        victims = descendants(sshd.pid)  # sshd-session, its user child and the shell: all hold or feed the socket
        subprocess.run(["ps", "-o", "pid,ppid,comm", "-p", ",".join(map(str, victims))])
        for v in victims:
            os.kill(v, 9)
        print(f"  SIGKILLed {len(victims)} sshd descendants")
        steps.append(wait_for(lambda s: ("TCP connection gone" in s or "ssh: connection closed" in s) and "DONE" in s, "kernel notices the dead peer", 30))
        hmp(f"screendump {PNG} -f png")
        time.sleep(0.5)
    elif all(steps) and MODE == "cad":
        hmp("sendkey ctrl-alt-delete")
        steps.append(wait_for(lambda s: "Ctrl-Alt-Del -> hard reset" in s, "Ctrl-Alt-Del message", 10))
        try:
            q.wait(timeout=10)
            print("  ok: QEMU exited after the reset (-no-reboot), rc", q.returncode)
        except subprocess.TimeoutExpired:
            steps.append(False)
            print("  FAIL: QEMU still running after Ctrl-Alt-Del")
    elif all(steps):
        typ("echo nc110-m3-okxy\b\b\n")  # two Backspaces: the pty must erase "xy"
        steps.append(wait_for(lambda s: s.count("nc110-m3-ok") >= 2 and prompts(s) >= 2, "echo output", 20))
        typ("echo $((6*7))\n")
        steps.append(wait_for(lambda s: "\r\n42\r\n" in s and prompts(s) >= 3, "shifted keys -> 42", 20))
        typ("sleep 30\n")
        time.sleep(2)
        t_c = time.time()
        hmp("sendkey ctrl-c")
        print("  typing Ctrl-C")
        steps.append(wait_for(lambda s: prompts(s) >= 4, "prompt back after Ctrl-C (sleep 30 interrupted)", 10))
        print(f"  Ctrl-C to prompt: {time.time() - t_c:.1f}s")
        # Terminal escapes: CSI 1 D (cursor left) then CSI K (erase to EOL): screen must read "AXZ", not "ABXZ".
        typ("printf 'AB\\033[1DX\\033[KZ\\n'\n")
        steps.append(wait_for(lambda s: "\x1b[1DX\x1b[KZ" in s.replace("^[", "\x1b") and prompts(s) >= 5, "CSI D/K test", 20))
        # Deferred wrap must survive an ignored SGR: 80 zeros fill the row, then ESC[0m, then B starts the next row.
        typ("printf '%080d\\033[0mB\\n' 0\n")
        steps.append(wait_for(lambda s: "0\x1b[0mB" in s.replace("^[", "\x1b") and prompts(s) >= 6, "SGR keeps pending wrap", 20))
        time.sleep(0.3)
        hmp(f"screendump {PNG0} -f png")  # before scrolling: echo / 42 / ^C / CSI lines
        typ("seq 1 30\n")  # forces the 80x25 screen to scroll
        steps.append(wait_for(lambda s: "\r\n30\r\n" in s and prompts(s) >= 7, "scroll test", 20))
        typ("uname -a\n")
        steps.append(wait_for(lambda s: "Darwin" in s and prompts(s) >= 8, "uname -a output", 20))
        time.sleep(0.5)
        hmp(f"screendump {PNG} -f png")
        typ("exit\n")
        steps.append(wait_for(lambda s: "DONE" in s, "session closed, DONE", 20))
        time.sleep(0.3)
        hmp(f"screendump {PNG2} -f png")
        time.sleep(0.5)
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
