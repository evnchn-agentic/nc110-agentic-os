#!/usr/bin/env python3
"""Mutant check for tools/term_test.rs: stub each src/term.rs feature in a scratch copy, rebuild the host tests,
and require at least one test to go red. A feature whose stub stays green is not actually tested."""
import os, re, shutil, subprocess, sys, tempfile

D = os.path.dirname(os.path.abspath(__file__))
SRC = open(os.path.join(D, "../src/term.rs")).read()
M = [  # (feature, exact text in term.rs, stubbed replacement)
    ("SGR", "b'm' => return self.sgr(),", "b'm' => return,"),
    ("ANSI->VGA colour order", "[0, 4, 2, 6, 1, 5, 3, 7]", "[0, 1, 2, 3, 4, 5, 6, 7]"),
    ("bold = bright", "self.fg | if self.bold { 8 } else { 0 }", "self.fg"),
    ("SGR 38;5;n", "if sub == 5 && i + 2 < np {", "if false {"),
    ("reverse video", "if self.rev {", "if false {"),
    ("bce erase", "self.attr() | b' ' as u16 // bce", "BLANK // bce"),
    ("DECSTBM", "if t < b {", "if false {"),
    ("LF scrolls the region", "if self.row == self.bot {\n            self.scroll(self.top, self.bot, 1, true);", "if self.row == self.h - 1 {\n            self.scroll(0, self.h - 1, 1, true);"),
    ("RI scrolls the region", "self.scroll(self.top, self.bot, 1, false);", "self.scroll(0, self.h - 1, 1, false);"),
    ("IL/DL inside the region", "self.scroll(row, self.bot, n, f == b'M');", "self.scroll(row, self.h - 1, n, f == b'M');"),
    ("CSI S", "b'S' => self.scroll(self.top, self.bot, n, true),", "b'S' => {}"),
    ("CSI T", "b'T' if self.np <= 1 => self.scroll(self.top, self.bot, n, false),", "b'T' if self.np <= 1 => {}"),
    ("CSI @", "self.cells.copy_within(s + c..s + self.w - n, s + c + n);", ""),
    ("CSI P", "self.cells.copy_within(s + c + n..s + self.w, s + c);", ""),
    ("IRM", "(false, 4) => self.insert = on,", "(false, 4) => {}"),
    ("ESC 8", "b'8' => self.restore(),", "b'8' => {}"),
    ("CSI u", "b'u' => self.restore(),", "b'u' => {}"),
    ("?1049", "(true, 1049) =>", "(true, 10490) =>"),
    ("?1049 restores the cursor", "self.set_alt(false, false);\n                        self.restore();", "self.set_alt(false, false);"),
    ("?25", "(true, 25) => self.cursor_on = on,", "(true, 25) => {}"),
    ("ESC ( 0", "self.g[i as usize] = b == b'0';", "self.g[i as usize] = false;"),
    ("SO", "0x0E => self.gl = 1,", "0x0E => {}"),
    ("REP", "b'b' => {", "b'\\xFF' => {"),
    ("DCS/APC swallowed", "b']' | b'P' | b'_' | b'^' | b'X' => self.st = St::Str,", "b']' => self.st = St::Str,"),
    ("DECCKM", "(true, 1) => self.app_cursor = on,", "(true, 1) => {}"),
    ("DECAWM", "self.wrap = self.autowrap;", "self.wrap = true;"),
    ("CSI intermediates", "if self.inter != 0 {", "if false {"),
    ("private CSI ignored", "if self.private {\n            return; // CSI ? s/r", "if false {\n            return; // CSI ? s/r"),
    ("DECSTR", "if self.inter == b'!' && f == b'p' {", "if false {"),
    ("saved cursor per screen", "self.saved[self.alt as usize] = Saved", "self.saved[0] = Saved"),
    ("colon sub-parameters", "b':' => self.inter = b':',", "b':' => self.np = self.np.max(1) + 1,"),
    ("ESC c", "b'c' => self.reset(),", "b'c' => {}"),
    # M4.5
    ("SGR 22", "22 => self.bold = false,", "22 => {}"),
    ("SGR 27", "27 => self.rev = false,", "27 => {}"),
    ("SGR 39", "39 => self.fg = DEF_FG,", "39 => {}"),
    ("SGR 49", "49 => self.bg = 0,", "49 => {}"),
    ("SGR 100-107 bright bg", "100..=107 => self.bg = ANSI_TO_VGA[(v - 100) as usize] | 8,", "100..=107 => self.bg = ANSI_TO_VGA[(v - 100) as usize],"),
    ("48;5;8-15 bright bg", "self.bg = c | if n >= 8 { 8 } else { 0 };", "self.bg = c;"),
    ("reverse keeps bright bg", "(f, b) = (b, f);", "(f, b) = (b, f & 7);"),
    ("DSR 5n", "5 => self.send(1, b\"\\x1b[0n\"),", "5 => {}"),
    ("DSR 6n", "self.send(0, &b[..n]);", ""),
    ("CPR row/col digits", "if v >= 10 {", "if false {"),
    ("DA1", "0 => self.send(2, b\"\\x1b[?6c\"),", "0 => {}"),
    ("DA2", "b'>' => self.send(3, b\"\\x1b[>0;10;0c\"),", "b'>' => {}"),
    ("DA only for Ps 0", "if f == b'c' && self.inter == 0 && self.arg(0, 0) == 0 {", "if f == b'c' && self.inter == 0 {"),
    ("DSR not for CSI ? n", "if f == b'n' && self.inter == 0 && self.lead == 0 {", "if f == b'n' && self.inter == 0 {"),
    ("reply all-or-nothing", "if self.nreply + s.len() > self.reply.len() {", "if self.nreply >= self.reply.len() {"),
    ("take_reply partial", "self.reply.copy_within(n..self.nreply, 0);", ""),
    ("UTF-8 decode", "(self.utf, self.need, self.min) = ((b & mask) as u32, need, min);", "let _ = (need, min, mask);"),
    ("UTF-8 continuation", "self.utf = (self.utf << 6) | (b & 0x3F) as u32;", "self.utf <<= 6;"),
    ("UTF-8 overlong/surrogate", "let ok = cp >= self.min && !(0xD800..=0xDFFF).contains(&cp) && cp <= 0x10FFFF;", "let ok = true;"),
    ("UTF-8 cut short -> ?", "self.need = 0; // sequence cut short (by a control, ESC, or a new lead byte): one '?', then this byte\n            self.print(b'?' as u32);", "self.need = 0;"),
    ("stray byte -> ?", "0x80..=0xFF => self.print(b'?' as u32), // stray", "0x80..=0xFF => {} // stray"),
    ("CP437 lookup", "Ok(i) => CP437[i].1,", "Ok(_) => b'?',"),
    ("wide = 2 cells", "            self.put(b'?');\n            self.put(b'?');", "            self.put(b'?');"),
    ("wide wraps whole", "if self.wrap || self.col >= self.w - 1 {", "if self.wrap {"),
    ("zero width", "        if w == 0 {\n            return;\n        }", ""),
    ("zero-width table", "} else if within(&ZERO_WIDTH) {", "} else if false {"),
    ("wide table", "} else if within(&WIDE) {", "} else if false {"),
    ("wide dropped at margin, ?7l", "                return; // no room and no wrap", "                // no room and no wrap"),
]

