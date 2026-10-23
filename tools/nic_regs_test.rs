//! Host-side check of src/nic_regs.rs against the pinned Linux v6.12 sources in third_party/linux-v6.12/.
//! Run: sh tools/test-nic-regs.sh  (verifies SHA256SUMS first, then builds this with `rustc --test`).
//! Every cited (file, line) must contain the identifier, and the value parsed from that line must equal ours.
#[path = "../src/nic_regs.rs"]
mod nic_regs;

use std::fs;

fn src(file: &str) -> Vec<String> {
    let p = format!("{}/../third_party/linux-v6.12/{}", env!("NIC_TEST_DIR"), file);
    fs::read_to_string(&p).unwrap_or_else(|e| panic!("{p}: {e}")).lines().map(String::from).collect()
}

/// `#define NAME <number>` lookup for symbolic shifts like `(7 << RXCFG_FIFO_SHIFT)`.
fn define(lines: &[String], name: &str) -> u64 {
    for l in lines {
        let t: Vec<&str> = l.split_whitespace().collect();
        if t.len() >= 3 && t[0] == "#define" && t[1] == name {
            return num(t[2]).unwrap_or_else(|| panic!("#define {name} is not a plain number"));
        }
    }
    panic!("no #define {name}");
}

fn num(s: &str) -> Option<u64> {
    let s = s.trim_end_matches(|c| c == 'U' || c == 'L' || c == 'u' || c == 'l');
    if let Some(h) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        u64::from_str_radix(h, 16).ok()
    } else {
        s.parse().ok()
    }
}

struct P<'a> {
    s: &'a [u8],
    i: usize,
    lines: &'a [String],
}

impl<'a> P<'a> {
    fn ws(&mut self) {
        while self.i < self.s.len() && self.s[self.i].is_ascii_whitespace() {
            self.i += 1;
        }
    }
    fn word(&mut self) -> String {
        self.ws();
        let st = self.i;
        while self.i < self.s.len() && (self.s[self.i].is_ascii_alphanumeric() || self.s[self.i] == b'_') {
            self.i += 1;
        }
        String::from_utf8(self.s[st..self.i].to_vec()).unwrap()
    }
    fn eat(&mut self, c: u8) -> bool {
        self.ws();
        if self.i < self.s.len() && self.s[self.i] == c {
            self.i += 1;
            true
        } else {
            false
        }
    }
    fn primary(&mut self) -> u64 {
        if self.eat(b'(') {
            let v = self.expr();
            assert!(self.eat(b')'), "missing )");
            return v;
        }
        let w = self.word();
        if w == "GENMASK" {
            assert!(self.eat(b'('));
            let h = self.expr();
            assert!(self.eat(b','));
            let l = self.expr();
            assert!(self.eat(b')'));
            return ((1u64 << (h + 1)) - 1) & !((1u64 << l) - 1);
        }
        num(&w).unwrap_or_else(|| define(self.lines, &w))
    }
    fn expr(&mut self) -> u64 {
        let a = self.primary();
        self.ws();
        if self.s[self.i..].starts_with(b"<<") {
            self.i += 2;
            a << self.primary()
        } else if self.s[self.i..].starts_with(b">>") {
            self.i += 2;
            a >> self.primary()
        } else {
            a
        }
    }
}

/// Value on a cited line: the expression after `ident ... =`, or after the last `&` when the line masks.
fn value_on_line(lines: &[String], line: usize, ident: &str) -> u64 {
    let l = &lines[line - 1];
    let l = &l[..l.find("/*").or_else(|| l.find("//")).unwrap_or(l.len())]; // drop comments
    let at = l.find(ident).unwrap_or_else(|| panic!("line {line} does not mention {ident}: {l:?}"));
    let rest = &l[at + ident.len()..];
    let rest = if let Some(a) = rest.rfind('&') {
        &rest[a + 1..]
    } else if let Some(e) = rest.find('=') {
        &rest[e + 1..]
    } else if l.trim_start().starts_with("#define") {
        rest
    } else {
        panic!("line {line}: no '=' or '&' after {ident}: {l:?}")
    };
    let mut p = P { s: rest.as_bytes(), i: 0, lines };
    p.expr()
}

fn check(cites: &[(&str, u32, &str, usize, &str)]) {
    let mut bad = 0;
    for &(field, ours, file, line, ident) in cites {
        let lines = src(file);
        let theirs = value_on_line(&lines, line, ident);
        let ok = theirs == ours as u64;
        println!("{} {:<20} ours={:#010x} linux={:#010x}  {}:{} {}", if ok { "ok  " } else { "FAIL" }, field, ours, theirs, file, line, ident);
        if !ok {
            bad += 1;
        }
    }
    assert_eq!(bad, 0, "{bad} register values differ from the cited Linux line");
}

#[test]
fn rtl8101e_matches_r8169_v6_12() {
    check(nic_regs::CITES_8101E);
}

#[test]
fn rtl8139cp_matches_8139cp_v6_12() {
    check(nic_regs::CITES_8139CP);
}

#[test]
fn the_deltas_m0_listed_are_real() {
    use nic_regs::{RTL8101E as E, RTL8139CP as C};
    assert_eq!((C.tx_poll, E.tx_poll), (0xD9, 0x38));
    assert_eq!((C.rx_err, E.rx_err), (1 << 20, 1 << 21));
    assert_eq!((C.rx_len_mask, E.rx_len_mask), (0x1FFF, 0x3FFF));
    // shared descriptor format and ring registers (why rtl8139 C+ is a fair QEMU stand-in)
    assert_eq!((C.tx_ring, C.rx_ring, C.cplus_cmd, C.desc_own), (E.tx_ring, E.rx_ring, E.cplus_cmd, E.desc_own));
}

#[test]
fn a_wrong_value_is_caught() {
    // negative control for the checker itself: TxPoll 0xD9 must NOT pass against r8169 line 185
    let lines = src("r8169_main.c");
    assert_ne!(value_on_line(&lines, 185, "TxPoll"), 0xD9);
    assert_eq!(value_on_line(&lines, 185, "TxPoll"), 0x38);
}

#[test]
fn storage_guard() {
    use nic_regs::clear_of_storage as c;
    assert!(c(0xF000_0000, &[]));
    assert!(c(0xF000_0000, &[0xF040_4000])); // AHCI ABAR well above
    assert!(!c(0xF040_4000, &[0xF040_4000])); // same base
    assert!(!c(0xF040_4100, &[0xF040_4000])); // inside a 16 KiB-aligned ABAR's possible span
    assert!(c(0xF040_8000, &[0xF040_4000])); // just past the 16 KiB bound
    assert!(!c(0xF03F_FF80, &[0xF040_0000])); // NIC window would run into the BAR
    assert!(c(0xF03F_FF00, &[0xF040_0000])); // ends exactly at the BAR
    assert!(!c(0xFEBD_1000, &[0xFEBD_0000, 0x1000_0000])); // any one storage BAR is enough to refuse
}
