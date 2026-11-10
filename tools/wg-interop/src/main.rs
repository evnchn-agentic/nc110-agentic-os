//! Runs the kernel's `src/wg.rs`, unchanged, on the host against Linux kernel WireGuard (see `tools/wg-interop.sh`).
//! The modules `wg.rs` calls into are stubbed here: the WiFi UDP socket is a host UDP socket, the clocks are the host's
//! with offsets, entropy is /dev/urandom.
//!
//! usage: wg-interop <our priv hex> <peer pub hex> <server ip> <port> <tunnel ip/len> <seconds>
//!   env RTC_OFFSET=<s>     added to the host's Unix time for the RTC (default 28800: the NC110 RTC holds HKT)
//!   env REKEY_AT=<s>       at this second, jump the uptime clock past REKEY_MS so the next poll re-handshakes
//!   env DROP_CONFIRM=<n>   drop our first data packet after the n-th handshake response (a lost WiFi frame)
//! Prints one `stat` line per second and the final `wg stat`.
#![allow(static_mut_refs, dead_code)]

use std::sync::atomic::{AtomicBool, AtomicI64, Ordering::Relaxed};
use std::sync::OnceLock;

#[macro_export]
macro_rules! kprintln {
    ($($arg:tt)*) => {{ eprintln!("[kernel] {}", format!($($arg)*)); }};
}
#[macro_export]
macro_rules! kprint {
    ($($arg:tt)*) => {{ eprint!("{}", format!($($arg)*)); }};
}

#[path = "../../../src/wg.rs"]
mod wg;

static UPTIME_OFFSET: AtomicI64 = AtomicI64::new(0);
static DROP_NEXT_DATA: AtomicBool = AtomicBool::new(false);
static RESPONSES: AtomicI64 = AtomicI64::new(0);

mod time {
    use super::*;
    static T0: OnceLock<std::time::Instant> = OnceLock::new();
    pub fn now_ms() -> i64 {
        T0.get_or_init(std::time::Instant::now).elapsed().as_millis() as i64 + UPTIME_OFFSET.load(Relaxed)
    }
    pub fn rtc_unix() -> Option<u64> {
        let off: i64 = std::env::var("RTC_OFFSET").ok().and_then(|v| v.parse().ok()).unwrap_or(28800);
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).ok()?.as_secs() as i64;
        Some((now + off) as u64)
    }
}

mod rng {
    pub fn fill(buf: &mut [u8]) -> Result<(), ()> {
        use std::io::Read;
        std::fs::File::open("/dev/urandom").and_then(|mut f| f.read_exact(buf)).map_err(|_| ())
    }
}

mod wdt {
    pub fn pet() {}
}

mod net {
    pub trait Link: smoltcp::phy::Device {}
}

mod bmctl {
    // Same RFC 8439 construction as src/bmctl.rs seal/open.
    use chacha20::cipher::{KeyIvInit, StreamCipher};
    use chacha20::ChaCha20;
    use poly1305::universal_hash::{KeyInit, UniversalHash};
    use poly1305::Poly1305;
    fn tag(pk: &[u8; 32], aad: &[u8], ct: &[u8]) -> [u8; 16] {
        let mut m = Poly1305::new(pk.into());
        m.update_padded(aad);
        m.update_padded(ct);
        let mut lens = [0u8; 16];
        lens[..8].copy_from_slice(&(aad.len() as u64).to_le_bytes());
        lens[8..].copy_from_slice(&(ct.len() as u64).to_le_bytes());
        m.update_padded(&lens);
        m.finalize().into()
    }
    fn cipher(key: &[u8; 32], n: &[u8; 12]) -> (ChaCha20, [u8; 32]) {
        let mut c = ChaCha20::new(key.into(), n.into());
        let mut b0 = [0u8; 64];
        c.apply_keystream(&mut b0);
        let mut pk = [0u8; 32];
        pk.copy_from_slice(&b0[..32]);
        (c, pk)
    }
    pub fn seal(key: &[u8; 32], n: &[u8; 12], aad: &[u8], buf: &mut [u8]) -> [u8; 16] {
        let (mut c, pk) = cipher(key, n);
        c.apply_keystream(buf);
        tag(&pk, aad, buf)
    }
    pub fn open(key: &[u8; 32], n: &[u8; 12], aad: &[u8], buf: &mut [u8], t: &[u8]) -> bool {
        let (mut c, pk) = cipher(key, n);
        if t.len() != 16 || tag(&pk, aad, buf) != t {
            return false;
        }
        c.apply_keystream(buf);
        true
    }
}

mod iwl {
    use super::*;
    use std::net::UdpSocket;
    static SOCK: OnceLock<UdpSocket> = OnceLock::new();
    fn sock() -> &'static UdpSocket {
        SOCK.get_or_init(|| {
            let s = UdpSocket::bind("0.0.0.0:0").unwrap();
            s.set_nonblocking(true).unwrap();
            s
        })
    }
    pub fn online() -> bool {
        true
    }
    pub fn net_poll() {}
    pub fn address() -> Option<(heapless::String<32>, smoltcp::wire::Ipv4Cidr)> {
        Some((heapless::String::new(), smoltcp::wire::Ipv4Cidr::new(smoltcp::wire::Ipv4Address::new(192, 0, 2, 10), 24)))
    }
    pub fn resolve(_name: &str, _ms: i64) -> Option<smoltcp::wire::Ipv4Address> {
        None
    }
    pub fn udp_send(to: smoltcp::wire::IpEndpoint, d: &[u8]) -> bool {
        if d.first() == Some(&4) && DROP_NEXT_DATA.swap(false, Relaxed) {
            eprintln!("[harness] dropped our first data packet after the handshake (lost WiFi frame)");
            return true;
        }
        let smoltcp::wire::IpAddress::Ipv4(a) = to.addr;
        sock().send_to(d, (std::net::Ipv4Addr::from(a.octets()), to.port)).is_ok()
    }
    pub fn udp_recv(buf: &mut [u8]) -> Option<usize> {
        let n = sock().recv(buf).ok()?;
        if buf.first() == Some(&2) {
            let n = RESPONSES.fetch_add(1, Relaxed) + 1;
            if std::env::var("DROP_CONFIRM").ok().and_then(|v| v.parse().ok()) == Some(n) {
                DROP_NEXT_DATA.store(true, Relaxed);
            }
        }
        Some(n)
    }
}

fn run(cmd: &str) -> String {
    let mut out = String::new();
    wg::cmd(&mut out, &mut cmd.split_whitespace());
    out
}

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    assert!(a.len() == 6, "usage: wg-interop <priv hex> <peer pub hex> <server ip> <port> <tunnel ip/len> <seconds>");
    println!("set: {}", run(&format!("set {} {} {} {} {}", a[0], a[1], a[2], a[3], a[4])));
    println!("up: {}", run("up"));
    let secs: i64 = a[5].parse().unwrap();
    let rekey_at: Option<i64> = std::env::var("REKEY_AT").ok().and_then(|v| v.parse().ok());
    let t0 = std::time::Instant::now();
    let mut last = 0;
    while t0.elapsed().as_secs() < secs as u64 {
        let s = t0.elapsed().as_secs() as i64;
        if rekey_at == Some(s) && UPTIME_OFFSET.load(Relaxed) == 0 {
            UPTIME_OFFSET.store(121_000, Relaxed);
            println!("t={s}: uptime clock +121 s (keys now past REKEY_MS)");
        }
        wg::tick();
        if s != last {
            last = s;
            println!("t={s}: {}", run("stat"));
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    println!("final: {}", run("stat"));
}
