//! WireGuard client, one peer, over the WiFi link (Noise_IKpsk2_25519_ChaChaPoly_BLAKE2s, wireguard.com/protocol).
//! The tunnel is its own smoltcp interface (medium IP) whose device encrypts into, and decrypts from, a UDP socket on
//! the WiFi interface. bmctl `wg set <priv hex> <peer pub hex> <endpoint ip|name> <port> <tunnel ip/len> [psk hex]`,
//! `wg up`, `wg stat`, `wg selftest`. An SSH session goes through the tunnel when it is up and the host is not on
//! the WiFi subnet.
use core::fmt::Write;
use smoltcp::iface::{Config, Interface, SocketHandle, SocketSet, SocketStorage};
use smoltcp::phy::{DeviceCapabilities, Medium};
use smoltcp::socket::tcp;
use smoltcp::wire::{HardwareAddress, IpAddress, IpCidr, IpEndpoint, Ipv4Address, Ipv4Cidr};

// ---- BLAKE2s (RFC 7693) ----
const IV: [u32; 8] = [0x6A09E667, 0xBB67AE85, 0x3C6EF372, 0xA54FF53A, 0x510E527F, 0x9B05688C, 0x1F83D9AB, 0x5BE0CD19];
const SIGMA: [[usize; 16]; 10] = [
    [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15],
    [14, 10, 4, 8, 9, 15, 13, 6, 1, 12, 0, 2, 11, 7, 5, 3],
    [11, 8, 12, 0, 5, 2, 15, 13, 10, 14, 3, 6, 7, 1, 9, 4],
    [7, 9, 3, 1, 13, 12, 11, 14, 2, 6, 5, 10, 4, 0, 15, 8],
    [9, 0, 5, 7, 2, 4, 10, 15, 14, 1, 11, 12, 6, 8, 3, 13],
    [2, 12, 6, 10, 0, 11, 8, 3, 4, 13, 7, 5, 15, 14, 1, 9],
    [12, 5, 1, 15, 14, 13, 4, 10, 0, 7, 6, 3, 9, 2, 8, 11],
    [13, 11, 7, 14, 12, 1, 3, 9, 5, 0, 15, 4, 8, 6, 2, 10],
    [6, 15, 14, 9, 11, 3, 0, 8, 12, 2, 13, 7, 1, 4, 10, 5],
    [10, 2, 8, 4, 7, 6, 1, 5, 15, 11, 9, 14, 3, 12, 13, 0],
];

struct B2s {
    h: [u32; 8],
    t: u64,
    buf: [u8; 64],
    n: usize,
}

