#!/usr/bin/env python3
"""M0.5 watchdog rehearsal: boot the kernel on QEMU q35 (ICH9 LPC + TCO) WITHOUT -no-reboot and
record serial lines and QMP events (WATCHDOG, RESET) with host timestamps.

Usage: python3 run-wdt.py <name> <kernel-elf | grub.iso> <cmdline (ignored for .iso)> <banners-to-wait-for> <deadline-s> <expect: watchdog|none|refuse> [locked]
Pass for `watchdog`: a HANG marker, then a QMP WATCHDOG(action=reset) at most 7.5 s after it and at least 4.5 s
after that boot started (TMR=5: 2 x 5 x 0.6 s = 6 s from the last reload, which may precede HANG), then the next
boot banner. Pass for `none`: a HANG marker, no WATCHDOG and no RESET event before the deadline.
Pass for `refuse`: GRUB printed "NOT arming", our kernel never booted, no WATCHDOG event.
Writes proof/wdt-<name>.log. One QEMU, -m 128, always killed on exit.
"""
import json, os, socket, subprocess, sys, threading, time

ROOT = os.path.dirname(os.path.abspath(__file__))
PROOF = os.path.join(ROOT, "proof")
name, kernel, cmdline, want, deadline = sys.argv[1], sys.argv[2], sys.argv[3], int(sys.argv[4]), float(sys.argv[5])
expect = sys.argv[6]
locked = len(sys.argv) > 7 and sys.argv[7] == "locked"  # also require the 'after:' line to show TCO_EN and TCO_LOCK still set
assert expect in ("watchdog", "none", "refuse")
ev = []  # (t, kind)
SERIAL = os.path.join(PROOF, f".wdt-{name}.serial")
QMP = os.path.join(PROOF, ".qmp.sock")
OUT = os.path.join(PROOF, f"wdt-{name}.log")
for p in (SERIAL, QMP):
    if os.path.exists(p):
        os.unlink(p)

cmd = [
    "qemu-system-x86_64", "-machine", "q35", "-m", "128", "-display", "none", "-vga", "std",
    "-global", "ICH9-LPC.noreboot=false",          # explicit: no NO_REBOOT pin strap
    "-action", "watchdog=reset",                     # explicit: QEMU default, stated for the record
    "-serial", f"file:{SERIAL}", "-qmp", f"unix:{QMP},server,wait=off",
] + sum((["-trace", t] for t in os.environ.get("WDT_TRACE", "").split(",") if t), []) + (["-cdrom", kernel, "-boot", "d"] if kernel.endswith(".iso") else ["-kernel", kernel, "-append", cmdline])
log = []
t0 = time.time()
def rec(src, msg):
    ev.append((time.time() - t0, f"{src}: {msg}"))
    log.append(f"[{time.time() - t0:7.3f}s] {src}: {msg}")
    print(log[-1], flush=True)

rec("host", " ".join(cmd))
q = subprocess.Popen(cmd)
stop = threading.Event()

def qmp_reader():
    for _ in range(50):
        try:
            s = socket.socket(socket.AF_UNIX); s.connect(QMP); break
        except OSError:
            time.sleep(0.1)
    else:
        rec("qmp", "connect failed"); return
    f = s.makefile("rwb")
    f.readline()
    f.write(b'{"execute":"qmp_capabilities"}\n'); f.flush()
    while not stop.is_set():
        try:
            line = f.readline()
        except OSError:
            break
        if not line:
            break
        m = json.loads(line)
        if "event" in m:
            rec("qmp", f"{m['event']} {json.dumps(m.get('data', {}))}")
    s.close()

th = threading.Thread(target=qmp_reader, daemon=True); th.start()
pos, banners, buf, settle = 0, 0, b"", 0.0
try:
    while time.time() - t0 < deadline and q.poll() is None:
        if os.path.exists(SERIAL):
            with open(SERIAL, "rb") as fh:
                fh.seek(pos); chunk = fh.read(); pos += len(chunk)
            buf += chunk
            while b"\n" in buf:
                line, buf = buf.split(b"\n", 1)
                text = line.decode(errors="replace")
                rec("serial", text)
                if "NC110 bare-metal" in text:
                    banners += 1
                    rec("host", f"boot #{banners}")
        if banners >= want:
            settle = settle or time.time() + 1.5  # let boot #want print its status lines
            if time.time() > settle:
                break
        time.sleep(0.05)
    rec("host", f"stop: banners={banners} want={want} elapsed={time.time() - t0:.1f}s")
finally:
    stop.set()
    if q.poll() is None:
        q.kill()
    q.wait(timeout=10)
    for p in (SERIAL, QMP):
        if os.path.exists(p):
            os.unlink(p)
hang = next((t for t, m in ev if "HANG now" in m), None)
wd = next((t for t, m in ev if m.startswith("qmp: WATCHDOG") and '"action": "reset"' in m and hang is not None and t > hang), None)
rst = [t for t, m in ev if m.startswith("qmp: RESET")]
nxt = next((t for t, m in ev if "NC110 bare-metal" in m and wd is not None and t > wd), None)
start = max((t for t, m in ev if hang is not None and t <= hang and ("grub: mode" in m or "NC110 bare-metal" in m)), default=None)
if expect == "watchdog":
    ok = None not in (hang, wd, nxt, start) and wd - hang <= 7.5 and wd - start >= 4.5
    if locked:  # GRUB %x: SMI_EN bit 13 and TCO1_CNT bit 12 = 4th hex digit from the right
        import re
        # Only the boot the WATCHDOG fired in: QEMU keeps TCO_LOCK across the watchdog reset while SMI_EN resets
        # to 0x21, so on later boots the lock pins TCO_EN=0 and the emulated BIOS cannot set it again.
        after = [re.search(r"TCO1_CNT=([0-9a-f]+) .*SMI_EN=([0-9a-f]+)", m) for t, m in ev
                 if "wdt: after:" in m and hang is not None and t < hang
                 and t > max((u for u, n in ev if "grub: mode" in n and u < hang), default=-1)]
        bit = lambda h, b: (int(h, 16) >> b) & 1
        lock_ok = bool(after) and all(a and bit(a[1], 12) and bit(a[2], 13) for a in after)
        rec("host", f"locked check: {len(after)} 'after:' line(s) in the watchdog's boot, TCO_LOCK and TCO_EN still set in all: {lock_ok}")
        ok = ok and lock_ok
    verdict = (f"boot-start->WATCHDOG {wd - start:.2f}s, HANG->WATCHDOG {wd - hang:.2f}s, next banner {nxt - wd:.2f}s later"
               if ok else f"start={start} hang={hang} wd={wd} next={nxt}")
elif expect == "refuse":
    wds = [t for t, m in ev if m.startswith("qmp: WATCHDOG")]
    refusals = sum("NOT arming" in m for _, m in ev)
    ok = refusals > 0 and banners == 0 and not wds
    verdict = f"{refusals} refusals, {len(rst)} GRUB reboots, 0 kernel boots, 0 WATCHDOG" if ok else f"refusals={refusals} banners={banners} wd={wds}"
else:
    ok = hang is not None and not any(m.startswith("qmp: WATCHDOG") for _, m in ev) and not rst
    verdict = f"no WATCHDOG/RESET for {time.time() - t0 - hang:.1f}s after HANG" if ok else f"hang={hang} wd={wd} resets={rst}"
rec("host", f"VERDICT expect={expect}: {'PASS' if ok else 'FAIL'} ({verdict})")
open(OUT, "w").write("\n".join(log) + "\n")
print(f"wrote {OUT}")
sys.exit(0 if ok else 1)
