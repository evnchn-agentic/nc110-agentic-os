#!/usr/bin/env python3
"""Generate src/font.rs: Spleen 12x24, 8x16 and 6x12 (BSD-2, tools/font/LICENSE.spleen) as 256-glyph CP437 tables for
the framebuffer console. CP437 -> Unicode comes from term.rs's CP437 table (inverted) plus ASCII; glyphs a size lacks
become '?'.   python3 tools/font/gen-font.py > src/font.rs"""
import re, sys
# Canonical CP437 -> Unicode (term.rs maps many code points onto one CP437 cell, so it cannot be inverted), plus
# alternates tried when Spleen lacks the canonical glyph.
LOW = [0, 0x263A, 0x263B, 0x2665, 0x2666, 0x2663, 0x2660, 0x2022, 0x25D8, 0x25CB, 0x25D9, 0x2642, 0x2640, 0x266A, 0x266B,
       0x263C, 0x25BA, 0x25C4, 0x2195, 0x203C, 0x00B6, 0x00A7, 0x25AC, 0x21A8, 0x2191, 0x2193, 0x2192, 0x2190, 0x221F,
       0x2194, 0x25B2, 0x25BC]
cp2u = {c: LOW[c] for c in range(1, 32)}
for c in range(0x20, 0x7F):
    cp2u[c] = c
cp2u[0x7F] = 0x2302
for c in range(0x80, 0x100):
    cp2u[c] = ord(bytes([c]).decode("cp437"))
ALT = {0x10: [0x25B6, 0x25B8], 0x11: [0x25C0, 0x25C2], 0x1E: [0x25B4], 0x1F: [0x25BE], 0x07: [0x2219, 0x00B7], 0xF9: [0x00B7, 0x2022],
       0xFE: [0x25AA, 0x25A0], 0xFB: [0x2713]}


