//! Host-side tests for src/term.rs: feed byte strings, assert the cell grid (char + VGA attribute): 80x25 by default,
//! and the panel grids (128x37, 170x50) through resize().
//! Run: sh tools/test-term.sh  (also runs tools/term-mutants.py: each feature's stub must turn a test red).
#[path = "../src/term.rs"]
#[allow(dead_code)]
mod term;

use term::Term;
const W: usize = 80; // Term::new() is the 80x25 VGA grid; the panel sizes are tested at the end via resize()
const H: usize = 25;

fn run(s: &[u8]) -> Box<Term> {
    let mut t = Box::new(Term::new());
    for &b in s {
        t.feed(b);
    }
    t
}
fn row(t: &Term, r: usize) -> String {
    (0..W).map(|c| (t.cell(r, c) & 0xFF) as u8 as char).collect::<String>().trim_end().to_string()
}
fn attr(t: &Term, r: usize, c: usize) -> u8 {
    (t.cell(r, c) >> 8) as u8
}
/// Rows 0..n labelled "r0".."rN" via CUP, so scroll tests can see which row moved where.
fn labelled(n: usize) -> Vec<u8> {
    let mut v = Vec::new();
    for r in 0..n {
        v.extend(format!("\x1b[{};1Hr{}", r + 1, r).bytes());
    }
    v
}
fn with(a: Vec<u8>, b: &[u8]) -> Vec<u8> {
    let mut a = a;
    a.extend_from_slice(b);
    a
}

#[test]
fn sgr_colours_map_to_vga() {
    let t = run(b"\x1b[31mR\x1b[1;32mG\x1b[0;44mB\x1b[93mY\x1b[39;49mD\x1b[100mE");
    assert_eq!(row(&t, 0), "RGBYDE");
    assert_eq!(attr(&t, 0, 0), 0x04, "31 red -> VGA 4");
    assert_eq!(attr(&t, 0, 1), 0x0A, "1;32 bold green -> bright green");
    assert_eq!(attr(&t, 0, 2), 0x17, "0;44 grey on blue");
    assert_eq!(attr(&t, 0, 3), 0x1E, "93 bright yellow keeps bg");
    assert_eq!(attr(&t, 0, 4), 0x07, "39;49 default");
    assert_eq!(attr(&t, 0, 5), 0x87, "100 bright black (dark grey) bg: attribute bit 7, blink is off");
}

fn reply(t: &mut Term) -> Vec<u8> {
    let mut b = [0u8; 128];
    let n = t.take_reply(&mut b);
    b[..n].to_vec()
}
fn cells(t: &Term, r: usize, n: usize) -> Vec<u8> {
    (0..n).map(|c| (t.cell(r, c) & 0xFF) as u8).collect()
}

#[test]
fn bright_backgrounds_and_resets() {
    let t = run(b"\x1b[102mA\x1b[48;5;12mB\x1b[44;7mC\x1b[0;1;101;7mD");
    assert_eq!(attr(&t, 0, 0), 0xA7, "102 -> bright green bg (VGA 10)");
    assert_eq!(attr(&t, 0, 1), 0x97, "48;5;12 -> bright blue bg (VGA 9)");
    assert_eq!(attr(&t, 0, 2), 0x71, "reverse swaps grey/blue");
    assert_eq!(attr(&t, 0, 3), 0xFC, "bold+bright-red bg reversed: bright red on bright white, no bit dropped");
    let t = run(b"\x1b[1;31;102;7mA\x1b[22mB\x1b[27mC\x1b[39mD\x1b[49mE");
    assert_eq!(attr(&t, 0, 0), 0xCA, "bold red on bright green, reversed");
    assert_eq!(attr(&t, 0, 1), 0x4A, "22: bold off");
    assert_eq!(attr(&t, 0, 2), 0xA4, "27: reverse off");
    assert_eq!(attr(&t, 0, 3), 0xA7, "39: default fg, bg kept");
    assert_eq!(attr(&t, 0, 4), 0x07, "49: default bg");
}

