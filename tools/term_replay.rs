//! Replay a byte stream through src/term.rs on the host and print the grid as hex cells (one row per line).
//! Usage: term_replay <file> [<byte offset to stop at>]. Used by tools/term-vs-pyte.py.
#[path = "../src/term.rs"]
#[allow(dead_code)]
mod term;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let data = std::fs::read(&a[1]).unwrap();
    let end = a.get(2).map(|s| s.parse().unwrap()).unwrap_or(data.len());
    let mut t = Box::new(term::Term::new());
    for &b in &data[..end] {
        t.feed(b);
    }
    for r in 0..term::H {
        println!("{}", (0..term::W).map(|c| format!("{:04x}", t.cell(r, c))).collect::<Vec<_>>().join(" "));
    }
    let (r, c) = t.cursor();
    println!("cursor {r} {c} {}", t.cursor_visible());
}
