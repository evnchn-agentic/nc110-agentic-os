#!/usr/bin/env python3
"""Screenshots of the agent OS screens in q35 at the panel's 1024x600 (cmdline q35fb: Bochs VBE, not the GMA path), 1x.
looks-q35-shots.py <outdir> [kernel]: boot.png (early boot), home.png, log.png (L), ssh.png (S: the SSH terminal; needs
tools/docker-sshd/run.sh sshd_config.hold), then a second boot with cmdline recshow for rec.png (the REC overlay; QEMU
has no ALC269 to record from). Exit 1 if a screen is missing or the framebuffer never came up."""
import json, os, socket, subprocess, sys, time
out = os.path.abspath(sys.argv[1]); os.makedirs(out, exist_ok=True)
K = sys.argv[2] if len(sys.argv) > 2 else "target/attended/x86_64-unknown-none/release/nc110-kernel"
fb_ok = []


def run(cl, steps):
    L, sock = f"/tmp/looks-q35-{os.getpid()}.log", f"/tmp/looks-q35-{os.getpid()}.qmp"
    for p in (L, sock):
        if os.path.exists(p): os.unlink(p)
    q = subprocess.Popen(["qemu-system-x86_64", "-machine", "q35", "-m", "128", "-display", "none", "-no-reboot",
        "-global", "ICH9-LPC.noreboot=false", "-serial", f"file:{L}", "-qmp", f"unix:{sock},server=on,wait=off",
        "-kernel", K, "-initrd", "test-keys/client.key", "-append", cl,
        "-netdev", "user,id=n0", "-device", "rtl8139,netdev=n0"])
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
    def shot(n):
        if os.path.exists(f"{out}/{n}.png"): os.unlink(f"{out}/{n}.png")
        if "return" not in qmp({"execute": "screendump", "arguments": {"filename": f"{out}/{n}.png", "format": "png"}}):
            sys.exit(f"FAIL: screendump {n}")
    key = lambda k: (qmp({"execute": "send-key", "arguments": {"keys": [{"type": "qcode", "data": k}]}}), time.sleep(0.8))
    def wait(p):
        for _ in range(160):
            if p in log(): return
            time.sleep(0.05 if p == "q35fb:" else 0.25)
        sys.exit(f"FAIL: never saw {p!r}")
    steps(shot, key, wait)
    q.kill(); s.close()
    fb_ok.append("q35fb: Bochs VBE 1024x600x32: true" in log())


def main_run(shot, key, wait):
    wait("q35fb:"); time.sleep(0.2); shot("boot")
    wait("home: menu up"); time.sleep(1); shot("home")
    key("l"); time.sleep(0.5); shot("log"); key("l")
    key("s"); wait("ssh(d): first shell output"); time.sleep(1.5); shot("ssh")


def rec_run(shot, key, wait):
    wait("home: menu up"); time.sleep(1); shot("rec")


run("bmctl nic=mmio q35fb", main_run)
run("bmctl nic=mmio q35fb recshow", rec_run)
ok = all(fb_ok) and all(os.path.exists(f"{out}/{n}.png") for n in ("boot", "home", "log", "ssh", "rec"))
print("RESULT:", "PASS" if ok else "FAIL"); sys.exit(0 if ok else 1)