#[test]
fn dsr_replies() {
    let mut t = run(b"\x1b[5n");
    assert_eq!(reply(&mut t), b"\x1b[0n");
    let mut t = run(b"\x1b[12;34Hx\x1b[6n");
    assert_eq!(reply(&mut t), b"\x1b[12;35R");
    let mut t = run(b"\x1b[3;4H\x1b[6n");
    assert_eq!(reply(&mut t), b"\x1b[3;4R", "single digits, no leading zero");
    let mut t = run(&with(vec![b'a'; 80], b"\x1b[6n"));
    assert_eq!(reply(&mut t), b"\x1b[1;80R", "pending wrap reports the last column");
    let mut t = run(b"\x1b[25;80H\x1b[?6n\x1b[6 n");
    assert_eq!(reply(&mut t), b"", "DECXCPR / intermediates: no plain CPR");
    assert_eq!(t.counts, [0, 0, 0, 0, 0]);
}

#[test]
fn device_attributes() {
    let mut t = run(b"\x1b[c\x1b[0c");
    assert_eq!(reply(&mut t), b"\x1b[?6c\x1b[?6c", "DA1: VT102");
    let mut t = run(b"\x1b[>c\x1b[>0c");
    assert_eq!(reply(&mut t), b"\x1b[>0;10;0c\x1b[>0;10;0c", "DA2");
    let mut t = run(b"\x1b[=c\x1b[1c");
    assert_eq!(reply(&mut t), b"", "DA3 and nonzero DA: no reply");
    assert_eq!(row(&t, 0), "", "queries draw nothing");
}

#[test]
fn reply_buffer_is_bounded_and_drains_partially() {
    let mut v = Vec::new();
    for _ in 0..20 {
        v.extend(b"\x1b[5n");
    }
    let mut t = run(&v);
    assert_eq!(t.counts[1], 16, "64-byte queue holds 16 four-byte replies");
    assert_eq!(t.counts[4], 4, "the rest are counted as dropped, not truncated");
    let mut b = [0u8; 6];
    assert_eq!(t.take_reply(&mut b), 6);
    assert_eq!(&b, b"\x1b[0n\x1b[");
    assert_eq!(reply(&mut t)[..6], *b"0n\x1b[0n", "the rest moves to the front");
    let mut v = Vec::new();
    for _ in 0..15 {
        v.extend(b"\x1b[5n");
    }
    let mut t = run(&with(v, b"\x1b[12;34H\x1b[6n\x1b[5n"));
    assert_eq!((t.counts[0], t.counts[1], t.counts[4]), (0, 16, 1), "an 8-byte CPR that does not fit is dropped whole");
    assert_eq!(reply(&mut t).len(), 64);
}

#[test]
fn utf8_to_cp437() {
    let t = run("box: ─│┌┐ deg: ° ±÷×é▀█░".as_bytes());
    assert_eq!(cells(&t, 0, 22), [b'b', b'o', b'x', b':', b' ', 0xC4, 0xB3, 0xDA, 0xBF, b' ', b'd', b'e', b'g', b':', b' ', 0xF8, b' ', 0xF1, 0xF6, b'?', 0x82, 0xDF]);
    assert_eq!(cells(&t, 0, 24)[22..], [0xDB, 0xB0]);
    let t = run("→▲♥⌂".as_bytes());
    assert_eq!(cells(&t, 0, 4), [0x1A, 0x1E, 0x03, 0x7F], "glyphs in CP437's control range");
}

#[test]
fn utf8_invalid_and_interrupted() {
    let t = run(b"a\xbdb\xe2\x94c\xc0\xafd\xed\xa0\x80e\xe0\x81\x81g\xe2\x94\x1b[1mf");
    assert_eq!(row(&t, 0), "a?b?c??d?e?g?f", "stray continuation, cut-short, invalid lead, surrogate, overlong 'A', ESC mid-sequence");
    assert_eq!(attr(&t, 0, 13), 0x0F, "the ESC that cut the sequence still starts the CSI");
    let t = run(b"e\xcc\x81x\xc2\x85y");
    assert_eq!(row(&t, 0), "exy", "combining mark and C1 take no cell");
}

