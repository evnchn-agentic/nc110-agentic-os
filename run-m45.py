#!/usr/bin/env python3
"""M4.5 proof: terminal replies, UTF-8, bright backgrounds, window-change (QEMU, throwaway sshd on 127.0.0.1:2223,
tools/sshd_config.m45 = m4 at LogLevel DEBUG1 so the window-change request is logged). Typed with the monitor's `sendkey`
(which cannot type non-ASCII, so the UTF-8 goes in as printf octal escapes; the bytes reaching the terminal are the same):
printf 'box: ─│┌┐ deg: ° cjk: 中\n', a second line with 中文| and SGR 42 vs 102 backgrounds,
`LANG=en_US.UTF-8 /opt/homebrew/bin/htop` (~3 s, F10), and `/usr/bin/vi /tmp/nc110-m45.txt` with the user's normal
startup (no -u NONE): insert a line, echo v:termresponse (= our DA2) / v:termu7resp / &ambiwidth, :wq. The shell also
reads the four replies back itself (`stty raw; printf 6n 5n c >c; head -c 25 | od -c`).
Writes proof/m45-serial.log, m45-sshd.log, m45-net.pcap and m45-{printf,replies,htop,vi,vi-exit}.png.
One QEMU (-m 256) and one sshd at a time; both are always killed on exit.
"""
import os, socket, subprocess, sys, time

ROOT = os.path.dirname(os.path.abspath(__file__))
PROOF = os.path.join(ROOT, "proof")
KERNEL = os.path.join(ROOT, "target/x86_64-unknown-none/release/nc110-kernel")
SERIAL, SSHD_LOG, PCAP, PNG_PRINTF, PNG_REPLIES, PNG_HTOP, PNG_VI, PNG_VI_EXIT = (os.path.join(PROOF, "m45-" + n) for n in
    ("serial.log", "sshd.log", "net.pcap", "printf.png", "replies.png", "htop.png", "vi.png", "vi-exit.png"))
OUT = "/tmp/nc110-m45.txt"
MON = os.path.join(PROOF, ".monitor.sock")
SSHD_CFG = os.path.join(ROOT, "tools/sshd_config.m45")
PIDFILE = os.path.join(ROOT, "test-keys/sshd-m45.pid")

KEYS = {" ": "spc", "-": "minus", "\n": "ret", "\b": "backspace", "$": "shift-4", "(": "shift-9", ")": "shift-0", "*": "shift-8", "%": "shift-5",
        "'": "apostrophe", "\\": "backslash", "[": "bracket_left", ":": "shift-semicolon", "/": "slash", ".": "dot",
        "=": "equal", '"': "shift-apostrophe", "&": "shift-7", "+": "shift-equal", "_": "shift-minus", ";": "semicolon", "|": "shift-backslash", ">": "shift-dot",
        "<": "shift-comma", "]": "bracket_right"}


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


for p in (SERIAL, SSHD_LOG, PCAP, PNG_PRINTF, PNG_REPLIES, PNG_HTOP, PNG_VI, PNG_VI_EXIT, MON, OUT):
    if os.path.exists(p):
        os.unlink(p)

os.makedirs("/tmp/nc110-m45-home", exist_ok=True)
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
    # sh's line editor redraws the prompt when a long command line wraps, so count prompts only after a marker
    prompt_after = lambda s, m, k=1: (lambda i: i >= 0 and s.find("nc110$ ", i) >= 0)(nth(s, m, k))
    def nth(s, m, k):
        i = -1
        for _ in range(k):
            i = s.find(m, i + 1)
            if i < 0:
                return -1
        return i + len(m)
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
        typ(r"printf 'box: \342\224\200\342\224\202\342\224\214\342\224\220 deg: \302\260 cjk: \344\270\255\n'" + "\n")
        steps.append(wait_for(lambda s: prompt_after(s, "box: ─│┌┐ deg: ° cjk: 中\r\n"), "printf UTF-8 line", 20))
        typ(r"printf '\344\270\255\346\226\207|\033[30;42m 42 \033[102m 102 \033[0m|\n'" + "\n")
        steps.append(wait_for(lambda s: prompt_after(s, "中文|^[[30;42m 42 ^[[102m 102 ^[[0m|\r\n"), "printf wide + bright bg line", 20))
        shot(PNG_PRINTF)
        # the four replies, read back by the shell itself: raw tty, ask, read exactly 25 bytes, dump them
        typ(r"stty raw -echo; printf '\033[3;7H\033[6n\033[5n\033[c\033[>c'; head -c 25 | od -c; stty sane" + "\n")
        steps.append(wait_for(lambda s: prompt_after(s, "0000031"), "shell read back 25 reply bytes", 20))
        shot(PNG_REPLIES)
        typ("clear\n")
        time.sleep(0.5)
        typ("LANG=en_US.UTF-8 /opt/homebrew/bin/htop\n")
        steps.append(wait_for(lambda s: alt_on(s) >= 1, "htop entered the alternate screen", 20))
        time.sleep(3)
        shot(PNG_HTOP)
        key("f10")
        if not wait_for(lambda s: prompt_after(s, "^[[?1049l", 1), "htop quit on F10", 8):
            steps.append(False)
            key("q")
            wait_for(lambda s: prompt_after(s, "^[[?1049l", 1), "htop quit on q (fallback)", 8)
        typ("/usr/bin/vi " + OUT + "\n")
        steps.append(wait_for(lambda s: alt_on(s) >= 2, "vi entered the alternate screen", 20))
        steps.append(wait_for(lambda s: s.rfind("[New]") > s.rfind("^[[?1049h"), "vi drew its status line", 20))
        time.sleep(1.5)
        typ("ihello from nc110 m45")
        key("esc")
        time.sleep(0.6)
        typ(':echo "rv=" . v:termresponse . " u7=" . v:termu7resp . " aw=" . &ambiwidth\n')
        # our reply is not mirrored to COM1, so the DA2 string in the log can only be vim echoing what it received
        steps.append(wait_for(lambda s: "[>0;10;0c" in s[nth(s, "^[[?1049h", 2):], "vim echoes the DA2 reply it received", 10))
        shot(PNG_VI)
        typ(":wq\n")
        steps.append(wait_for(lambda s: prompt_after(s, "^[[?1049l", 2), "vi :wq, prompt back", 20))
        got = open(OUT).read() if os.path.exists(OUT) else None
        print(f"  {OUT} = {got!r}")
        steps.append(got == "hello from nc110 m45\n")
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