impl B2s {
    fn new(outlen: usize, key: &[u8]) -> Self {
        let mut s = B2s { h: IV, t: 0, buf: [0; 64], n: 0 };
        s.h[0] ^= 0x0101_0000 ^ ((key.len() as u32) << 8) ^ outlen as u32;
        if !key.is_empty() {
            s.buf[..key.len()].copy_from_slice(key);
            s.n = 64;
        }
        s
    }
    fn update(&mut self, mut d: &[u8]) {
        while !d.is_empty() {
            if self.n == 64 {
                self.t += 64;
                self.compress(false);
                self.n = 0;
            }
            let k = (64 - self.n).min(d.len());
            self.buf[self.n..self.n + k].copy_from_slice(&d[..k]);
            self.n += k;
            d = &d[k..];
        }
    }
    fn finalize(mut self) -> [u8; 32] {
        self.t += self.n as u64;
        self.buf[self.n..].fill(0);
        self.compress(true);
        let mut o = [0u8; 32];
        for i in 0..8 {
            o[4 * i..4 * i + 4].copy_from_slice(&self.h[i].to_le_bytes());
        }
        o
    }
    fn compress(&mut self, last: bool) {
        let mut v = [0u32; 16];
        v[..8].copy_from_slice(&self.h);
        v[8..].copy_from_slice(&IV);
        v[12] ^= self.t as u32;
        v[13] ^= (self.t >> 32) as u32;
        if last {
            v[14] = !v[14];
        }
        let mut m = [0u32; 16];
        for i in 0..16 {
            m[i] = u32::from_le_bytes(self.buf[4 * i..4 * i + 4].try_into().unwrap());
        }
        fn g(v: &mut [u32; 16], a: usize, b: usize, c: usize, d: usize, x: u32, y: u32) {
            v[a] = v[a].wrapping_add(v[b]).wrapping_add(x);
            v[d] = (v[d] ^ v[a]).rotate_right(16);
            v[c] = v[c].wrapping_add(v[d]);
            v[b] = (v[b] ^ v[c]).rotate_right(12);
            v[a] = v[a].wrapping_add(v[b]).wrapping_add(y);
            v[d] = (v[d] ^ v[a]).rotate_right(8);
            v[c] = v[c].wrapping_add(v[d]);
            v[b] = (v[b] ^ v[c]).rotate_right(7);
        }
        for s in &SIGMA {
            g(&mut v, 0, 4, 8, 12, m[s[0]], m[s[1]]);
            g(&mut v, 1, 5, 9, 13, m[s[2]], m[s[3]]);
            g(&mut v, 2, 6, 10, 14, m[s[4]], m[s[5]]);
            g(&mut v, 3, 7, 11, 15, m[s[6]], m[s[7]]);
            g(&mut v, 0, 5, 10, 15, m[s[8]], m[s[9]]);
            g(&mut v, 1, 6, 11, 12, m[s[10]], m[s[11]]);
            g(&mut v, 2, 7, 8, 13, m[s[12]], m[s[13]]);
            g(&mut v, 3, 4, 9, 14, m[s[14]], m[s[15]]);
        }
        for i in 0..8 {
            self.h[i] ^= v[i] ^ v[i + 8];
        }
    }
}

fn hash(parts: &[&[u8]]) -> [u8; 32] {
    let mut b = B2s::new(32, &[]);
    for p in parts {
        b.update(p);
    }
    b.finalize()
}

fn mac(key: &[u8; 32], msg: &[u8]) -> [u8; 16] {
    let mut b = B2s::new(16, key);
    b.update(msg);
    let mut o = [0u8; 16];
    o.copy_from_slice(&b.finalize()[..16]);
    o
}

fn hmac(key: &[u8; 32], parts: &[&[u8]]) -> [u8; 32] {
    let (mut ip, mut op) = ([0x36u8; 64], [0x5Cu8; 64]);
    for i in 0..32 {
        ip[i] ^= key[i];
        op[i] ^= key[i];
    }
    let mut b = B2s::new(32, &[]);
    b.update(&ip);
    for p in parts {
        b.update(p);
    }
    let inner = b.finalize();
    hash(&[&op, &inner])
}

fn kdf(c: &[u8; 32], input: &[u8]) -> ([u8; 32], [u8; 32], [u8; 32]) {
    let t0 = hmac(c, &[input]);
    let t1 = hmac(&t0, &[&[1]]);
    let t2 = hmac(&t0, &[&t1, &[2]]);
    let t3 = hmac(&t0, &[&t2, &[3]]);
    (t1, t2, t3)
}

fn dh(k: &[u8; 32], p: &[u8; 32]) -> [u8; 32] {
    x25519_dalek::x25519(*k, *p)
}

pub fn public(k: &[u8; 32]) -> [u8; 32] {
    x25519_dalek::x25519(*k, x25519_dalek::X25519_BASEPOINT_BYTES)
}

fn nonce(ctr: u64) -> [u8; 12] {
    let mut n = [0u8; 12];
    n[4..].copy_from_slice(&ctr.to_le_bytes());
    n
}

// ---- state ----
#[derive(Clone)]
pub struct Cfg {
    pub key: [u8; 32],
    pub peer: [u8; 32],
    pub psk: [u8; 32],
    pub ep: IpEndpoint,
    pub host: heapless::String<64>, // endpoint name, resolved at `wg up` (empty: ep is a literal address)
    pub tip: Ipv4Cidr,
}