#[test]
fn wide_chars_take_two_cells() {
    let t = run("cjk: 中文|".as_bytes());
    assert_eq!(row(&t, 0), "cjk: ????|");
    assert_eq!(t.cursor(), (0, 10));
    let t = run(&with(vec![b'a'; 79], "中x".as_bytes()));
    assert_eq!(row(&t, 0).len(), 79, "no room at the last column: the wide char wraps whole");
    assert_eq!(row(&t, 1), "??x");
    let t = run("\x1b[3;1H中\x1b[2b|".as_bytes());
    assert_eq!(row(&t, 2), "??????|", "REP repeats a wide char as wide");
    let t = run("か\u{3099}X \u{628}\u{64E}Y 🚀Z".as_bytes());
    assert_eq!(row(&t, 0), "??X ?Y ??Z", "kana + combining dakuten, Arabic + fatha (zero width), emoji (wide)");
    let t = run(&with(with(vec![b'a'; 79], b"\x1b[?7lS"), "中".as_bytes()));
    assert_eq!((t.cell(0, 79) & 0xFF) as u8, b'S', "autowrap off, no room: the wide char is dropped, the last cell kept");
    assert_eq!(row(&t, 1), "");
    let t = run(&with(with(vec![b'a'; 79], b"\x1b[?7lS\x1b[4h"), "中".as_bytes()));
    assert_eq!((t.cell(0, 79) & 0xFF) as u8, b'S', "same in insert mode");
}


#[test]
fn exact_colours_kept_per_cell() {
    let t = run(b"\x1b[38;2;10;200;30;48;5;196mX\x1b[0mY\x1b[38;2;0;0;0mZ\x1b[7mR");
    let c = t.cell_full(0, 0);
    assert_eq!((c >> 16 & 0xFF_FFFF, c >> 40 & 0xFF_FFFF), (0x0AC81E, 0xFF0000), "truecolour fg, 256-colour bg (196 = red)");
    assert_eq!(t.cell_full(0, 1) >> 16, 0, "SGR 0: back to the palette");
    let b = run(b"\x1b[38;2;0;255;128mA\x1b[38;5;19mB");
    assert_eq!(b.cell_full(0, 0) >> 16 & 0xFF_FFFF, 0x00FF80, "a colour without red is kept, not turned black");
    assert_eq!(b.cell_full(0, 1) >> 16 & 0xFF_FFFF, 0x0000AF, "256-colour 19 = 0x0000AF");
    assert_eq!(t.cell_full(0, 2) >> 16 & 0xFF_FFFF, 0x010101, "true black is stored as 0x010101 (0 means palette)");
    let r = t.cell_full(0, 3);
    assert_eq!((r >> 16 & 0xFF_FFFF, r >> 40 & 0xFF_FFFF), (0, 0x010101), "reverse video swaps the exact colours too");
}

#[test]
fn modern_tui_glyphs() {
    let t = run("╭─╮│●✓…⠋".as_bytes());
    let cells: Vec<u8> = (0..8).map(|c| (t.cell(0, c) & 0xFF) as u8).collect();
    assert_eq!(cells, [0xDA, 0xC4, 0xBF, 0xB3, 0x07, 0xFB, 0xFA, 0xF9], "rounded corners, dot, check, ellipsis, braille");
}

#[test]
fn sgr_256_and_private_m_do_not_misparse() {
    let t = run(b"\x1b[38;5;4mA\x1b[0m\x1b[>4;2mB\x1b[38;2;200;0;0;1mC\x1b[0m\x1b[48;5;236mD\x1b[0m\x1b[38;5;214mE");
    assert_eq!(attr(&t, 0, 0), 0x01, "38;5;4 -> blue");
    assert_eq!(attr(&t, 0, 1), 0x07, "CSI > 4;2 m is modifyOtherKeys, not SGR");
    assert_eq!(attr(&t, 0, 2), 0x0C, "truecolour dark red -> red, and the trailing 1 (bold) still brightens it");
    assert_eq!(attr(&t, 0, 3) >> 4, 0x00, "256-colour dark grey background -> black");
    assert_eq!(attr(&t, 0, 4) & 0x0F, 0x06, "256-colour orange (214) -> yellow/brown");
    let t = run(b"\x1b[38:2::0:0:31mC");
    assert_eq!(attr(&t, 0, 0), 0x07, "colon-form truecolour: the 31 (blue component) is not SGR red");
    assert_eq!(row(&run(b"keep\x1b[?2J\x1b[?1K"), 0), "keep", "DECSED/DECSEL (private) are not ED/EL");
}