def synth(c, w, h):
    """Glyphs Spleen lacks at this size, drawn from strokes: box drawing, shades, arrows, triangles, dots, tick."""
    g = [[0] * w for _ in range(h)]
    cx, cy = (w - 1) // 2, (h - 1) // 2
    t = 2 if w >= 12 else 1  # stroke width

    def put(x, y):
        if 0 <= x < w and 0 <= y < h:
            g[y][x] = 1

    def rows():
        return [sum(b << (w - 1 - x) for x, b in enumerate(r)) for r in g]
    BOX = {0xB3: (1, 1, 0, 0), 0xB4: (1, 1, 1, 0), 0xB5: (1, 1, 2, 0), 0xB6: (2, 2, 1, 0), 0xB7: (0, 2, 1, 0), 0xB8: (0, 1, 2, 0),
           0xB9: (2, 2, 2, 0), 0xBA: (2, 2, 0, 0), 0xBB: (0, 2, 2, 0), 0xBC: (2, 0, 2, 0), 0xBD: (2, 0, 1, 0), 0xBE: (1, 0, 2, 0),
           0xBF: (0, 1, 1, 0), 0xC0: (1, 0, 0, 1), 0xC1: (1, 0, 1, 1), 0xC2: (0, 1, 1, 1), 0xC3: (1, 1, 0, 1), 0xC4: (0, 0, 1, 1),
           0xC5: (1, 1, 1, 1), 0xC6: (1, 1, 0, 2), 0xC7: (2, 2, 0, 1), 0xC8: (2, 0, 0, 2), 0xC9: (0, 2, 0, 2), 0xCA: (2, 0, 2, 2),
           0xCB: (0, 2, 2, 2), 0xCC: (2, 2, 0, 2), 0xCD: (0, 0, 2, 2), 0xCE: (2, 2, 2, 2), 0xCF: (1, 0, 2, 2), 0xD0: (2, 0, 1, 1),
           0xD1: (0, 1, 2, 2), 0xD2: (0, 2, 1, 1), 0xD3: (2, 0, 0, 1), 0xD4: (1, 0, 0, 2), 0xD5: (0, 1, 0, 2), 0xD6: (0, 2, 0, 1),
           0xD7: (2, 2, 1, 1), 0xD8: (1, 1, 2, 2), 0xD9: (1, 0, 1, 0), 0xDA: (0, 1, 0, 1)}
    if c in BOX:
        u, d, l, r = BOX[c]
        gap = max(1, t)  # half the distance between the two strokes of a double line
        for k in range(t):
            for (lo, hi, weight) in ((0, cy, u), (cy, h - 1, d)):
                for off in ((0,) if weight == 1 else (-gap - k, gap + k) if weight == 2 else ()):
                    for y in range(lo, hi + 1):
                        put(cx + off + (k if weight == 1 else 0), y)
            for (lo, hi, weight) in ((0, cx, l), (cx, w - 1, r)):
                for off in ((0,) if weight == 1 else (-gap - k, gap + k) if weight == 2 else ()):
                    for x in range(lo, hi + 1):
                        put(x, cy + off + (k if weight == 1 else 0))
        return rows()
    if c in (0xB0, 0xB1, 0xB2):
        for y in range(h):
            for x in range(w):
                put(x, y) if {0xB0: (x + 2 * y) % 4 == 0, 0xB1: (x + y) % 2 == 0, 0xB2: (x + 2 * y) % 4 != 0}[c] else None
        return rows()
    if c in (0x10, 0x11, 0x1E, 0x1F):  # solid triangles
        n = min(w, h) - 2
        for i in range(n):
            half = (i if c in (0x1E, 0x1F) else i) * (w // 2 if c in (0x10, 0x11) else w // 2) // max(n - 1, 1)
            for j in range(-half, half + 1):
                if c == 0x10:
                    put(1 + i * (w - 2) // n, cy + j * (h // 3) // max(w // 2, 1))
                elif c == 0x11:
                    put(w - 2 - i * (w - 2) // n, cy + j * (h // 3) // max(w // 2, 1))
                elif c == 0x1E:
                    put(cx + j, cy - n // 2 + i)
                else:
                    put(cx + j, cy + n // 2 - i)
        return rows()
    if c in (0x18, 0x19, 0x1A, 0x1B, 0x12, 0x1D, 0x17):  # arrows
        vert = c in (0x18, 0x19, 0x12, 0x17)
        if vert:
            for k in range(t):
                for y in range(2, h - 2):
                    put(cx + k, y)
        else:
            for k in range(t):
                for x in range(1, w - 1):
                    put(x, cy + k)
        heads = {0x18: ["up"], 0x19: ["down"], 0x1A: ["right"], 0x1B: ["left"], 0x12: ["up", "down"], 0x1D: ["left", "right"], 0x17: ["up", "down"]}[c]
        a = max(2, w // 3)
        for hd in heads:
            for i in range(a + 1):
                if hd == "up":
                    put(cx - i, 2 + i); put(cx + i + t - 1, 2 + i)
                elif hd == "down":
                    put(cx - i, h - 3 - i); put(cx + i + t - 1, h - 3 - i)
                elif hd == "right":
                    put(w - 2 - i, cy - i); put(w - 2 - i, cy + i + t - 1)
                else:
                    put(1 + i, cy - i); put(1 + i, cy + i + t - 1)
        if c == 0x17:
            for x in range(1, w - 1):
                put(x, h - 1)
        return rows()
    if c in (0x07, 0xF9, 0xFA, 0xFE, 0x09, 0x16):  # dots and squares
        r = {0x07: max(1, w // 4), 0xF9: max(1, w // 6), 0xFA: 0, 0xFE: max(1, w // 4), 0x09: max(1, w // 3), 0x16: 0}[c]
        if c == 0x16:
            for y in range(h * 2 // 3, h * 2 // 3 + max(2, h // 8)):
                for x in range(1, w - 1):
                    put(x, y)
            return rows()
        for y in range(-r, r + 1):
            for x in range(-r, r + 1):
                inside = x * x + y * y <= r * r + r if c != 0xFE else True
                ring = c == 0x09 and x * x + y * y < (r - 1) * (r - 1)
                if inside and not ring:
                    put(cx + x, cy + y)
        return rows()
    if c == 0xFB:  # tick
        for i in range(w // 3 + 1):
            for k in range(t):
                put(1 + i, cy + i + k)
        for i in range(w * 2 // 3):
            for k in range(t):
                put(1 + w // 3 + i, cy + w // 3 - i * (h // 2) // max(w * 2 // 3, 1) + k)
        return rows()
    if c == 0x1C:  # right angle
        for k in range(t):
            for y in range(cy, h - 2):
                put(1 + k, y)
            for x in range(1, w - 1):
                put(x, h - 3 + k)
        return rows()
    return None
out = ["//! Generated by tools/font/gen-font.py: Spleen (c) 2018-2024 Frederic Cambus, BSD 2-Clause (tools/font/LICENSE.spleen),",
       "//! as CP437. Each glyph row holds the pixels in its low `w` bits, leftmost = bit w-1.",
       "pub struct Font {", "    pub w: usize,", "    pub h: usize,", "    pub rows: &'static [u16],", "}",
       "impl Font {", "    pub fn glyph(&self, c: u8) -> &[u16] {", "        &self.rows[c as usize * self.h..(c as usize + 1) * self.h]", "    }", "}"]
for w, h in ((12, 24), (8, 16), (6, 12)):
    bdf = open(f"tools/font/spleen-{w}x{h}.bdf").read()
    fbb = re.search(r"FONTBOUNDINGBOX (\d+) (\d+) (-?\d+) (-?\d+)", bdf).groups()
    glyphs = {}
    for m in re.finditer(r"ENCODING (\d+)\n.*?BBX (-?\d+) (-?\d+) (-?\d+) (-?\d+)\nBITMAP\n(.*?)ENDCHAR", bdf, re.S):
        enc, bw, bh, bx, by = map(int, m.groups()[:5])
        assert (bw, bh, bx, by) == tuple(map(int, fbb)), (w, enc)
        rows = [int(r, 16) for r in m.group(6).split()]
        pad = len(m.group(6).split()[0]) * 4 - w  # BDF pads each row to whole bytes
        glyphs[enc] = [r >> pad for r in rows]
    full = (1 << w) - 1
    blocks = {0xDB: [full] * h, 0xDC: [0] * (h // 2) + [full] * (h - h // 2), 0xDF: [full] * (h // 2) + [0] * (h - h // 2),
              0xDD: [full ^ ((1 << (w - w // 2)) - 1)] * h, 0xDE: [(1 << (w - w // 2)) - 1] * h}  # exact blocks (the logo uses 0xDB)
    flat, miss = [], 0
    for c in range(256):
        g = blocks.get(c) or glyphs.get(cp2u.get(c, -1))
        for alt in ALT.get(c, []):
            g = g or glyphs.get(alt)
        if g is None and c:
            g = synth(c, w, h)
        if g is None:
            miss += c != 0
            g = glyphs[ord("?")] if c else [0] * h
        flat += g
    out.append(f"pub static F{w}X{h}: Font = Font {{ w: {w}, h: {h}, rows: &[")
    for i in range(0, len(flat), 24):
        out.append("    " + ", ".join(f"{r:#x}" for r in flat[i:i + 24]) + ",")
    out.append("] };")
    print(f"// {w}x{h}: {miss} CP437 codes fell back to '?'", file=sys.stderr)
print("\n".join(out))