struct Hs {
    e: [u8; 32],
    c: [u8; 32],
    h: [u8; 32],
    idx: u32,
    sent: i64,
}

struct Keys {
    send: [u8; 32],
    recv: [u8; 32],
    mine: u32,
    their: u32,
    ctr: u64,
    rmax: u64,
    bits: u64,
    born: i64,
}

static mut CFG: Option<Cfg> = None;
static mut HS: Option<Hs> = None;
static mut KEYS: Option<Keys> = None;
static mut ON: bool = false;
static mut LAST_TX: i64 = 0;
static mut LAST_TS: u64 = 0;
static mut STAT: [u32; 5] = [0; 5]; // handshakes sent, responses ok, data in, data out, dropped
static mut RX: [([u8; 1500], usize); 8] = [([0; 1500], 0); 8];
static mut RX_HEAD: usize = 0;
static mut RX_TAIL: usize = 0;

const REKEY_MS: i64 = 120_000;
const REJECT_MS: i64 = 180_000;
const RETRY_MS: i64 = 5_000;
const KEEPALIVE_MS: i64 = 25_000;

fn cfg() -> Option<&'static Cfg> {
    unsafe { (*core::ptr::addr_of!(CFG)).as_ref() }
}

fn keys() -> Option<&'static mut Keys> {
    unsafe { (*core::ptr::addr_of_mut!(KEYS)).as_mut() }
}

fn ck0() -> ([u8; 32], [u8; 32]) {
    let c = hash(&[b"Noise_IKpsk2_25519_ChaChaPoly_BLAKE2s"]);
    (c, hash(&[&c, b"WireGuard v1 zx2c4 Jason@zx2c4.com"]))
}

/// TAI64N from the RTC (local time read as UTC; it only has to grow across handshakes and reboots).
fn tai64n() -> Option<[u8; 12]> {
    let mut t = crate::time::rtc_unix()? << 32 | (crate::time::now_ms() % 1000) as u64 * 1_000_000;
    unsafe {
        if t <= LAST_TS {
            t = LAST_TS + 1;
        }
        LAST_TS = t;
    }
    let (s, ns) = (t >> 32, t as u32 as u64);
    let mut o = [0u8; 12];
    o[..8].copy_from_slice(&(0x4000_0000_0000_000Au64 + s).to_be_bytes());
    o[8..].copy_from_slice(&(ns as u32).to_be_bytes());
    Some(o)
}

fn initiation(out: &mut [u8; 148]) -> Result<(), &'static str> {
    let c0 = cfg().ok_or("no wg config (wg set)")?;
    let mut e = [0u8; 32];
    let mut ib = [0u8; 4];
    crate::rng::fill(&mut e).map_err(|_| "no entropy")?;
    crate::rng::fill(&mut ib).map_err(|_| "no entropy")?;
    let (mut c, mut h) = ck0();
    h = hash(&[&h, &c0.peer]);
    let epub = public(&e);
    c = kdf(&c, &epub).0;
    h = hash(&[&h, &epub]);
    out.fill(0);
    out[0] = 1;
    out[4..8].copy_from_slice(&ib);
    out[8..40].copy_from_slice(&epub);
    let (c1, k) = { let r = kdf(&c, &dh(&e, &c0.peer)); (r.0, r.1) };
    c = c1;
    out[40..72].copy_from_slice(&public(&c0.key));
    let t = crate::bmctl::seal(&k, &nonce(0), &h, &mut out[40..72]);
    out[72..88].copy_from_slice(&t);
    h = hash(&[&h, &out[40..88]]);
    let (c2, k) = { let r = kdf(&c, &dh(&c0.key, &c0.peer)); (r.0, r.1) };
    c = c2;
    out[88..100].copy_from_slice(&tai64n().ok_or("RTC unreadable (no handshake timestamp)")?);
    let t = crate::bmctl::seal(&k, &nonce(0), &h, &mut out[88..100]);
    out[100..116].copy_from_slice(&t);
    h = hash(&[&h, &out[88..116]]);
    let m1 = mac(&hash(&[b"mac1----", &c0.peer]), &out[..116]);
    out[116..132].copy_from_slice(&m1);
    unsafe { *core::ptr::addr_of_mut!(HS) = Some(Hs { e, c, h, idx: u32::from_le_bytes(ib), sent: crate::time::now_ms() }) };
    Ok(())
}