tmp = tempfile.mkdtemp(prefix="term-mutants-")
os.makedirs(os.path.join(tmp, "src"))
os.makedirs(os.path.join(tmp, "tools"))
shutil.copy(os.path.join(D, "term_test.rs"), os.path.join(tmp, "tools"))
bad = []
try:
    for name, old, new in M:
        if SRC.count(old) != 1:
            bad.append(name)
            print(f"  MUTANT-SETUP {name}: text found {SRC.count(old)}x (want 1)")
            continue
        open(os.path.join(tmp, "src/term.rs"), "w").write(SRC.replace(old, new))
        exe = os.path.join(tmp, "t")
        c = subprocess.run(["rustc", "--edition", "2021", "--test", os.path.join(tmp, "tools/term_test.rs"), "-o", exe, "-A", "warnings"], capture_output=True, text=True)
        if c.returncode != 0:
            bad.append(name)
            print(f"  MUTANT-BUILD {name}: does not compile\n{c.stderr[-800:]}")
            continue
        r = subprocess.run([exe, "--test-threads", "1"], capture_output=True, text=True)
        failed = re.findall(r"^test (\S+) \.\.\. FAILED", r.stdout, re.M)
        if r.returncode == 0:
            bad.append(name)
            print(f"  SURVIVED {name}: all tests still pass")
        else:
            print(f"  killed   {name:28} by {', '.join(failed)}")
finally:
    shutil.rmtree(tmp)
print(f"mutants: {len(M) - len(bad)}/{len(M)} killed")
sys.exit(1 if bad else 0)
