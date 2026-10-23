#!/usr/bin/env python3
"""q35 check of the attended kernel: argv[1]=kernel, argv[2]=cmdline. Prints arm/HANG/DONE lines with times and
how QEMU ended (-no-reboot: any reset, watchdog or 0xCF9, exits QEMU). QMP not used; -action watchdog=shutdown
makes a watchdog end distinguishable (exit via shutdown) from 0xCF9 (exit via reset)."""
import subprocess, sys, time
k, cl = sys.argv[1], sys.argv[2]
p = subprocess.Popen(["qemu-system-x86_64", "-machine", "q35", "-m", "128", "-display", "none", "-no-reboot",
    "-global", "ICH9-LPC.noreboot=false", "-action", "watchdog=poweroff", "-serial", "stdio", "-d", "guest_errors",
    "-kernel", k, "-append", cl, "-netdev", "user,id=n0", "-device", "rtl8139,netdev=n0"],
    stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
t0 = time.time(); last = ""
for line in p.stdout:
    l = line.decode(errors="replace").rstrip(); last = l
    if any(x in l for x in ("probe arm", "HANG", "DONE", "0xCF9", "OFFER", "halt:")):
        print(f"[{time.time()-t0:6.2f}] {l}", flush=True)
    if time.time() - t0 > 150:
        p.kill(); print("TIMEOUT"); break
p.wait(); print(f"qemu ended after {time.time()-t0:.1f}s, last line: {last[:80]}")
