#!/usr/bin/env python3
"""Min-entropy assessment of the NC110 jitter source (8-bit raw samples = low byte of each walk count, src/rng.rs).

  tools/.venv/bin/python tools/bmctl.py rng 1000000 /tmp/raw.bin   # capture from a running kernel (real board / q35)
  python3 tools/entropy-est.py /tmp/raw.bin                         # quick floor + NIST SP 800-90B non-IID
  python3 tools/entropy-est.py proof/m25-rngdump-serial.log         # old `rng=dump` serial format also accepted
  python3 tools/entropy-est.py --quick FILE                         # skip NIST (seconds instead of minutes)
  python3 tools/entropy-est.py --cutoffs 0.25                       # SP 800-90B RCT/APT cutoffs for a claimed H
  python3 tools/entropy-est.py --selftest                           # mutation check: constant/counter score ~0, urandom ~8

Two layers, report the MIN:
  quick   MCV (SP 800-90B 6.3.1, 99% upper bound) and lag-1: max over contexts a of the 99% upper bound of
          max_b P(b | a), a serial-correlation check MCV misses. Same estimators the kernel gates on at startup.
  NIST    the reference implementation, usnistgov/SP800-90B_EntropyAssessment @ 87c104d, `ea_non_iid -a`, run in
          Docker (tools/ea/Dockerfile, image nc110-ea:87c104d). Its final line, min(H_original, 8 x H_bitstring),
          is the SP 800-90B non-IID estimate. NIST wants >= 1,000,000 samples; fewer runs but is flagged.
Numbers from QEMU measure the HOST (TCG's TSC moves in 1 us host steps), not the N570: q35 only tests the plumbing.
"""
import math, os, random, re, subprocess, sys, tempfile
from collections import Counter, defaultdict

IMAGE = "nc110-ea:87c104d"


def ub(p, n):
    return min(1.0, p + 2.576 * math.sqrt(p * (1 - p) / max(n - 1, 1)))


def cutoffs(h, alpha_exp=30, w=512):
    rct = 1 + math.ceil(alpha_exp / h)
    p = 2 ** -h
    pm = [math.comb(w - 1, k) * p ** k * (1 - p) ** (w - 1 - k) for k in range(w)]
    for c in range(1, w + 1):
        if sum(pm[c - 1:]) <= 2 ** -alpha_exp:
            return rct, c
    return rct, w


def load(path):
    data = open(path, "rb").read()
    if b"rawjit " in data[:1 << 16]:
        s = []
        for line in data.decode(errors="replace").splitlines():
            m = re.match(r"rawjit \d+:((?: [0-9a-f]{2})+)", line.strip())
            if m:
                s += [int(x, 16) for x in m.group(1).split()]
        return bytes(s)
    return data


def quick(s):
    n = len(s)
    c = Counter(s)
    v, k = c.most_common(1)[0]
    h_mcv = max(0.0, -math.log2(ub(k / n, n)))
    trans = defaultdict(Counter)
    for a, b in zip(s, s[1:]):
        trans[a][b] += 1
    worst = 0.0
    for a, cb in trans.items():
        na = sum(cb.values())
        if na >= 20:
            worst = max(worst, ub(cb.most_common(1)[0][1] / na, na))
    h_lag = max(0.0, -math.log2(worst)) if worst else float("nan")
    return {"n": n, "distinct": len(c), "top": (v, k), "mcv": h_mcv, "lag1": h_lag}


def parse_nist(out, rc, distinct):
    """Fail closed. Single-symbol alphabet -> NIST's own 'no entropy awarded' = 0. <= 2 symbols are assessed as
    binary, so H_original alone is the per-sample figure. Otherwise the run must have exited 0 AND printed the
    combined min(H_original, 8 x H_bitstring) line; anything else is an error, never a fallback."""
    per = {f"{k} [{b}-bit]": float(v) for k, v, b in re.findall(r"^\t(.+?Estimate(?: \(bit string\))?) = (-?[0-9.]+) / (\d) bit", out, re.M)}
    for k in ("H_original", "H_bitstring"):
        mm = re.search(rf"^{k}: (-?[0-9.]+)", out, re.M)
        if mm:
            per[k] = float(mm.group(1))
    if distinct == 1 and "No entropy awarded" in out:
        return 0.0, {"NIST: one-symbol alphabet, no entropy awarded": 0.0}
    if rc != 0:
        raise ValueError(f"ea_non_iid exited {rc}")
    m = re.search(r"min\(H_original, 8 X H_bitstring\):\s*(-?[0-9.]+)", out)
    if not m and distinct <= 2:
        m = re.search(r"^H_original: (-?[0-9.]+)", out, re.M)
    if not m:
        raise ValueError("ea_non_iid output lacks its final estimate")
    return max(0.0, float(m.group(1))), per


def nist(s):
    with tempfile.TemporaryDirectory() as d:
        open(os.path.join(d, "raw.bin"), "wb").write(s)
        r = subprocess.run(["docker", "run", "--rm", "-v", f"{d}:/d", "-w", "/d", IMAGE, "ea_non_iid", "-a", "-v",
                            "raw.bin", "8"], capture_output=True, text=True)
    out = r.stdout + r.stderr
    try:
        h, per = parse_nist(out, r.returncode, len(set(s)))
    except ValueError as e:
        sys.exit(f"{e}:\n{out[-1500:]}")
    return h, per, out