#[test]
fn reverse_video() {
    let t = run(b"\x1b[31;7mX\x1b[27mY\x1b[0;7mZ");
    assert_eq!(attr(&t, 0, 0), 0x40, "red reversed: black on red");
    assert_eq!(attr(&t, 0, 1), 0x04);
    assert_eq!(attr(&t, 0, 2), 0x70, "default reversed: black on grey");
}

#[test]
fn erase_uses_current_background_bce() {
    let t = run(b"\x1b[44m\x1b[2J\x1b[41mAB\x1b[1D\x1b[K");
    assert_eq!(attr(&t, 10, 10), 0x17, "ED 2 fills with blue background");
    assert_eq!(attr(&t, 0, 0), 0x47);
    assert_eq!(attr(&t, 0, 1), 0x47, "EL from the cursor fills with red background");
    assert_eq!(attr(&t, 0, 79), 0x47);
    assert_eq!(row(&t, 0), "A");
}

#[test]
fn scroll_region_lf_scrolls_only_the_region() {
    let t = run(&with(labelled(6), b"\x1b[2;4r\x1b[4;1H\n"));
    assert_eq!((row(&t, 0), row(&t, 1), row(&t, 2), row(&t, 3), row(&t, 4)), ("r0".into(), "r2".into(), "r3".into(), "".into(), "r4".into()));
    assert_eq!(t.cursor(), (3, 0));
}

#[test]
fn decstbm_homes_the_cursor() {
    let t = run(b"\x1b[10;10H\x1b[3;20r");
    assert_eq!(t.cursor(), (0, 0));
}

#[test]
fn reverse_index_at_region_top_scrolls_region_down() {
    let t = run(&with(labelled(6), b"\x1b[2;4r\x1b[2;1H\x1bM"));
    assert_eq!((row(&t, 0), row(&t, 1), row(&t, 2), row(&t, 3), row(&t, 4)), ("r0".into(), "".into(), "r1".into(), "r2".into(), "r4".into()));
}

#[test]
fn insert_delete_line_stay_inside_region() {
    let t = run(&with(labelled(6), b"\x1b[2;4r\x1b[3;5H\x1b[L"));
    assert_eq!((row(&t, 1), row(&t, 2), row(&t, 3), row(&t, 4)), ("r1".into(), "".into(), "r2".into(), "r4".into()));
    assert_eq!(t.cursor(), (2, 0), "IL moves to column 0");
    let t = run(&with(labelled(6), b"\x1b[2;4r\x1b[2;1H\x1b[2M"));
    assert_eq!((row(&t, 1), row(&t, 2), row(&t, 3), row(&t, 4)), ("r3".into(), "".into(), "".into(), "r4".into()));
}

#[test]
fn scroll_up_down_su_sd() {
    let t = run(&with(labelled(4), b"\x1b[2S"));
    assert_eq!((row(&t, 0), row(&t, 1), row(&t, 2)), ("r2".into(), "r3".into(), "".into()));
    let t = run(&with(labelled(4), b"\x1b[T"));
    assert_eq!((row(&t, 0), row(&t, 1), row(&t, 4)), ("".into(), "r0".into(), "r3".into()));
}

#[test]
fn insert_delete_char() {
    let t = run(b"ABCDE\x1b[1;2H\x1b[2@");
    assert_eq!(row(&t, 0), "A  BCDE");
    let t = run(b"ABCDE\x1b[1;2H\x1b[2P");
    assert_eq!(row(&t, 0), "ADE");
    let t = run(b"ABCDE\r\x1b[4hX");
    assert_eq!(row(&t, 0), "XABCDE", "IRM insert mode");
}

