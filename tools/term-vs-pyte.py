#!/usr/bin/env python3
"""Compare src/term.rs with pyte (an independent VT/xterm emulator) on the shell output captured in an M4/M4.5 serial log.
The kernel mirrors the channel to COM1 with ESC shown as '^['; we undo that, replay the stream up to each cut point
through term.rs (tools/term_replay.rs) and through pyte at 80x25, and diff characters, colours and reverse video.
Usage: <venv with pyte>/bin/python tools/term-vs-pyte.py proof/m45-serial.log
M4.5: pyte's Unicode cells are mapped to CP437 with Python's cp437 codec (plus the glyphs CP437 has in 0x01-0x1F), a wide
character's two cells to '??', and bright backgrounds (SGR 100-107) to VGA 8-15."""
import os, re, subprocess, sys, tempfile
import pyte
from wcwidth import wcwidth


class Screen(pyte.Screen):  # pyte 0.8 crashes on CSI > 4;2 m (vim's modifyOtherKeys); xterm ignores private SGR
    def select_graphic_rendition(self, *a, private=False, **kw):
        if not private:
            super().select_graphic_rendition(*a, **kw)

    # pyte 0.8.2 has no SU/SD (CSI S / CSI T); htop under a UTF-8 locale uses CSI 2 T inside a scroll region.
    # xterm semantics: scroll the region by n, cursor unchanged. Built from pyte's own IND/RI at the margins.
    def _scroll(self, n, up, private=False):
        if private:
            return
        top, bot = self.margins or (0, self.lines - 1)
        y = self.cursor.y
        self.cursor.y = bot if up else top
        for _ in range(max(n or 1, 1)):
            self.index() if up else self.reverse_index()
        self.cursor.y = y

    def scroll_up(self, n=None, *rest, private=False):
        self._scroll(n, True, private)

    def scroll_down(self, n=None, *rest, private=False):
        if not rest:  # CSI Ps ; ... T with several parameters is mouse tracking, not SD
            self._scroll(n, False, private)


class Stream(pyte.ByteStream):
    csi = dict(pyte.ByteStream.csi, S="scroll_up", T="scroll_down")


def for_pyte(b):  # pyte has no REP (CSI b); ncurses' rep is always <char> ESC [ n-1 b, so expand it textually
    return re.sub(rb"([\x20-\x7e])\x1b\[(\d+)b", lambda m: m.group(1) * (int(m.group(2)) + 1), b)

D = os.path.dirname(os.path.abspath(__file__))
log = open(sys.argv[1], "rb").read()
start = log.index(b"^[[?1034h")  # first shell output
stream = log[start:].replace(b"^[", b"\x1b")
end = stream.find(b"\r\nssh: ")  # kernel log resumes after the channel closes
stream = stream[: end if end > 0 else len(stream)]
exe = os.path.join(D, "../target/host/term_replay")
subprocess.run(["rustc", "--edition", "2021", "-O", "--cfg", "test", os.path.join(D, "term_replay.rs"), "-o", exe], check=True)
tmp = tempfile.NamedTemporaryFile(delete=False)
tmp.write(stream)
tmp.close()

ANSI = {"black": 0, "red": 1, "green": 2, "brown": 3, "blue": 4, "magenta": 5, "cyan": 6, "white": 7, "default": None}
VGA = [0, 4, 2, 6, 1, 5, 3, 7]
LOW = {c: i + 1 for i, c in enumerate("☺☻♥♦♣♠•◘○◙♂♀♪♫☼►◄↕‼¶§▬↨↑↓→←∟↔▲▼")}  # CP437 glyphs the codec calls controls
LOW["⌂"] = 0x7F


def colour(name, default):  # "red" -> VGA 4, "brightred" (pyte also spells one "bfightmagenta") -> VGA 12
    bright = name.startswith("b") and name[2:].startswith("ight") and name not in ANSI
    v = ANSI.get(name[6:] if bright else name, None)
    return default if v is None else VGA[v] | (8 if bright else 0)


def code(c):
    if c == "":
        return ord("?")  # right half of a wide character
    if wcwidth(c) == 2:
        return ord("?")
    if c in LOW:
        return LOW[c]
    try:
        return c.encode("cp437")[0]
    except UnicodeEncodeError:
        return ord("?")


def pyte_cell(ch):
    fg, bg = colour(ch.fg, 7), colour(ch.bg, 0)
    if ch.bold:
        fg |= 8
    if ch.reverse:
        fg, bg = bg, fg
    return (bg << 12) | (fg << 8) | code(ch.data)

def compare(name, cut):
    scr = Screen(80, 25)
    st = Stream(scr)
    st.feed(for_pyte(stream[:cut]))
    out = subprocess.run([exe, tmp.name, str(cut)], capture_output=True, text=True, check=True).stdout.splitlines()
    ours = [[int(x, 16) for x in l.split()] for l in out[:25]]
    diffs = [(r, c, ours[r][c], pyte_cell(scr.buffer[r][c])) for r in range(25) for c in range(80) if ours[r][c] != pyte_cell(scr.buffer[r][c])]
    chars = sum(1 for d in diffs if d[2] & 0xFF != d[3] & 0xFF)
    print(f"{name}: byte {cut}: {len(diffs)} cells differ ({chars} in the character), cursor ours {out[25][7:]} pyte {scr.cursor.y} {scr.cursor.x}")
    for r, c, a, b in diffs[:8]:
        print(f"   r{r} c{c}: ours {a:04x} pyte {b:04x}")
    return len(diffs)

cuts = []
h = stream.find(b"\x1b[?1049h")  # main screen before the first full-screen app; M4.5: before the reply probe moves
p = stream.find(b"\x1b[3;7H", 0, h)  # the cursor and `clear` wipes the printf lines (UTF-8, wide, bright backgrounds)
cuts.append(("shell (printf lines)", p if p > 0 else h))
i = stream.find(b"\x1b[?1049l")  # htop's rmcup: compare the last htop frame
cuts.append(("htop (last frame)", i))
j = stream.find(b"\x1b[?1049l", i + 1)
k = stream.rfind(b"\x1b[25;1H", 0, j)  # vi: just before the ':' command line is drawn
e = stream.find(b"\x1b[25;1H:", i, j)  # M4.5: before the `:echo`, whose output has a literal "^[" the mirror makes ESC
k = e if e > 0 else k
cuts.append(("vi (before the : command line)", k))
# No cut after a ?1049l: pyte 0.8 has no alternate screen, so it keeps the app residue that xterm (and we) restore.
bad = sum(compare(n, c) for n, c in cuts)
os.unlink(tmp.name)
sys.exit(1 if bad else 0)
