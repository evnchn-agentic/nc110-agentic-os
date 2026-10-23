#!/usr/bin/env python3
"""NC110 push-to-talk, Mac side: watch the agent OS for finished mic clips (F12 on the netbook), pull each one over
bmctl (authenticated + encrypted), transcribe it with the DS relay's ASR (~/nds-claude/relay/asr.py, faster-whisper),
and send the text back: `type` puts it in the SSH shell (no Enter) or on the home screen; `say` reports "heard
nothing". Run under the nds-claude venv:
  ~/nds-claude/.venv/bin/python tools/nc110-voice.py [--model base.en]
Clips are written to proof/voice/ only with --keep (they are room audio)."""
import argparse, os, shutil, subprocess, sys, tempfile, time

ap = argparse.ArgumentParser()
ap.add_argument("--model", default="base.en")
ap.add_argument("--keep", action="store_true")
a = ap.parse_args()
sys.path.insert(0, os.path.expanduser("~/nds-claude"))
from relay import asr  # noqa: E402

BM = ["tools/.venv/bin/python", "tools/bmctl.py"]


def bm(*args, timeout=30):
    r = subprocess.run(BM + list(args), capture_output=True, text=True, timeout=timeout)
    return r.returncode, r.stdout.strip()


HOSTS = ["192.168.50.120", "10.6.0.9"]  # the cable at home, then the WireGuard tunnel (venue)


def pick_host():
    """The address the netbook answers on now (it moves between the cable and the tunnel); None if neither."""
    for h in HOSTS:
        os.environ["NC110_HOST"] = h
        try:
            if bm("ping", timeout=8)[0] == 0:
                return h
        except subprocess.TimeoutExpired:
            pass
    return None


def status():
    rc, out = bm("aud")
    f = out.split()
    if rc != 0 or f[:1] != ["clip"]:
        return None
    return int(f[1]), int(f[3]), f[5] == "1", f[-1] == "1" and f[-2] == "overrun"


last = None
print("nc110-voice: watching for clips (F12 on the netbook)", flush=True)
while True:
    try:
        st = status()
    except subprocess.TimeoutExpired:
        st = None
    if st is None:
        h = pick_host()
        print(f"nc110-voice: netbook at {h}", flush=True) if h else None
        time.sleep(2)
        continue
    cid, n, rec, over = st
    if over and last is not None and cid != last[0]:
        last = (cid, 0)
        bm("say", "recording dropped (capture overrun): try again")
    elif last is None or cid < last[0]:  # first look, or the netbook rebooted (ids restart): only new clips count
        last = (cid, n if not rec else -1)
    elif not rec and n > 0 and (cid, n) != last:
        last = (cid, n)
        d = "proof/voice" if a.keep else tempfile.mkdtemp()
        os.makedirs(d, exist_ok=True)
        wav = os.path.join(d, f"clip-{time.strftime('%Y%m%d-%H%M%S')}.wav")
        try:
            rc, out = bm("audget", wav, timeout=120)
            if rc != 0:
                print("audget failed:", out, flush=True)
                continue
            t0 = time.time()
            r = asr.transcribe(wav, model_size=a.model)
            segs = [s for s in r.segments if s.no_speech_prob < 0.6]
            text = " ".join(s.text.strip() for s in segs).strip()
            print(f"clip {cid}: {n / 32000:.1f} s -> {text!r} ({time.time() - t0:.1f} s ASR)", flush=True)
            if text:
                bm("type", text)
            else:
                bm("say", "heard nothing")
        except Exception as e:  # keep the daemon alive; the finally still removes the room audio
            print(f"clip {cid}: failed: {e!r}", flush=True)
        finally:
            if not a.keep:
                shutil.rmtree(d, ignore_errors=True)
    time.sleep(0.7)
