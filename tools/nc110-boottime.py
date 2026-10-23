#!/usr/bin/env python3
"""Stopwatch for one NC110 boot: `bmctl reboot`, then time the new boot's UDP broadcasts on 5514.
  tools/.venv/bin/python tools/nc110-boottime.py [runs] [label]   -> one line per run, appended to proof/boottime.log
t=0 is the reboot ack (the kernel resets 500 ms later). Marks: first datagram of the new boot (NIC up, udplog),
first 'hello bmctl' with a new boot id (bmctl ready = provisioning channel back), plus a bmctl ping round trip."""
import re, socket, subprocess, sys, time

PY, BM = "tools/.venv/bin/python", "tools/bmctl.py"
runs = int(sys.argv[1]) if len(sys.argv) > 1 else 1
label = sys.argv[2] if len(sys.argv) > 2 else ""


def bm(*a, timeout=20):
    return subprocess.run([PY, BM, *a], capture_output=True, text=True, timeout=timeout).stdout.strip()


def one():
    s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEPORT, 1)
    s.bind(("", 5514))
    old = re.search(r"boot (\w{32})", bm("stats") or "") and None
    p = bm("ping")
    if not p.startswith("pong"):
        sys.exit(f"board not answering before reboot: {p!r}")
    r = bm("reboot")
    t0 = time.monotonic()
    if "rebooting" not in r:
        sys.exit(f"reboot not acked: {r!r}")
    s.settimeout(1.0)
    first = ready = None
    olds = set()
    while time.monotonic() - t0 < 180:
        try:
            d, _ = s.recvfrom(4096)
        except socket.timeout:
            continue
        t = time.monotonic() - t0
        if t < 0.6:  # still the old kernel (it resets 500 ms after the ack)
            m = re.search(rb"hello bmctl \S+ boot (\w+)", d)
            if m:
                olds.add(m.group(1))
            continue
        m = re.search(rb"hello bmctl \S+ boot (\w+)", d)
        if m and m.group(1) in olds:
            continue
        first = first if first is not None else t
        if m:
            ready = t
            break
    p = bm("ping")
    tp = time.monotonic() - t0
    return first, ready, tp, p


for i in range(runs):
    f, r, tp, p = one()
    line = f"{time.strftime('%Y-%m-%d %H:%M:%S')} {label} first_udp={f and round(f, 1)}s bmctl_ready={r and round(r, 1)}s ping_after={round(tp, 1)}s {p}"
    print(line, flush=True)
    open("proof/boottime.log", "a").write(line + "\n")
    time.sleep(5)