#[test]
fn save_restore_cursor_and_attributes() {
    let t = run(b"\x1b[5;10H\x1b[32m\x1b7\x1b[1;1H\x1b[0mX\x1b8Y");
    assert_eq!((t.cell(4, 9) & 0xFF) as u8, b'Y');
    assert_eq!(attr(&t, 4, 9), 0x02, "ESC 8 restores the SGR state");
    let t = run(b"\x1b[3;3H\x1b[s\x1b[9;9H\x1b[uZ");
    assert_eq!(row(&t, 2), "  Z", "CSI s/u");
}

#[test]
fn alternate_screen_1049_restores_main_grid_and_cursor() {
    let mut t = run(b"main\x1b[3;3H\x1b[?1049h");
    assert_eq!(row(&t, 0), "", "alternate screen starts clear");
    for &b in b"ALT\x1b[10;10Hjunk" {
        t.feed(b);
    }
    for &b in b"\x1b[?1049l" {
        t.feed(b);
    }
    assert_eq!(row(&t, 0), "main");
    assert_eq!(row(&t, 2), "", "nothing drawn on the alternate screen leaks");
    assert_eq!(row(&t, 9), "");
    assert_eq!(t.cursor(), (2, 2));
    let t = run(b"\x1b[3;3H\x1b[?1049h\x1b[10;10H\x1b7\x1b[?1049l");
    assert_eq!(t.cursor(), (2, 2), "an app's ESC 7 on the alternate screen must not clobber the main screen's saved cursor");
}

#[test]
fn cursor_visibility() {
    assert!(!run(b"\x1b[?25l").cursor_visible());
    assert!(run(b"\x1b[?25l\x1b[?12l\x1b[?25h").cursor_visible());
}

#[test]
fn dec_line_drawing_to_cp437() {
    let t = run(b"\x1b(0lqkx\x1b(Bq");
    let cells: Vec<u8> = (0..5).map(|c| (t.cell(0, c) & 0xFF) as u8).collect();
    assert_eq!(cells, [0xDA, 0xC4, 0xBF, 0xB3, b'q']);
    let t = run(b"\x1b)0\x0eq\x0fq");
    assert_eq!(((t.cell(0, 0) & 0xFF) as u8, (t.cell(0, 1) & 0xFF) as u8), (0xC4, b'q'), "G1 via SO/SI");
}

#[test]
fn rep_repeats_last_char() {
    let t = run(b"\x1b[32m-\x1b[4b|");
    assert_eq!(row(&t, 0), "-----|");
    assert_eq!(attr(&t, 0, 4), 0x02);
}

#[test]
fn dcs_and_osc_are_swallowed() {
    let t = run(b"\x1bP+q544e\x1b\\\x1b]11;?\x07\x1b_apc\x1b\\X");
    assert_eq!(row(&t, 0), "X");
}

#[test]
fn decckm_for_the_keyboard() {
    assert!(run(b"\x1b[?1h\x1b=").app_cursor());
    assert!(!run(b"\x1b[?1h\x1b[?1l").app_cursor());
}

#[test]
fn autowrap_off_overwrites_last_column() {
    let t = run(&with(vec![b'a'; 80], b"\x1b[?7lXY"));
    assert_eq!(row(&t, 1), "", "no wrap with ?7l");
    let t2 = run(&with(with(vec![], b"\x1b[?7l"), &[b'a'; 82]));
    assert_eq!(row(&t2, 1), "");
    assert_eq!((t.cell(0, 79) & 0xFF) as u8, b'Y');
}

#[test]
fn deferred_wrap_survives_sgr() {
    let t = run(&with(vec![b'0'; 80], b"\x1b[0mB"));
    assert_eq!(row(&t, 1), "B");
}

#[test]
fn intermediate_csi_is_not_misread() {
    let t = run(b"ABC\x1b[1;1H\x1b[1 @\x1b[2 q");
    assert_eq!(row(&t, 0), "ABC", "CSI SP @ (scroll left) is not ICH");
}