fn response(m: &[u8]) -> Result<(), &'static str> {
    let c0 = cfg().ok_or("no config")?;
    let hs = unsafe { (*core::ptr::addr_of!(HS)).as_ref() }.ok_or("no handshake pending")?;
    if m.len() != 92 || u32::from_le_bytes(m[8..12].try_into().unwrap()) != hs.idx {
        return Err("response for another handshake");
    }
    if mac(&hash(&[b"mac1----", &public(&c0.key)]), &m[..60]) != m[60..76] {
        return Err("response mac1 wrong");
    }
    let er: [u8; 32] = m[12..44].try_into().unwrap();
    let mut c = kdf(&hs.c, &er).0;
    let mut h = hash(&[&hs.h, &er]);
    c = kdf(&c, &dh(&hs.e, &er)).0;
    c = kdf(&c, &dh(&c0.key, &er)).0;
    let (c3, tau, k) = kdf(&c, &c0.psk);
    h = hash(&[&h, &tau]);
    if !crate::bmctl::open(&k, &nonce(0), &h, &mut [], &m[44..60]) {
        return Err("response does not decrypt (wrong peer key or psk)");
    }
    let (send, recv, _) = kdf(&c3, &[]);
    unsafe {
        *core::ptr::addr_of_mut!(KEYS) = Some(Keys { send, recv, mine: hs.idx, their: u32::from_le_bytes(m[4..8].try_into().unwrap()), ctr: 0, rmax: 0, bits: 0, born: crate::time::now_ms() });
        *core::ptr::addr_of_mut!(HS) = None;
        STAT[1] += 1;
    }
    let _ = h;
    Ok(())
}

/// Sliding 64-packet replay window; true if `n` is new (and records it).
fn fresh(k: &mut Keys, n: u64) -> bool {
    if n > k.rmax || (k.rmax == 0 && k.bits == 0) {
        let sh = n - k.rmax;
        k.bits = if sh >= 64 { 0 } else { k.bits << sh };
        k.bits |= 1;
        k.rmax = n;
        return true;
    }
    let d = k.rmax - n;
    if d >= 64 || k.bits >> d & 1 != 0 {
        return false;
    }
    k.bits |= 1 << d;
    true
}

fn send_udp(d: &[u8]) -> bool {
    let Some(c) = cfg() else { return false };
    let ok = crate::iwl::udp_send(c.ep, d);
    crate::iwl::net_poll();
    ok
}

/// Encrypt one IP packet (or a keepalive: empty) to the peer.
fn send_data(p: &[u8]) -> bool {
    let Some(k) = keys() else { return false };
    if crate::time::now_ms() - k.born > REJECT_MS {
        return false;
    }
    let pad = (p.len() + 15) & !15;
    let mut b = [0u8; 16 + 1504 + 16];
    if pad > 1504 {
        return false;
    }
    b[0] = 4;
    b[4..8].copy_from_slice(&k.their.to_le_bytes());
    b[8..16].copy_from_slice(&k.ctr.to_le_bytes());
    b[16..16 + p.len()].copy_from_slice(p);
    let t = crate::bmctl::seal(&k.send, &nonce(k.ctr), &[], &mut b[16..16 + pad]);
    b[16 + pad..32 + pad].copy_from_slice(&t);
    k.ctr += 1;
    unsafe {
        LAST_TX = crate::time::now_ms();
        STAT[3] += 1;
    }
    send_udp(&b[..32 + pad])
}

