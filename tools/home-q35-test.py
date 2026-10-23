#!/usr/bin/env python3
"""q35 check of the agent OS home screen keys (cmdline 'bmctl'): menu up, L shows the log and L returns, S opens an
SSH session with bmctl still answering, the session's end returns home, R restarts. Needs the throwaway sshd:
tools/docker-sshd/run.sh sshd_config.hold. Screenshots to <outdir> (default /tmp/home-q35). Exit 1 on any failure."""
import json, os, socket, subprocess, sys, time
out = sys.argv[1] if len(sys.argv) > 1 else "/tmp/home-q35"
os.makedirs(out, exist_ok=True)
L, sock = "/tmp/home-q35.log", f"/tmp/home-q35-{os.getpid()}.qmp"
os.environ["NC110_KEY"] = "test-keys/client.key"
if os.path.exists(L): os.unlink(L)
q = subprocess.Popen(["qemu-system-x86_64", "-machine", "q35", "-m", "128", "-display", "none", "-no-reboot",
    "-global", "ICH9-LPC.noreboot=false", "-serial", f"file:{L}", "-qmp", f"unix:{sock},server=on,wait=off",
    "-kernel", "target/attended/x86_64-unknown-none/release/nc110-kernel", "-initrd", "test-keys/client.key",
    "-append", "bmctl nic=mmio", "-netdev", "user,id=n0,hostfwd=udp:127.0.0.1:15515-:5515", "-device", "rtl8139,netdev=n0"])
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
def key(k): qmp({"execute": "send-key", "arguments": {"keys": [{"type": "qcode", "data": k}]}}); time.sleep(0.5)
def shot(n): qmp({"execute": "screendump", "arguments": {"filename": f"{os.path.abspath(out)}/{n}.png", "format": "png"}})
def ping(): return subprocess.run(["tools/.venv/bin/python", "tools/bmctl.py", "--host", "127.0.0.1", "--port", "15515", "ping"],
                                  capture_output=True, text=True).stdout
check(wait("home: menu up", 40), "home menu up"); time.sleep(1.5); shot("1-home")
key("l"); time.sleep(0.5); shot("2-log"); key("l"); time.sleep(0.5); shot("3-home-again")
check(ping().startswith("pong"), "bmctl ping at home")
key("s")
check(wait("term: VGA now belongs", 40), "S opened an SSH session"); time.sleep(1); shot("4-ssh")
check(ping().startswith("pong"), "bmctl ping during the SSH session")
check(wait("ssh: session loop ended", 60), "session ended (shell exits after 40 s)"); time.sleep(1.5); shot("5-home-after")
check(ping().startswith("pong"), "bmctl ping after the session")
key("r"); time.sleep(2)
check(q.poll() is not None, "R restarted (qemu exited)")
q.kill()
if os.path.exists(sock): os.unlink(sock)
print("RESULT:", "PASS" if not fail else "FAIL"); sys.exit(fail)