#[test]
fn soft_and_hard_reset() {
    let t = run(b"\x1b[2;5r\x1b[?1h\x1b[31m\x1b[!pX");
    assert_eq!(attr(&t, 0, 0), 0x07);
    assert!(!t.app_cursor());
    let t = run(b"\x1b[44mhello\x1b[?1049h\x1bc");
    assert_eq!(row(&t, 0), "");
    assert_eq!(attr(&t, 0, 0), 0x07);
    assert_eq!(t.cursor(), (0, 0));
}

#[test]
fn full_screen_scroll_still_works() {
    let mut v = Vec::new();
    for i in 0..30 {
        v.extend(format!("{i}\r\n").bytes());
    }
    let t = run(&v);
    assert_eq!(row(&t, 0), "6");
    assert_eq!(row(&t, H - 2), "29");
}

fn sized(w: usize, h: usize, s: &[u8]) -> Box<Term> {
    let mut t = Box::new(Term::new());
    t.resize(w, h);
    for &b in s {
        t.feed(b);
    }
    t
}
fn row_w(t: &Term, r: usize) -> String {
    (0..t.w).map(|c| (t.cell(r, c) & 0xFF) as u8 as char).collect::<String>().trim_end().to_string()
}

#[test]
fn panel_sizes_cursor_clamps_to_the_real_edge() {
    for (w, h) in [(128, 37), (170, 50)] {
        let t = sized(w, h, b"\x1b[999;999HX");
        assert_eq!(t.cursor(), (h - 1, w - 1), "{}x{}", w, h);
        assert_eq!((t.cell(h - 1, w - 1) & 0xFF) as u8, b'X');
    }
}

#[test]
fn panel_sizes_scroll_at_the_last_row() {
    for (w, h) in [(128, 37), (170, 50)] {
        let mut v = Vec::new();
        for r in 0..h {
            v.extend(format!("\x1b[{};1Hr{}", r + 1, r).bytes());
        }
        v.extend(b"\x1b[999;1H\n");
        let t = sized(w, h, &v);
        assert_eq!(row_w(&t, 0), "r1", "{}x{}", w, h);
        assert_eq!(row_w(&t, h - 2), format!("r{}", h - 1));
        assert_eq!(row_w(&t, h - 1), "");
    }
}

#[test]
fn panel_sizes_wrap_at_the_real_width() {
    let line: Vec<u8> = std::iter::repeat(b'a').take(170).chain(*b"b").collect();
    let t = sized(170, 50, &line);
    assert_eq!(row_w(&t, 0).len(), 170);
    assert_eq!(row_w(&t, 1), "b");
    let t = sized(128, 37, &line);
    assert_eq!(row_w(&t, 0).len(), 128);
    assert_eq!(row_w(&t, 1), format!("{}b", "a".repeat(42)));
}

#[test]
fn panel_sizes_erase_line_and_screen() {
    let t = sized(170, 50, b"\x1b[10;1Hhello world\x1b[10;6H\x1b[K\x1b[50;170HZ\x1b[50;170H\x1b[1K");
    assert_eq!(row_w(&t, 9), "hello");
    assert_eq!(row_w(&t, 49), "");
    let t = sized(128, 37, b"\x1b[37;128HQ\x1b[2J");
    assert_eq!(row_w(&t, 36), "");
}

#[test]
fn panel_sizes_alternate_screen_round_trip() {
    let t = sized(170, 50, b"\x1b[50;1Hmain\x1b[?1049h\x1b[50;160Halt\x1b[?1049l");
    assert_eq!(row_w(&t, 49), "main");
}

#[test]
fn panel_sizes_insert_delete_chars_at_the_real_width() {
    let t = sized(170, 50, &[b"\x1b[1;165Habcdef".as_slice(), b"\x1b[1;165H\x1b[2@"].concat());
    assert_eq!((t.cell(0, 169) & 0xFF) as u8, b'd'); // "abcdef" at 165..170 shifted right by 2: e, f fall off
    let t = sized(128, 37, &[b"\x1b[1;120Habcdefgh".as_slice(), b"\x1b[1;120H\x1b[3P"].concat());
    assert_eq!(&row_w(&t, 0)[119..], "defgh");
}