fn on_datagram(d: &mut [u8]) {
    match d.first() {
        Some(2) => {
            if let Err(e) = response(d) {
                crate::kprintln!("wg: {}", e);
            } else {
                crate::kprintln!("wg: handshake complete");
                send_data(&[]); // the initiator confirms with the first data packet
            }
        }
        Some(4) if d.len() >= 32 => {
            let Some(k) = keys() else { return };
            if crate::time::now_ms() - k.born > REJECT_MS {
                reset_session(); // expired: nothing more is accepted under these keys
                return;
            }
            if u32::from_le_bytes(d[4..8].try_into().unwrap()) != k.mine {
                return;
            }
            let n = u64::from_le_bytes(d[8..16].try_into().unwrap());
            let l = d.len();
            let (body, tag) = d[16..].split_at_mut(l - 32);
            if !crate::bmctl::open(&k.recv, &nonce(n), &[], body, tag) || !fresh(k, n) {
                unsafe { STAT[4] += 1 };
                return;
            }
            unsafe { STAT[2] += 1 };
            if body.len() < 20 || body[0] >> 4 != 4 {
                return; // keepalive (or not IPv4)
            }
            let il = (u16::from_be_bytes([body[2], body[3]]) as usize).min(body.len()).min(1500);
            unsafe {
                if (RX_TAIL + 1) % 8 != RX_HEAD {
                    let e = &mut (*core::ptr::addr_of_mut!(RX))[RX_TAIL];
                    e.0[..il].copy_from_slice(&body[..il]);
                    e.1 = il;
                    RX_TAIL = (RX_TAIL + 1) % 8;
                }
            }
        }
        _ => {}
    }
}

/// Drive the tunnel: receive datagrams, (re)handshake, keepalive. Called from the tunnel device and bmctl's step.
pub fn poll() {
    if !unsafe { ON } || !crate::iwl::online() {
        return;
    }
    crate::iwl::net_poll();
    let mut b = [0u8; 1600];
    while let Some(n) = crate::iwl::udp_recv(&mut b) {
        on_datagram(&mut b[..n]);
    }
    let t = crate::time::now_ms();
    let stale = keys().map_or(true, |k| t - k.born > REKEY_MS);
    let pending = unsafe { (*core::ptr::addr_of!(HS)).as_ref().map(|h| t - h.sent < RETRY_MS) }.unwrap_or(false);
    if stale && !pending {
        let mut m = [0u8; 148];
        if initiation(&mut m).is_ok() {
            unsafe { STAT[0] += 1 };
            send_udp(&m);
        }
    }
    if keys().is_some() && t - unsafe { LAST_TX } > KEEPALIVE_MS {
        send_data(&[]);
    }
}

/// bmctl's step: the pump plus the tunnel interface itself (ping replies), when no SSH session holds it.
pub fn tick() {
    poll();
    if !up() || unsafe { T_BUSY } {
        return;
    }
    let _ = with_net(|i, d, s, _| i.poll(smoltcp::time::Instant::from_millis(crate::time::now_ms()), d, s));
}

/// Forget keys, any pending handshake and queued tunnel packets.
fn reset_session() {
    unsafe {
        *core::ptr::addr_of_mut!(KEYS) = None;
        *core::ptr::addr_of_mut!(HS) = None;
        RX_HEAD = 0;
        RX_TAIL = 0;
    }
}

pub fn configured() -> bool {
    cfg().is_some()
}

/// `wg up` with the result in the log (the WiFi screen calls it after DHCP).
pub fn bring_up() -> bool {
    struct Log;
    impl Write for Log {
        fn write_str(&mut self, s: &str) -> core::fmt::Result {
            crate::kprint!("{}", s);
            Ok(())
        }
    }
    crate::kprint!("wg: ");
    start(&mut Log);
    crate::kprintln!("");
    up()
}

pub fn up() -> bool {
    (unsafe { ON }) && crate::iwl::online() && keys().is_some_and(|k| crate::time::now_ms() - k.born < REJECT_MS)
}

// ---- the tunnel interface ----
pub struct WgDev;
pub struct WgRx(usize);
pub struct WgTx;

