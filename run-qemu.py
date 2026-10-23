#!/usr/bin/env python3
"""Boot the kernel in QEMU once, headless; save serial log, VGA screenshot and NIC pcap to proof/.

Usage: python3 run-qemu.py [kernel-elf]   (default: target/x86_64-unknown-none/release/nc110-kernel)
Env: TAG (proof file prefix), CPU (default max; qemu64 = no RDRAND), KEYMOD (key module, or none),
     APPEND (kernel cmdline), BRIDGE=1 (NIC behind a PCI bridge), EXTRA (more QEMU args), ISO (boot a GRUB ISO via -cdrom
     instead of -kernel/-initrd), DEADLINE (s).
One QEMU at a time, -m 256, hard deadline; QEMU is always killed on exit.
"""
import os, socket, subprocess, sys, time

ROOT = os.path.dirname(os.path.abspath(__file__))
PROOF = os.path.join(ROOT, "proof")
KERNEL = sys.argv[1] if len(sys.argv) > 1 else os.path.join(ROOT, "target/x86_64-unknown-none/release/nc110-kernel")
TAG = os.environ.get("TAG", "")  # e.g. TAG=m2- writes proof/m2-serial.log, m2-net.pcap, m2-screen.png
SERIAL, PCAP, PNG = (os.path.join(PROOF, TAG + n) for n in ("serial.log", "net.pcap" if TAG else "dhcp.pcap", "screen.png"))
MON = os.path.join(PROOF, ".monitor.sock")
DEADLINE = int(os.environ.get("DEADLINE", "60"))
KEYMOD = os.environ.get("KEYMOD", os.path.join(ROOT, "test-keys/client.key"))  # KEYMOD=none: boot without it
APPEND = os.environ.get("APPEND", "")  # kernel cmdline, e.g. "rng=dump nic=mmio"
BRIDGE = os.environ.get("BRIDGE", "") == "1"  # put the NIC behind a pci-bridge (bus 1) to exercise the walk

os.makedirs(PROOF, exist_ok=True)
for p in (SERIAL, PCAP, PNG, MON):
    if os.path.exists(p):
        os.unlink(p)

cmd = [
    "qemu-system-x86_64", "-machine", "pc", "-cpu", os.environ.get("CPU", "max"), "-m", "256", "-no-reboot",
    "-display", "none", "-vga", "std",
    "-serial", f"file:{SERIAL}",
    "-monitor", f"unix:{MON},server,nowait",
    "-netdev", "user,id=n0",
    *(["-device", "pci-bridge,chassis_nr=1,id=br0", "-device", "rtl8139,netdev=n0,mac=52:54:00:4e:43:11,bus=br0,addr=3"]
      if BRIDGE else ["-device", "rtl8139,netdev=n0,mac=52:54:00:4e:43:11"]),
    "-object", f"filter-dump,id=f0,netdev=n0,file={PCAP}",
    *(["-cdrom", os.environ["ISO"]] if os.environ.get("ISO") else
      ["-kernel", KERNEL, *(["-initrd", KEYMOD] if KEYMOD != "none" else []), *(["-append", APPEND] if APPEND else [])]),
    *os.environ.get("EXTRA", "").split(),  # e.g. EXTRA="-device ahci,id=sata0" (no disk attached)
]
print("+", " ".join(cmd))
q = subprocess.Popen(cmd)
try:
    t0 = time.time()
    done = False
    while time.time() - t0 < DEADLINE and q.poll() is None:
        if os.path.exists(SERIAL) and (b"DONE" in open(SERIAL, "rb").read() or b"PANIC" in open(SERIAL, "rb").read()):
            done = True
            break
        time.sleep(0.2)
    print(f"serial marker seen: {done} after {time.time() - t0:.1f}s")
    if q.poll() is None:
        s = socket.socket(socket.AF_UNIX)
        s.connect(MON)
        s.settimeout(3)
        for line in (f"screendump {PNG} -f png\n", "quit\n"):
            s.sendall(line.encode())
            time.sleep(0.5)
        s.close()
    q.wait(timeout=10)
finally:
    if q.poll() is None:
        q.kill()
    if os.path.exists(MON):
        os.unlink(MON)
print(open(SERIAL, errors="replace").read() if os.path.exists(SERIAL) else "(no serial log)")
sys.exit(0 if done else 1)
