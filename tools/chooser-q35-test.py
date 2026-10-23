#!/usr/bin/env python3
"""q35 check of the SSH host chooser: a second host added over bmctl (`host set`), S opens the chooser (two hosts),
Esc closes it, S then 2 connects to the second host, a bad pin is refused, `host del` drops back to one host (S
connects directly). Needs tools/docker-sshd/run.sh sshd_config.hold. Screenshot to argv[1]/chooser.png."""
import json, os, socket, subprocess, sys, time
out = sys.argv[1] if len(sys.argv) > 1 else "/tmp/chooser-q35"
os.makedirs(out, exist_ok=True)
L, sock = "/tmp/chooser-q35.log", f"/tmp/chooser-q35-{os.getpid()}.qmp"
if os.path.exists(L): os.unlink(L)
os.environ["NC110_KEY"] = "test-keys/client.key"
q = subprocess.Popen(["qemu-system-x86_64", "-machine", "q35", "-m", "128", "-display", "none", "-no-reboot",
    "-global", "ICH9-LPC.noreboot=false", "-serial", f"file:{L}", "-qmp", f"unix:{sock},server=on,wait=off",
    "-kernel", "target/attended/x86_64-unknown-none/release/nc110-kernel", "-initrd", "test-keys/client.key",
    "-append", "bmctl nic=mmio", "-netdev", "user,id=n0,hostfwd=udp:127.0.0.1:15515-:5515", "-device", "rtl8139,netdev=n0"])
fail = 0
def log(): return open(L, errors="replace").read() if os.path.exists(L) else ""
def wait(p, secs, count=1):
    t = time.time()
    while time.time() - t < secs:
        if log().count(p) >= count: return True
        time.sleep(0.3)
    return False
def check(ok, what):
    global fail
    print(("ok   " if ok else "FAIL ") + what); fail |= not ok
def bm(*a): return subprocess.run(["tools/.venv/bin/python", "tools/bmctl.py", "--host", "127.0.0.1", "--port", "15515", *a],
                                  capture_output=True, text=True).stdout.strip()
time.sleep(1)
s = socket.socket(socket.AF_UNIX); s.connect(sock); f = s.makefile("rw"); f.readline()
def qmp(c):
    f.write(json.dumps(c) + "\n"); f.flush()
    while True:
        r = json.loads(f.readline())
        if "return" in r or "error" in r: return r
qmp({"execute": "qmp_capabilities"})
def key(k): qmp({"execute": "send-key", "arguments": {"keys": [{"type": "qcode", "data": k}]}}); time.sleep(0.5)
pin = open("test-keys/host_pub.bin", "rb").read().hex()
check(wait("home: menu up", 40), "home menu up")
r = bm("host", "set", "1", "second", "192.168.50.95", "2223", "evnchn", "00" * 32 + "")
check(r.startswith("ok"), f"host set accepts a well-formed entry: {r}")
r = bm("host", "set", "2", "bad", "192.168.50.95", "2223", "evnchn", "zz" * 32)
check(r.startswith("err"), f"host set refuses a non-hex pin: {r}")
r = bm("host", "set", "1", "second", "192.168.50.95", "2223", "evnchn", pin)
check(r.startswith("ok"), f"host set with the real pin: {r}")
lst = bm("host", "list")
check("0 placeholder" in lst and "1 second" in lst, "host list shows both: " + lst.replace("\n", " | "))
key("s"); time.sleep(0.5)
qmp({"execute": "screendump", "arguments": {"filename": f"{os.path.abspath(out)}/chooser.png", "format": "png"}})
check("SSH session ->" not in log(), "S with two hosts opened the chooser, not a session")
key("esc"); key("s"); key("2")
check(wait("home: SSH session -> second", 10), "S, 2 connected to the second host")
check(wait("ssh: session loop ended", 60), "session ended")
check(bm("host", "del", "1").startswith("ok"), "host del")
key("s")
check(wait("home: SSH session -> placeholder", 10), "one host left: S connects directly")
q.kill()
if os.path.exists(sock): os.unlink(sock)
print("RESULT:", "PASS" if not fail else "FAIL"); sys.exit(fail)