def report(path, s, use_nist=True):
    """Prints the assessment; returns (combined, quick, nist) min-entropies per 8-bit sample (nist None if skipped)."""
    q = quick(s)
    v, k = q["top"]
    print(f"{path}: {q['n']} samples, {q['distinct']} distinct, most common {v:#04x} x{k}")
    print(f"  quick  MCV {q['mcv']:.3f}  lag-1 {q['lag1']:.3f} bits/sample")
    hq = min(q["mcv"], q["lag1"]) if not math.isnan(q["lag1"]) else q["mcv"]
    hn = None
    if use_nist:
        if q["n"] < 1_000_000:
            print(f"  NOTE   {q['n']} < 1,000,000 samples: NIST runs, but SP 800-90B wants at least 1e6")
        hn, per, _ = nist(s)
        for name, val in per.items():
            print(f"    {name.strip():<56} {val:.4f}")
        print(f"  NIST   SP 800-90B non-IID min(H_original, 8 x H_bitstring) = {hn:.4f} bits/sample")
    h = hq if hn is None else min(hq, hn)
    print(f"  RESULT min-entropy {h:.4f} bits per 8-bit sample (kernel claims 0.100)")
    return h, hq, hn


def selftest():
    """Mutation check, each layer asserted ON ITS OWN (Codex: a combined minimum let a broken NIST layer hide
    behind the quick estimators). Noise-free or predictable sources must score below the kernel's 0.1 claim."""
    bad = 0
    # parser fixtures (canned ea_non_iid output): fail closed on anything incomplete
    fx = [
        ("one symbol", "Symbol alphabet consists of 1 symbol. No entropy awarded...", 255, 1, 0.0),
        ("binary alphabet", "H_original: 0.25\n", 0, 2, 0.25),
        ("multi-valued", "H_original: 7.7\nH_bitstring: 0.88\nmin(H_original, 8 X H_bitstring): 7.04\n", 0, 256, 7.04),
        ("negative zero", "H_original: -0.000000\nmin(H_original, 8 X H_bitstring): -0.000000\n", 0, 256, 0.0),
        ("multi-valued, no final line", "H_original: 7.7\n", 0, 256, ValueError),
        ("nonzero exit", "H_original: 7.7\nmin(H_original, 8 X H_bitstring): 7.0\n", 1, 256, ValueError),
    ]
    for name, out, rc, dist, want in fx:
        try:
            got = parse_nist(out, rc, dist)[0]
        except ValueError:
            got = ValueError
        ok = got == want
        print(f"  {'ok  ' if ok else 'FAIL'} parser fixture: {name} -> {got}")
        bad |= not ok
    n = 200_000
    rnd = random.Random(1)
    fib = [rnd.randrange(256), rnd.randrange(256)]
    while len(fib) < n:
        fib.append((fib[-1] + fib[-2]) % 256)
    cyc4 = bytes((0x10, 0x20, 0x30, 0x40)[i % 4] if rnd.random() > 0.01 else rnd.randrange(256) for i in range(n))
    LOW = lambda h: h is not None and h < 0.1
    cases = [  # (name, data, expectation on quick, expectation on NIST)
        ("constant timer (raw_delta returns the same count)", bytes([0x5a]) * n, LOW, LOW),
        ("TSC frozen (raw_delta returns 0)", bytes(n), LOW, LOW),
        ("deterministic counter (i mod 256)", bytes(i % 256 for i in range(n)), LOW, LOW),
        ("two-value alternation", bytes((0x40, 0x41) * (n // 2)), LOW, LOW),
        ("count stepping every 1000 samples", bytes((i // 1000) % 256 for i in range(n)), LOW, LOW),
        ("4-value cycle, 1% random replacement", cyc4, lambda h: h < 0.5, LOW),
        # x[i] = x[i-1] + x[i-2] mod 256 (Pisano period 384, so lag-1 sees it too): a deterministic sequence with a
        # spread histogram (MCV ~3.6). Every case asserts the NIST layer on its own, so a NIST layer that inflated
        # its estimate (Codex replaced it with a constant 8) fails here and in every other negative case.
        ("Fibonacci mod 256 (deterministic, spread histogram)", bytes(fib), LOW, LOW),
        # Full entropy is 8, but the estimators are conservative: NIST's bitstring estimates give ~6.6-7.2 on 1e6
        # urandom bytes, the quick lag-1 (max over 256 contexts x 256 values) ~6.3. A good source must land far
        # above the 0.1 claim; it is not expected to score 8.
        ("os.urandom (1e6 bytes)", os.urandom(1_000_000), lambda h: h > 6.0, lambda h: h is not None and h > 6.0),
    ]
    for name, data, want_q, want_n in cases:
        print(f"== {name}")
        _, hq, hn = report(name, data)
        oq, on = want_q(hq), want_n(hn)
        print(f"  {'ok  ' if oq else 'FAIL'} quick layer   {'ok  ' if on else 'FAIL'} NIST layer\n")
        bad |= not (oq and on)
    print("SELFTEST:", "PASS" if not bad else "FAIL")
    sys.exit(bad)


def main():
    a = sys.argv[1:]
    if not a:
        sys.exit(__doc__)
    if a[0] == "--cutoffs":
        h = float(a[1])
        rct, apt = cutoffs(h)
        print(f"H={h}: RCT cutoff {rct} (1+ceil(30/H)), APT W=512 cutoff {apt} (P(1+Bin(511,2^-H)>=C)<=2^-30)")
        return
    if a[0] == "--selftest":
        selftest()
    use_nist = a[0] != "--quick"
    path = a[-1]
    report(path, load(path), use_nist)


main()