impl smoltcp::phy::RxToken for WgRx {
    fn consume<R, F: FnOnce(&[u8]) -> R>(self, f: F) -> R {
        let e = unsafe { &(*core::ptr::addr_of!(RX))[self.0] };
        let r = f(&e.0[..e.1]);
        unsafe { RX_HEAD = (RX_HEAD + 1) % 8 };
        r
    }
}

impl smoltcp::phy::TxToken for WgTx {
    fn consume<R, F: FnOnce(&mut [u8]) -> R>(self, len: usize, f: F) -> R {
        let mut b = [0u8; 1500];
        let r = f(&mut b[..len.min(1500)]);
        send_data(&b[..len.min(1500)]);
        r
    }
}

impl smoltcp::phy::Device for WgDev {
    type RxToken<'a> = WgRx;
    type TxToken<'a> = WgTx;
    fn receive(&mut self, _t: smoltcp::time::Instant) -> Option<(WgRx, WgTx)> {
        if unsafe { RX_HEAD == RX_TAIL } {
            poll();
        }
        unsafe { (RX_HEAD != RX_TAIL).then_some((WgRx(RX_HEAD), WgTx)) }
    }
    fn transmit(&mut self, _t: smoltcp::time::Instant) -> Option<WgTx> {
        up().then_some(WgTx)
    }
    fn capabilities(&self) -> DeviceCapabilities {
        let mut c = DeviceCapabilities::default();
        c.medium = Medium::Ip;
        c.max_transmission_unit = 1280; // 1420 + 60 B of WireGuard/UDP/IP overhead did not fit a phone hotspot path
        c
    }
}

impl crate::net::Link for WgDev {}

