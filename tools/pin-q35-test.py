"""q35 check (from the 2026-10-21 control review): an authenticated `host set` with a 64-BYTE but non-ASCII pin must be
refused with an error, not panic (usb-q35-test.py covers the same pattern in `usb out`) the kernel (a panic now means a dark board).
Usage: pin-q35-test.py [kernel]. Exit 1 on failure."""
import os, subprocess, sys, time

KERNEL = sys.argv[1] if len(sys.argv) > 1 else "target/attended/x86_64-unknown-none/release/nc110-kernel"
KEY = "test-keys/client.key"
PORT = 25516
LOG = f"/tmp/pin-q35-{os.getpid()}.log"
open(LOG, "w").close()
q = subprocess.Popen(["qemu-system-x86_64", "-machine", "q35", "-m", "128", "-display", "none", "-no-reboot",
    "-global", "ICH9-LPC.noreboot=false", "-serial", f"file:{LOG}", "-kernel", KERNEL, "-initrd", KEY,
    "-append", "bmctl nic=mmio", "-netdev", f"user,id=n0,hostfwd=udp:127.0.0.1:{PORT}-:5515", "-device", "rtl8139,netdev=n0"])
for _ in range(80):
    if "bmctl: listening" in open(LOG, errors="replace").read():
        break
    time.sleep(0.5)
B = ["tools/.venv/bin/python", "tools/bmctl.py", "--host", "127.0.0.1",
     "--port", str(PORT), "--key", KEY]
env = dict(os.environ, NC110_KEY=KEY)
print("ping:", subprocess.run(B + ["ping"], capture_output=True, text=True, env=env).stdout.strip())
pin = "€" + "0" * 61
assert len(pin.encode()) == 64
t0 = time.time()
r = subprocess.run(B + ["host", "set", "0", "x", "1.2.3.4", "22", "u", pin], capture_output=True, text=True, env=env)
print("host set (non-ASCII pin):", (r.stdout + r.stderr).strip())
print("ping after:", (lambda r: (r.stdout + r.stderr).strip())(subprocess.run(B + ["ping"], capture_output=True, text=True, env=env)))
time.sleep(3)
print("qemu:", "EXITED (reset) %.0f s after the command" % (time.time() - t0) if q.poll() is not None else "still running")
for l in open(LOG, errors="replace").read().splitlines():
    if "PANIC" in l or "wdt:" in l:
        print("serial:", l.strip())
r2 = subprocess.run(B + ["ping"], capture_output=True, text=True, env=env).stdout
good = "pin must be 64 hex digits" in (r.stdout + r.stderr) and r2.startswith("pong") and q.poll() is None
if q.poll() is None:
    q.kill()
print("RESULT:", "PASS" if good else "FAIL"); sys.exit(0 if good else 1)
