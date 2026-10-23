#!/usr/bin/env python3
"""A pixel-exact picture of the NC110 screen from its cell grid: pulls the cells over bmctl (cellsdump) and draws them
with the kernel's own glyphs (src/font.rs) and palette (src/console.rs PAL). Pixel-only layers (the big title, demo
numbers, graphics, QR) are not in the cells and do not show.
  uv run --with pillow python tools/nc110-shot.py out.png [cells.bin]   (NC110_HOST picks the address, as bmctl.py)"""
import os, re, subprocess, sys
from PIL import Image
out = sys.argv[1]
src = sys.argv[2] if len(sys.argv) > 2 else "/tmp/nc110-cells.bin"
if len(sys.argv) <= 2:
    r = subprocess.run(["tools/.venv/bin/python", "tools/bmctl.py", "cellsdump", src], capture_output=True, text=True)
    if r.returncode:
        sys.exit(r.stdout + r.stderr)
raw = open(src, "rb").read()
hdr, cells = raw.split(b"\n", 1)
f = hdr.decode().split()
cols, rows = map(int, f[0].split("x"))
fw, fh = map(int, f[2].split("x"))
fonts = {}
for m in re.finditer(r"pub static F(\d+)X(\d+): Font = Font \{ w: \d+, h: \d+, rows: &\[(.*?)\]", open("src/font.rs").read(), re.S):
    fonts[(int(m.group(1)), int(m.group(2)))] = [int(x, 16) for x in re.findall(r"0x([0-9a-fA-F]+)", m.group(3))]
glyphs = fonts[(fw, fh)]
pal_src = open("src/console.rs").read()
pal = [int(x, 16) for x in re.findall(r"0x([0-9A-F]{6})", pal_src[pal_src.index("const PAL: [u32; 16]"):].split(";", 1)[0] + pal_src[pal_src.index("const PAL: [u32; 16]"):].split("];")[0])][:16]
W, H = 1024, 600
x0, y0 = (W - cols * fw) // 2, (H - rows * fh) // 2
img = Image.new("RGB", (W, H), tuple(pal[0].to_bytes(3, "big")))
px = img.load()
for i in range(cols * rows):
    v = int.from_bytes(cells[8 * i:8 * i + 8], "little")
    ch, attr, fr, br = v & 0xFF, v >> 8 & 0xFF, v >> 16 & 0xFFFFFF, v >> 40 & 0xFFFFFF
    fg = tuple((fr or pal[attr & 15]).to_bytes(3, "big"))
    bg = tuple((br or pal[attr >> 4 & 15]).to_bytes(3, "big"))
    cx, cy = x0 + (i % cols) * fw, y0 + (i // cols) * fh
    for y in range(fh):
        bits = glyphs[ch * fh + y]
        for x in range(fw):
            px[cx + x, cy + y] = fg if bits >> (fw - 1 - x) & 1 else bg
img.save(out)
print(f"{cols}x{rows} cells, font {fw}x{fh} -> {out}")