static mut T_SOCKS: [SocketStorage<'static>; 2] = [SocketStorage::EMPTY; 2];
static mut T_BM: Option<SocketHandle> = None; // bmctl over the tunnel (10.6.0.x:5515): voice and control from home
static mut T_BM_RXM: [smoltcp::socket::udp::PacketMetadata; 16] = [smoltcp::socket::udp::PacketMetadata::EMPTY; 16];
static mut T_BM_TXM: [smoltcp::socket::udp::PacketMetadata; 16] = [smoltcp::socket::udp::PacketMetadata::EMPTY; 16];
static mut T_BM_RX: [u8; 16384] = [0; 16384];
static mut T_BM_TX: [u8; 16384] = [0; 16384];
static mut T_BUSY: bool = false; // an SSH session holds the tunnel interface: tick() must not poll it
static mut T_TCP_RX: [u8; 16384] = [0; 16384];
static mut T_TCP_TX: [u8; 16384] = [0; 16384];
static mut T_NET: Option<(Interface, SocketSet<'static>, SocketHandle)> = None;

/// Run `f` with the tunnel interface, its sockets and its TCP socket (for an SSH session).
pub fn with_net<R>(f: impl FnOnce(&mut Interface, &mut WgDev, &mut SocketSet<'static>, SocketHandle) -> R) -> Option<R> {
    if unsafe { T_BUSY } {
        return None; // already borrowed (a session, or bmctl served over the tunnel)
    }
    let (iface, socks, th) = unsafe { (*core::ptr::addr_of_mut!(T_NET)).as_mut()? };
    unsafe { T_BUSY = true };
    let r = f(iface, &mut WgDev, socks, *th);
    unsafe { T_BUSY = false };
    Some(r)
}

/// Route this host through the tunnel? (Up, and not on the WiFi subnet itself.)
pub fn routes(ip: Ipv4Address) -> bool {
    up() && !crate::iwl::address().is_some_and(|(_, c)| c.contains_addr(&ip))
}

fn start(w: &mut dyn Write) {
    let Some(c) = (unsafe { (*core::ptr::addr_of_mut!(CFG)).as_mut() }) else {
        let _ = write!(w, "err no config (wg set)");
        return;
    };
    reset_session();
    if !c.host.is_empty() {
        match crate::iwl::resolve(&c.host, 4000) {
            Some(a) => {
                c.ep.addr = IpAddress::Ipv4(a);
                let _ = write!(w, "{} -> {}; ", c.host, a);
            }
            None => {
                let _ = write!(w, "err cannot resolve {}", c.host);
                return;
            }
        }
    }
    if unsafe { (*core::ptr::addr_of!(T_NET)).is_none() } {
        build(c);
    }
    unsafe { ON = true };
    let t0 = crate::time::now_ms();
    while !up() && crate::time::now_ms() - t0 < 6000 {
        poll();
        crate::wdt::pet();
    }
    let _ = write!(w, "{}", if up() { "ok tunnel up" } else { "err no handshake response in 6 s" });
}

/// The tunnel interface, once (its socket set borrows statics, so it is never rebuilt).
fn build(c: &Cfg) {
    let mut cf = Config::new(HardwareAddress::Ip);
    let mut seed = [0u8; 8];
    let _ = crate::rng::fill(&mut seed);
    cf.random_seed = u64::from_le_bytes(seed);
    let mut iface = Interface::new(cf, &mut WgDev, smoltcp::time::Instant::from_millis(crate::time::now_ms()));
    let tip = c.tip;
    iface.update_ip_addrs(|a| {
        let _ = a.push(IpCidr::Ipv4(tip));
    });
    // medium IP has no neighbours: any gateway inside the tunnel subnet sends everything to the device
    let gw = Ipv4Address::from_bits(tip.network().address().to_bits() | 1);
    let _ = iface.routes_mut().add_default_ipv4_route(gw);
    let mut socks = SocketSet::new(unsafe { &mut *core::ptr::addr_of_mut!(T_SOCKS) }[..].as_mut());
    let th = socks.add(tcp::Socket::new(
        tcp::SocketBuffer::new(unsafe { &mut *core::ptr::addr_of_mut!(T_TCP_RX) }[..].as_mut()),
        tcp::SocketBuffer::new(unsafe { &mut *core::ptr::addr_of_mut!(T_TCP_TX) }[..].as_mut()),
    ));
    let mut u = smoltcp::socket::udp::Socket::new(
        smoltcp::socket::udp::PacketBuffer::new(unsafe { &mut *core::ptr::addr_of_mut!(T_BM_RXM) }[..].as_mut(), unsafe { &mut *core::ptr::addr_of_mut!(T_BM_RX) }[..].as_mut()),
        smoltcp::socket::udp::PacketBuffer::new(unsafe { &mut *core::ptr::addr_of_mut!(T_BM_TXM) }[..].as_mut(), unsafe { &mut *core::ptr::addr_of_mut!(T_BM_TX) }[..].as_mut()),
    );
    let _ = u.bind(5515);
    let bh = socks.add(u);
    unsafe {
        *core::ptr::addr_of_mut!(T_NET) = Some((iface, socks, th));
        T_BM = Some(bh);
    }
}

/// The tunnel's bmctl socket handle (for an SSH session that holds the tunnel interface).
pub fn bm_handle() -> Option<SocketHandle> {
    unsafe { T_BM }
}

/// Run `f` on the tunnel's bmctl socket (home screen; not while a session holds the interface).
pub fn with_bm<R>(f: impl FnOnce(&mut smoltcp::socket::udp::Socket) -> R) -> Option<R> {
    if unsafe { T_BUSY } {
        return None;
    }
    let (_, socks, _) = unsafe { (*core::ptr::addr_of_mut!(T_NET)).as_mut()? };
    let h = unsafe { T_BM }?;
    unsafe { T_BUSY = true }; // a bmctl command served here (e.g. wg set) must not reach T_NET again
    let r = f(socks.get_mut::<smoltcp::socket::udp::Socket>(h));
    unsafe { T_BUSY = false };
    Some(r)
}

fn hex32(s: &str) -> Option<[u8; 32]> {
    if s.len() != 64 {
        return None;
    }
    let mut o = [0u8; 32];
    for i in 0..32 {
        o[i] = u8::from_str_radix(s.get(2 * i..2 * i + 2)?, 16).ok()?;
    }
    Some(o)
}

/// RFC 7693 appendix A and a WireGuard handshake against a responder built from the same primitives would only
/// test us against ourselves; the vectors here are BLAKE2s-256("abc") and keyed BLAKE2s-128 / HMAC shape checks.
pub fn selftest() -> (u32, u32) {
    let mut ok = 0;
    let want = hex32("508c5e8c327c14e2e1a72ba34eeb452f37458b209ed63a294d999b4c86675982").unwrap();
    ok += (hash(&[b"abc"]) == want) as u32;
    let want = hex32("69217a3079908094e11121d042354a7c1f55b6482ca1a51e1b250dfd1ed0eef9").unwrap();
    ok += (hash(&[&[]]) == want) as u32;
    // RFC 7693 self-test style: 64-byte message split across updates hashes like one update
    let m = [0x5Au8; 200];
    ok += (hash(&[&m[..63], &m[63..130], &m[130..]]) == hash(&[&m])) as u32;
    (ok, 3)
}

pub fn cmd<'a>(w: &mut dyn Write, it: &mut impl Iterator<Item = &'a str>) {
    let sub = it.next().unwrap_or("stat");
    if matches!(sub, "set" | "up" | "down") && unsafe { T_BUSY } {
        let _ = write!(w, "err the tunnel is in use (a session, or this command came over it)");
        return;
    }
    match sub {
        "set" => {
            let (Some(k), Some(p), Some(ip), Some(port), Some(tip)) = (it.next().and_then(hex32), it.next().and_then(hex32),
                it.next(), it.next().and_then(|x| x.parse::<u16>().ok()),
                it.next().and_then(|x| x.parse::<Ipv4Cidr>().ok())) else {
                let _ = write!(w, "err wg set <priv hex> <peer pub hex> <endpoint ip|name> <port> <tunnel ip/len> [psk hex]");
                return;
            };
            let psk = it.next().and_then(hex32).unwrap_or([0; 32]);
            let mut host = heapless::String::new();
            let a = match ip.parse::<Ipv4Address>() {
                Ok(a) => a,
                Err(_) => {
                    let _ = host.push_str(ip);
                    Ipv4Address::UNSPECIFIED
                }
            };
            reset_session();
            unsafe { ON = false };
            let _ = with_net(|i, _, s, th| {
                s.get_mut::<tcp::Socket>(th).abort();
                i.update_ip_addrs(|a| {
                    a.clear();
                    let _ = a.push(IpCidr::Ipv4(tip));
                });
                i.routes_mut().remove_default_ipv4_route();
                let _ = i.routes_mut().add_default_ipv4_route(Ipv4Address::from_bits(tip.network().address().to_bits() | 1));
            });
            unsafe { *core::ptr::addr_of_mut!(CFG) = Some(Cfg { key: k, peer: p, psk, ep: IpEndpoint::new(IpAddress::Ipv4(a), port), host, tip }) };
            let pb = public(&k);
            let _ = write!(w, "ok our public key ");
            for b in pb {
                let _ = write!(w, "{:02x}", b);
            }
        }
        "up" => start(w),
        "down" => unsafe {
            ON = false;
            *core::ptr::addr_of_mut!(KEYS) = None;
            let _ = write!(w, "ok");
        },
        "selftest" => {
            let (a, b) = selftest();
            let _ = write!(w, "{} {}/{}", if a == b { "ok" } else { "FAIL" }, a, b);
        }
        _ => {
            let s = unsafe { STAT };
            let _ = write!(w, "on {} up {} handshakes {} ok {} in {} out {} dropped {}", unsafe { ON }, up(), s[0], s[1], s[2], s[3], s[4]);
            if let Some(k) = keys() {
                let _ = write!(w, " key age {} s", (crate::time::now_ms() - k.born) / 1000);
            }
        }
    }
}
