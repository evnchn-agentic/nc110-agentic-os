#!/usr/bin/env python3
"""q35 check: a kernel panic restarts the board by itself via 0xCF9 within about 10 s, instead of halting until the TCO
fires (on the NC110 a TCO reset has left the board dark since the native-resolution default, 2026-10-21). Boots
cmdline 'bmctl nic=mmio wdt=panic' with -no-reboot, so the reset shows as QEMU exiting. Usage: panic-q35-test.py
[kernel]. Exit 1 unless the log shows the 0xCF9 reset and QEMU exits cleanly 8-20 s after the panic."""
import os, subprocess, sys, time
K = sys.argv[1] if len(sys.argv) > 1 else "target/attended/x86_64-unknown-none/release/nc110-kernel"
L = f"/tmp/panic-q35-{os.getpid()}.log"
q = subprocess.Popen(["qemu-system-x86_64", "-machine", "q35", "-m", "128", "-display", "none", "-no-reboot",
    "-global", "ICH9-LPC.noreboot=false", "-serial", f"file:{L}", "-kernel", K, "-initrd", "test-keys/client.key",
    "-append", "bmctl nic=mmio wdt=panic", "-netdev", "user,id=n0", "-device", "rtl8139,netdev=n0"])
log = lambda: open(L, errors="replace").read() if os.path.exists(L) else ""
t0 = time.time()
while "PANIC" not in log() and time.time() - t0 < 30 and q.poll() is None:
    time.sleep(0.2)
tp = time.time()
print("panic seen:", "PANIC" in log())
while q.poll() is None and time.time() - tp < 25:
    time.sleep(0.2)
dt = time.time() - tp
gone = q.poll() is not None
if not gone:
    q.kill()
print(f"qemu {'exited (reset) %.1f s after the panic' % dt if gone else 'still running 25 s after the panic'}")
for l in log().splitlines():
    if "PANIC" in l or "restarting" in l or "0xCF9" in l:
        print("serial:", l.strip())
ok = "PANIC" in log() and "hard reset via 0xCF9" in log() and gone and q.returncode == 0 and 8 <= dt <= 20
print("RESULT:", "PASS" if ok else "FAIL"); sys.exit(0 if ok else 1)
