//! bmctl: authenticated UDP control port (5515) so an agent can inspect and drive the kernel over the LAN, the way
//! the DS client is driven over net2. Frame: [seq u64 LE][ciphertext][16-byte tag], ChaCha20-Poly1305 (RFC 8439),
//! nonce = b"C2K\0"|b"K2C\0" ++ seq, AAD = this boot's random 16-byte id. Master = SHA-256("nc110-bmctl-v1" ++
//! client seed from the key module): nothing secret is compiled in. Handshake (Codex MUST-FIX x2, nonce reuse):
//! `who` ++ a fresh random 16-byte client nonce Nc is the only clear-text request; the reply carries the boot id, a
//! NEW session number and check = SHA-256("nc110-bmctl-v1 who" ++ master ++ Nc ++ boot id ++ session)[..16].
//! Traffic key = SHA-256("nc110-bmctl-v1 session" ++ master ++ boot id ++ session ++ Nc); seq counts from 1. A
//! replayed or forged `who` reply cannot make two client runs share a key, because Nc is fresh per run, and the
//! kernel never reissues a session number in a boot. A session binds to the endpoint that asked `who`; pending and authenticated sessions live in separate tables.
//! Protocol v2 (2026-10-21, stateless cookie, after a `who` flood starved v1): the clear-text request is `who2` ++ Nc
//! and the kernel keeps NO state for it. The session number IS a cookie: the first 4 bytes of SHA-256(per-boot random
//! cookie key ++ client IP ++ port ++ 30-s time bucket ++ Nc); the reply is "nc110 v2 boot B session S bucket b check
//! C" with C = SHA-256("nc110-bmctl-v2 who" ++ master ++ Nc ++ boot ++ S ++ b)[..16]. Every client frame carries Nc and
//! the bucket's low byte after the seq (seq bit 63 set): an unknown session is created only when the cookie recomputes
//! for that endpoint in the current or previous bucket AND the frame authenticates under SHA-256("nc110-bmctl-v2
//! session" ++ master ++ boot ++ S ++ Nc). A promoted session number is remembered for as long as its cookie can
//! still recompute, so a replayed first frame cannot re-run after its session is evicted (fail closed when full). A v1 `who` gets a reply an old client rejects
//! loudly ("bad who reply ... v2-only:update-tools/bmctl.py").
//! A duplicate of the last accepted seq gets the cached reply bytes back (safe UDP retries, nothing re-executed).
//! The loop pets the watchdog; 10 minutes without a valid command -> 0xCF9 reset.
//! `peek` is read-only and allowlisted (see `peek`): RAM only inside the multiboot map and never the kernel image
//! (text, data, bss: stack, session table, keys), NIC registers only inside its 256-byte MMIO window as aligned
//! dwords, ports only an explicit allowlist of PM/TCO registers; everything else is refused. Confidentiality note:
//! RAM outside the image can hold GRUB leftovers (multiboot info, maybe a copy of the key file). That is not an
//! escalation: the bmctl master key is derived from that same seed, so any authenticated peer already holds it.
//! NIC dword reads can clear RTL8139 statistics counters (QEMU only); r8169-family dumps use the same reads.
use crate::net::Nic;
use crate::nic::{Bus, Rtl};
use crate::port::{inb, inl, inw};
use crate::{console, kprintln, rng, time, wdt};
use chacha20::cipher::{KeyIvInit, StreamCipher};
use chacha20::ChaCha20;
use poly1305::universal_hash::{KeyInit, UniversalHash};
use poly1305::Poly1305;
use sha2::{Digest, Sha256};
use smoltcp::iface::{Config, Interface, SocketSet, SocketStorage};
use smoltcp::socket::{dhcpv4, udp};
use smoltcp::time::Instant;
use smoltcp::wire::{EthernetAddress, HardwareAddress, IpAddress, IpCidr, IpEndpoint, Ipv4Address};

const PORT: u16 = 5515;
const IDLE_MS: i64 = 600_000; // default; cmdline idle=<s> (5..3600) overrides it, used by the q35 stall test

static mut SESS: [Option<Sess>; 16] = [const { None }; 16]; // authenticated
static mut SOCKS: [SocketStorage<'static>; 3] = [SocketStorage::EMPTY; 3];
static mut RX_META: [udp::PacketMetadata; 64] = [udp::PacketMetadata::EMPTY; 64]; // a flood burst must not crowd out
static mut RX_BUF: [u8; 65536] = [0; 65536]; // the one real request: room for 64 packets, drained up to 64 per step
static mut TX_META: [udp::PacketMetadata; 8] = [udp::PacketMetadata::EMPTY; 8];
static mut TX_BUF: [u8; 8192] = [0; 8192];

fn now() -> Instant {
    Instant::from_millis(time::now_ms())
}

fn nonce(dir: &[u8; 4], seq: u64) -> [u8; 12] {
    let mut n = [0u8; 12];
    n[..4].copy_from_slice(dir);
    n[4..].copy_from_slice(&seq.to_le_bytes());
    n
}

/// RFC 8439 tag over aad and ct with the one-time key from keystream block 0.
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
    c.apply_keystream(&mut b0); // block 0 -> Poly1305 key; the stream continues at block 1
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
    let want = tag(&pk, aad, buf);
    if t.len() != 16 {
        return false;
    }
    let diff = want.iter().zip(t).fold(0u8, |a, (x, y)| a | (x ^ y));
    if diff != 0 {
        return false;
    }
    c.apply_keystream(buf);
    true
}

fn session_key2(master: &[u8; 32], boot_id: &[u8; 16], sess: u32, nc: &[u8; 16]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(b"nc110-bmctl-v2 session");
    h.update(master);
    h.update(boot_id);
    h.update(sess.to_le_bytes());
    h.update(nc);
    h.finalize().into()
}

fn who2_check(master: &[u8; 32], nc: &[u8; 16], boot_id: &[u8; 16], sess: u32, bucket: u8) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(b"nc110-bmctl-v2 who");
    h.update(master);
    h.update(nc);
    h.update(boot_id);
    h.update(sess.to_le_bytes());
    h.update([bucket]);
    h.finalize().into()
}

/// The v2 cookie (= session number) for this endpoint, 30-s bucket and client nonce. Fixed-length input under a
/// per-boot random key, so a plain SHA-256 prefix MAC is enough (no length extension to exploit).
fn cookie(ck: &[u8; 32], from: &IpEndpoint, bucket: u32, nc: &[u8; 16]) -> u32 {
    let ip = match from.addr {
        IpAddress::Ipv4(a) => a.octets(),
    };
    let mut h = Sha256::new();
    h.update(ck);
    h.update(ip);
    h.update(from.port.to_le_bytes());
    h.update(bucket.to_le_bytes());
    h.update(nc);
    let d: [u8; 32] = h.finalize().into();
    u32::from_le_bytes([d[0], d[1], d[2], d[3]])
}

struct Sess {
    id: u32,
    key: [u8; 32],
    last_seq: u64,
    peer: Option<IpEndpoint>,
    last_t: i64,
    retx: u8,
    reply: [u8; 8 + 1300 + 16],
    reply_len: usize,
}

pub fn derive_key(seed: &[u8; 32]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(b"nc110-bmctl-v1");
    h.update(seed);
    h.finalize().into()
}

fn hex(b: &[u8], out: &mut heapless::String<64>) {
    for x in b {
        let _ = core::fmt::write(out, format_args!("{:02x}", x));
    }
}

extern "C" {
    static __load_start: u8;
    static __bss_end: u8;
}

/// Allowlisted read. Returns Err(reason) for anything outside the policy; never touches refused addresses.
fn peek(w: &mut Writer, nic_mmio: Option<u64>, what: &str, a: &str, b: &str) -> Result<(), &'static str> {
    let addr = u64::from_str_radix(a.trim_start_matches("0x"), 16).map_err(|_| "addr: hex")?;
    let n: u64 = b.parse().map_err(|_| "len/width: decimal")?;
    match what {
        "mem" => {
            if n == 0 || n > 512 {
                return Err("len 1..512");
            }
            let end = addr.checked_add(n).ok_or("overflow")?;
            let (ks, ke) = (core::ptr::addr_of!(__load_start) as u64, core::ptr::addr_of!(__bss_end) as u64);
            if addr < ke && ks < end {
                return Err("refused: kernel image (code, stack, keys)");
            }
            if end > 0x1_0000_0000 || !crate::mb::ram_contains(addr, n) {
                return Err("refused: not inside one multiboot RAM range below 4 GiB");
            }
            for i in 0..n {
                let v = unsafe { core::ptr::read_volatile((addr + i) as *const u8) };
                let _ = core::fmt::write(w, format_args!("{:02x}", v));
            }
        }
        "nic" => {
            let base = nic_mmio.ok_or("refused: NIC not on MMIO")?;
            if addr % 4 != 0 || n % 4 != 0 || n == 0 || addr.checked_add(n).is_none_or(|e| e > 0x100) {
                return Err("nic: dword-aligned offset/len inside 0..0x100");
            }
            for o in (addr..addr + n).step_by(4) {
                let v = unsafe { core::ptr::read_volatile((base + o) as *const u32) };
                let _ = core::fmt::write(w, format_args!("{:08x} ", v));
            }
        }
        "io" => {
            // Explicit (offset from PMBASE, width) allowlist from the NM10 datasheet (13.8.3, 13.9). Deliberately
            // absent: LV2/LV3/LV4 at +0x14..+0x16, whose READ requests C2/C3/C4 (Codex MUST-FIX), and anything else.
            const OK: [(u64, u64); 16] = [
                (0x00, 2), (0x02, 2), (0x04, 4), (0x08, 4), (0x10, 4), (0x28, 4), (0x2c, 4), (0x30, 4), (0x34, 4),
                (0x60, 2), (0x64, 2), (0x66, 2), (0x68, 2), (0x6a, 2), (0x72, 2), (0x62, 1),
            ];
            let pm = wdt::pmbase() as u64;
            let off = addr.checked_sub(pm).ok_or("io: below PMBASE")?;
            if pm == 0 || !OK.contains(&(off, n)) {
                return Err("io: only allowlisted PM/TCO registers (PMBASE+off,width): 0/2 2/2 4/4 8/4 10/4 28/4 2c/4 30/4 34/4 60/2 62/1 64/2 66/2 68/2 6a/2 72/2");
            }
            let p = u16::try_from(addr).map_err(|_| "io: port > 0xffff")?;
            let v = unsafe { match n { 1 => inb(p) as u32, 2 => inw(p) as u32, _ => inl(p) } };
            let _ = core::fmt::write(w, format_args!("{:#x}", v));
        }
        _ => return Err("peek mem <hexaddr> <len> | peek nic <hexoff> <len> | peek io <hexport> <1|2|4>"),
    }
    Ok(())
}

/// What the caller must do after `Ctl::step`; the caller owns the NIC, so it does the hop/reset itself.
pub enum Act {
    Idle,
    Reboot,
    Sleep(u32),
    Jump(crate::loader::Plan),
}

/// bmctl's per-boot state, so the same command handler runs in its own loop (`serve`) or inside the SSH loop.
pub struct Ctl {
    seed: [u8; 32],
    key: [u8; 32],
    boot_id: [u8; 16],
    bid: heapless::String<64>,
    ip: Ipv4Address,
    pub idle_ms: i64,
    nic_mmio: Option<u64>,
    last_ok: i64,
    last_hello: i64,
    last_footer: i64,
    battery: Option<Option<(u8, bool, bool)>>, // EC reading, refreshed every 5 s for the footer
    power: Option<i32>,
    pub busypoll: bool,      // cmdline `busypoll`: no idle naps (A/B power comparison)      // battery draw in mW (EC), refreshed every 5 s off AC
    ok: u32,
    bad: u32,
    replay: u32,
    retx: u32,
    ck: [u8; 32],            // v2 cookie key: per boot, random, unrelated to the seed
    promoted: [(u32, u32); 256], // v2 (session, bucket) promoted while its cookie can still recompute (2 buckets)
    promoted_n: usize,
    who2_n: u64,             // v2 `who2` replies sent, and the TSC cycles spent on them (cost per flood packet)
    who2_cyc: u64,
    who_window: i64,
    who_count: u32,
    reboot_at: Option<i64>,
    sleep_at: Option<(i64, u32)>,
    stage: Option<crate::loader::Stage>,
    jump_at: Option<(i64, crate::loader::Plan)>,
}

/// The UDP socket bmctl serves on (static buffers: create it once per boot).
pub fn socket() -> udp::Socket<'static> {
    udp::Socket::new(
        udp::PacketBuffer::new(unsafe { &mut *core::ptr::addr_of_mut!(RX_META) }[..].as_mut(), unsafe { &mut *core::ptr::addr_of_mut!(RX_BUF) }[..].as_mut()),
        udp::PacketBuffer::new(unsafe { &mut *core::ptr::addr_of_mut!(TX_META) }[..].as_mut(), unsafe { &mut *core::ptr::addr_of_mut!(TX_BUF) }[..].as_mut()),
    )
}

pub fn run(nic: Rtl, seed: [u8; 32], info: u64) {
    if rng::init(false).is_err() {
        return;
    }
    let mut rs = [0u8; 8];
    if rng::fill(&mut rs).is_err() {
        kprintln!("bmctl: entropy failed -> not starting");
        return;
    }
    let mac = nic.mac;
    let mut dev = Nic(nic);
    let mut cfg = Config::new(HardwareAddress::Ethernet(EthernetAddress(mac)));
    cfg.random_seed = u64::from_le_bytes(rs);
    let mut iface = Interface::new(cfg, &mut dev, now());
    let mut sockets = SocketSet::new(unsafe { &mut *core::ptr::addr_of_mut!(SOCKS) }[..].as_mut());
    let dhcp_h = sockets.add(dhcpv4::Socket::new());
    let udp_h = sockets.add(socket());
    let tcp_h = sockets.add(crate::ssh::tcp_socket());
    let t0 = time::now_ms();
    let mut ip = None;
    let link = dev.0.link_up();
    if !link {
        let _ = sockets.remove(dhcp_h); // no cable: no DHCP Discovers into a dead port
    }
    while link && ip.is_none() && time::now_ms() - t0 < 15_000 {
        wdt::pet();
        crate::net::rx_budget(Some(32));
        iface.poll(now(), &mut dev, &mut sockets);
        if let Some(dhcpv4::Event::Configured(c)) = sockets.get_mut::<dhcpv4::Socket>(dhcp_h).poll() {
            iface.update_ip_addrs(|a| {
                a.clear();
                let _ = a.push(IpCidr::Ipv4(c.address));
            });
            if let Some(gw) = c.router {
                let _ = iface.routes_mut().add_default_ipv4_route(gw);
            }
            ip = Some(c.address.address());
        }
    }
    // No wired lease (no cable, e.g. away from home): run the home screen anyway, offline. WiFi (W) is then the
    // network; bmctl cannot be reached, so its 10-minute idle reset is off.
    let offline = ip.is_none();
    if offline {
        kprintln!("bmctl: no wired lease (no cable, or none in 15 s) -> offline home screen (WiFi from the W key)");
    }
    let ip = ip.unwrap_or(Ipv4Address::UNSPECIFIED);
    let signkey = sunset::SignKey::Ed25519(sunset::ed25519_dalek::SigningKey::from_bytes(&seed));
    let lport = 49152 + (u16::from_le_bytes([rs[0], rs[1]]) % 16000);
    let Some(mut ctl) = Ctl::new(&dev.0, seed, info, ip, sockets.get_mut::<udp::Socket>(udp_h)) else { return };
    if offline {
        ctl.idle_ms = i64::MAX / 4;
    }
    crate::ssh::home(&mut ctl, &mut iface, &mut dev, &mut sockets, udp_h, tcp_h, &signkey, lport);
}

impl Ctl {
    /// Binds `sock` to the bmctl port. Needs `rng::init` done; None = entropy failure (printed).
    /// The wired address (unspecified when there was no DHCP lease: offline mode).
    pub fn wired_ip(&self) -> Ipv4Address {
        self.ip
    }

    pub fn new(nic: &Rtl, seed: [u8; 32], info: u64, ip: Ipv4Address, sock: &mut udp::Socket) -> Option<Ctl> {
        let idle_ms = crate::mb::cmdline(info).split(' ').find_map(|w| w.strip_prefix("idle=")).and_then(|v| v.parse::<i64>().ok())
            .filter(|v| (5..=3600).contains(v)).map_or(IDLE_MS, |v| v * 1000);
        crate::loader::snapshot_boot_info(info);
        let nic_mmio = match nic.bus {
            Bus::Mmio(b) => Some(b),
            Bus::Pio(_) => None,
        };
        let mut boot_id = [0u8; 16];
        if rng::fill(&mut boot_id).is_err() {
            kprintln!("bmctl: entropy failed -> not starting");
            return None;
        }
        let mut ck = [0u8; 32];
        if rng::fill(&mut ck).is_err() {
            kprintln!("bmctl: entropy failed -> not starting");
            return None;
        }
        let mut bid = heapless::String::<64>::new();
        hex(&boot_id, &mut bid);
        sock.bind(PORT).expect("bind");
        kprintln!("bmctl: listening on {}:{} boot id {} (idle reset after {} s)", ip, PORT, bid, idle_ms / 1000);
        crate::udplog::dump(5);
        console::status("ready");
        let t = time::now_ms();
        Some(Ctl {
            seed, key: derive_key(&seed), boot_id, bid, ip, idle_ms, nic_mmio, last_ok: t, last_hello: t - 2001, last_footer: t - 1000, battery: None, power: None, busypoll: crate::mb::cmdline(info).split(' ').any(|w| w == "busypoll"),
            ok: 0, bad: 0, replay: 0, retx: 0, ck, promoted: [(0, 0); 256], promoted_n: 0, who2_n: 0, who2_cyc: 0, who_window: 0, who_count: 0, reboot_at: None, sleep_at: None, stage: None, jump_at: None,
        })
    }

    /// Carries out an `Act`: reboot and chain-load never return; false = idle timeout, the caller must reset.
    pub fn act(&mut self, a: Act, nic: Option<&Rtl>) -> bool {
        match (a, nic) {
            (Act::Idle, _) => false,
            (Act::Reboot, _) => wdt::hard_reset(),
            (Act::Sleep(m), Some(n)) => {
                if let Err(e) = crate::sleep::enter(m, n) {
                    kprintln!("sleep: {}", e);
                    console::status("sleep failed (see log)");
                }
                true
            }
            (Act::Jump(p), Some(n)) => crate::loader::jump(&p, n),
            (_, None) => {
                kprintln!("bmctl: chain-load / sleep need the Ethernet NIC, not available on this link");
                true
            }
        }
    }

    /// Deadlines first, then at most one datagram. Only an authenticated command counts as activity (`last_ok`).
    pub fn step(&mut self, sock: &mut udp::Socket, t: i64) -> Option<Act> {
        crate::hda::poll(t);
        crate::iwl::poll();
        crate::wg::tick();
        if t - self.last_footer >= 1000 {
            self.last_footer = t;
            let mut r = heapless::String::<72>::new();
            let s = t / 1000;
            let _ = core::fmt::write(&mut r, format_args!("up {}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60));
            if s % 5 == 0 || self.battery.is_none() {
                self.battery = Some(crate::ec::battery());
            }
            if let Some(Some((pct, ac, chg))) = self.battery {
                let _ = core::fmt::write(&mut r, format_args!("   BAT {}%{}", pct, if chg { " charging" } else if ac { " AC" } else { "" }));
                if !ac && s % 5 == 0 || self.power.is_none() {
                    self.power = crate::ec::power_mw();
                }
                if let (false, Some(mw)) = (ac, self.power) {
                    let _ = core::fmt::write(&mut r, format_args!(" {}.{} W", mw / 1000, mw % 1000 / 100)); // the whole netbook, off AC
                }
            }
            if let Some((h, m)) = time::rtc_hm() {
                let _ = core::fmt::write(&mut r, format_args!("   {:02}:{:02}", h, m));
            }
            let mut ip = heapless::String::<16>::new();
            match crate::iwl::address() {
                Some((_, w)) => { let _ = core::fmt::write(&mut ip, format_args!("{}", w.address())); }
                None if self.ip.is_unspecified() => { let _ = ip.push_str("offline"); }
                None => { let _ = core::fmt::write(&mut ip, format_args!("{}", self.ip)); }
            }
            console::footer(ip.as_bytes(), r.as_bytes());
        }
        if t - self.last_ok > self.idle_ms {
            kprintln!("bmctl: idle {} s -> reset", self.idle_ms / 1000);
            console::status("no control traffic: restarting");
            return Some(Act::Idle);
        }
        if self.jump_at.as_ref().is_some_and(|(j, _)| t > *j) {
            let (_, p) = self.jump_at.take().unwrap();
            kprintln!("bmctl: chain-loading the staged kernel");
            console::status("loading new kernel");
            crate::udplog::dump(7);
            return Some(Act::Jump(p));
        }
        if let Some((at, m)) = self.sleep_at {
            if t > at {
                self.sleep_at = None;
                console::status("sleeping (wake: magic packet, power button, or the RTC alarm)");
                return Some(Act::Sleep(m));
            }
        }
        if self.reboot_at.is_some_and(|r| t > r) {
            kprintln!("bmctl: reboot requested");
            console::status("restarting");
            return Some(Act::Reboot);
        }
        if t - self.last_hello > 2000 {
            self.last_hello = t;
            let mut h = heapless::String::<128>::new();
            let _ = core::fmt::write(&mut h, format_args!("nc110 hello bmctl {}:{} boot {}", self.ip, PORT, self.bid));
            let _ = sock.send_slice(h.as_bytes(), IpEndpoint::new(IpAddress::Ipv4(Ipv4Address::BROADCAST), 5514));
        }
        if let Some(a) = self.serve(sock, t) {
            return Some(a);
        }
        // and bmctl over the WireGuard tunnel (skipped while a session holds the tunnel: it passes that socket in)
        crate::wg::with_bm(|s| self.serve(s, t)).flatten()
    }

    /// The datagram half of `step`, also run on the tunnel's bmctl socket (wg.rs) so home can reach the netbook at
    /// the venue (voice, control).
    pub fn serve(&mut self, sock: &mut udp::Socket, t: i64) -> Option<Act> {
        let mut buf = [0u8; 1500];
        // Drain up to 64 datagrams per step, so a flood cannot hold the real request back in the socket queue.
        for _ in 0..64 {
            let Ok((n, meta)) = sock.recv_slice(&mut buf) else { return None };
            if let Some(a) = self.datagram(sock, t, &mut buf, n, meta.endpoint) {
                return Some(a);
            }
        }
        None
    }

    fn datagram(&mut self, sock: &mut udp::Socket, t: i64, buf: &mut [u8; 1500], n: usize, from: IpEndpoint) -> Option<Act> {
        let sess = unsafe { &mut *core::ptr::addr_of_mut!(SESS) };
        let bucket = (t / 30_000) as u32;
        if n == 4 + 16 && &buf[..4] == b"who2" {
            if !sock.can_send() {
                return None; // TX queue full: no handshake work, keep the slots for authenticated replies
            }
            // Stateless: nothing is stored; the reply's session number is the cookie for this endpoint and bucket.
            let c0 = crate::port::rdtsc();
            let mut nc = [0u8; 16];
            nc.copy_from_slice(&buf[4..20]);
            let id = cookie(&self.ck, &from, bucket, &nc);
            let mut chk = heapless::String::<64>::new();
            hex(&who2_check(&self.key, &nc, &self.boot_id, id, bucket as u8)[..16], &mut chk);
            let mut h = heapless::String::<160>::new();
            let _ = core::fmt::write(&mut h, format_args!("nc110 v2 boot {} session {} bucket {} check {}", self.bid, id, bucket as u8, chk));
            let _ = sock.send_slice(h.as_bytes(), from);
            self.who2_n += 1;
            self.who2_cyc += crate::port::rdtsc().wrapping_sub(c0);
            return None;
        }
        if n == 3 + 16 && &buf[..3] == b"who" {
            if t - self.who_window > 1000 {
                (self.who_window, self.who_count) = (t, 0);
            }
            self.who_count += 1;
            if self.who_count <= 10 {
                // An old (v1) client: 7 fields so its parser reaches the length check and prints this field.
                let _ = sock.send_slice(b"nc110 boot v2-only:update-tools/bmctl.py session 0 check 0", from);
            }
            return None;
        }
        // Frame: [session u32 LE][seq u64 LE, bit 63 set][Nc 16][bucket u8][ct][tag]
        if n < 4 + 8 + 17 + 16 {
            self.bad += 1;
            return None;
        }
        let sid = u32::from_le_bytes(buf[..4].try_into().unwrap());
        let raw = u64::from_le_bytes(buf[4..12].try_into().unwrap());
        if raw >> 63 != 1 {
            self.bad += 1;
            return None;
        }
        let seq = raw & !(1 << 63);
        let mut nc = [0u8; 16];
        nc.copy_from_slice(&buf[12..28]);
        let b8 = buf[28];
        // Authenticated session, or a new one whose cookie recomputes for this endpoint (promoted below once the frame
        // authenticates; only then may it evict the least recently used authenticated session).
        let in_sess = sess.iter().position(|x| x.as_ref().is_some_and(|x| x.id == sid && x.peer == Some(from)));
        let fresh = in_sess.is_none()
            && [bucket, bucket.wrapping_sub(1)].iter().any(|&b| b as u8 == b8 && cookie(&self.ck, &from, b, &nc) == sid)
            && !self.promoted.iter().any(|&(s, b)| s == sid && b.wrapping_add(1) >= bucket && b != 0);
        let mut cand = None;
        let se = match in_sess {
            Some(i) => sess[i].as_mut().unwrap(),
            None if fresh => cand.insert(Sess { id: sid, key: session_key2(&self.key, &self.boot_id, sid, &nc), last_seq: 0, peer: Some(from), last_t: t, retx: 0, reply: [0; 8 + 1300 + 16], reply_len: 0 }),
            None => {
                self.bad += 1;
                return None;
            }
        };
        let (body, tg) = buf[29..n].split_at_mut(n - 29 - 16);
        if !open(&se.key, &nonce(b"C2K\0", seq), &self.boot_id, body, tg) {
            self.bad += 1;
            return None;
        }
        if seq == se.last_seq && se.reply_len > 0 {
            if se.retx >= 5 {
                return None; // cap resends of one reply
            }
            se.retx += 1;
            self.retx += 1; // retry of the last request: same bytes again (to the bound peer), nothing re-executed
            let _ = sock.send_slice(&se.reply[..se.reply_len], from);
            return None;
        }
        if seq <= se.last_seq {
            self.replay += 1;
            return None;
        }
        se.last_seq = seq;
        se.last_t = t;
        se.retx = 0;
        self.last_ok = t;
        let se = if in_sess.is_none() {
            let dst = (0..sess.len()).find(|&j| sess[j].is_none())
                .unwrap_or_else(|| (0..sess.len()).min_by_key(|&j| sess[j].as_ref().map_or(i64::MIN, |x| x.last_t)).unwrap());
            // Remember it for as long as its cookie can recompute (this bucket and the next). Fail closed: with no
            // expired slot left (256 promotions within a minute), refuse rather than forget a live one.
            let Some(j) = self.promoted.iter().position(|&(_, b)| b == 0 || b.wrapping_add(1) < bucket) else {
                self.bad += 1;
                return None;
            };
            self.promoted[j] = (sid, bucket.max(1));
            self.promoted_n += 1;
            sess[dst] = cand.take();
            sess[dst].as_mut().unwrap()
        } else {
            se
        };
        self.ok += 1;
        let mut out = [0u8; 1300];
        let len = {
            let mut w = Writer { b: &mut out, n: 0 };
            let loading = |c: &[u8]| c.starts_with(b"ldat") || c.starts_with(b"lbeg") || c.starts_with(b"lcom"); // incl. lcomlinux
            if self.jump_at.is_some() && loading(body) {
                // committed: the staged image is frozen until the hop (Codex loader MUST-FIX 2)
                let _ = core::fmt::write(&mut w, format_args!("err committed, jump pending"));
            } else if body.len() >= 8 && &body[..4] == b"ldat" {
                // binary chunk: "ldat" ++ u32 LE offset ++ bytes (checked before the text parser)
                let off = u32::from_le_bytes(body[4..8].try_into().unwrap()) as u64;
                let r = match self.stage.as_mut() {
                    Some(st) => crate::loader::data(st, off, &body[8..]),
                    None => Err("no lbeg"),
                };
                match r {
                    Ok(got) => { let _ = core::fmt::write(&mut w, format_args!("ok {}", got)); }
                    Err(e) => { let _ = core::fmt::write(&mut w, format_args!("err {}", e)); }
                }
            }
            let cmd = if w.n > 0 { "" } else { core::str::from_utf8(body).unwrap_or("") };
            let mut it = cmd.split(' ');
            match it.next().unwrap_or("") {
                "" if w.n > 0 => {} // already answered (ldat)
                "lbeg" => {
                    let size: u64 = it.next().and_then(|x| x.parse().ok()).unwrap_or(0);
                    match crate::loader::begin(size, it.next().unwrap_or("")) {
                        Ok(st) => {
                            self.stage = Some(st);
                            let _ = core::fmt::write(&mut w, format_args!("ok staging {} B at {:#x}", size, crate::loader::STAGE));
                        }
                        Err(e) => { let _ = core::fmt::write(&mut w, format_args!("err {}", e)); }
                    }
                }
                "lcomlinux" => {
                    let rest = cmd.splitn(2, ' ').nth(1).unwrap_or("");
                    let r = match self.stage.as_ref() {
                        Some(st) => crate::loader::prepare_linux(st, rest),
                        None => Err("no lbeg"),
                    };
                    match r {
                        Ok(p) => {
                            let _ = core::fmt::write(&mut w, format_args!("ok booting Linux in 300 ms"));
                            self.jump_at = Some((t + 300, p));
                        }
                        Err(e) => { let _ = core::fmt::write(&mut w, format_args!("err {}", e)); }
                    }
                }
                "lcom" => {
                    let rest = cmd.splitn(2, ' ').nth(1).unwrap_or("");
                    let r = match self.stage.as_ref() {
                        Some(st) => crate::loader::prepare(st, rest, &self.seed),
                        None => Err("no lbeg"),
                    };
                    match r {
                        Ok(p) => {
                            let _ = core::fmt::write(&mut w, format_args!("ok jumping in 300 ms"));
                            self.jump_at = Some((t + 300, p)); // the reply leaves first, then the hop
                        }
                        Err(e) => { let _ = core::fmt::write(&mut w, format_args!("err {}", e)); }
                    }
                }
                "ping" => {
                    let _ = core::fmt::write(&mut w, format_args!("pong uptime_ms={} ip={}", t, self.ip));
                }
                "pwr" => crate::pwr::cmd(&mut w, &mut it),
                "wifi" => crate::wifi::cmd(&mut w, &mut it),
                "wg" => crate::wg::cmd(&mut w, &mut it),
                "wpa" => {
                    let t = crate::time::now_ms();
                    let (ok, n) = crate::wpa::selftest();
                    let _ = core::fmt::write(&mut w, format_args!("wpa selftest {}/{} in {} ms", ok, n, crate::time::now_ms() - t));
                }
                "power" => match (crate::ec::battery(), crate::ec::power_mw()) {
                    (Some((pct, ac, chg)), Some(mw)) => {
                        let _ = core::fmt::write(&mut w, format_args!("power {} mW ({}) battery {}%", mw, if chg { "charging" } else if ac { "on AC: not the draw" } else { "discharging: whole-system draw" }, pct));
                    }
                    _ => { let _ = core::fmt::write(&mut w, format_args!("err no battery reading")); }
                },
                "stats" => {
                    let _ = core::fmt::write(&mut w, format_args!("ok={} bad={} replay={} retx={} sessions_issued={} log_bytes={} who2={} who2_cyc_avg={}", self.ok, self.bad, self.replay, self.retx, self.promoted_n, console::captured().len(), self.who2_n, self.who2_cyc / self.who2_n.max(1)));
                }
                "log" => {
                    let off: usize = it.next().and_then(|x| x.parse().ok()).unwrap_or(0);
                    let l = console::captured();
                    let st = off.min(l.len());
                    let s = &l[st..st.saturating_add(1200).min(l.len())];
                    w.put(s);
                }
                "peek" => {
                    let (what, x, y) = (it.next().unwrap_or(""), it.next().unwrap_or(""), it.next().unwrap_or(""));
                    if let Err(e) = peek(&mut w, self.nic_mmio, what, x, y) {
                        w.n = 0;
                        let _ = core::fmt::write(&mut w, format_args!("err {}", e));
                    }
                }
                "rng" => {
                    let k: usize = it.next().and_then(|x| x.parse().ok()).unwrap_or(0).min(1200);
                    let mut raw = [0u8; 1200];
                    rng::raw_bytes(&mut raw[..k]);
                    w.put(&raw[..k]);
                }
                "hdadump" => {
                    crate::hda::dump();
                    let _ = core::fmt::write(&mut w, format_args!("ok (see log)"));
                }
                "gma" => crate::gma::state(&mut w),
                "mouse" => {
                    let mut r = [0u8; 48];
                    let n = crate::kbd::raw_aux(&mut r);
                    for b in &r[..n] {
                        let _ = core::fmt::write(&mut w, format_args!("{:02x} ", b));
                    }
                }
                // cells <first>: up to 150 raw screen cells (u64 LE, term::Cell: CP437, attribute, exact fg/bg RGB), for
                // tools/nc110-shot.py
                "unmapped" => {
                    for (cp, n) in crate::term::unmapped() {
                        let _ = core::fmt::write(&mut w, format_args!("U+{:04X} {} ", cp, n));
                    }
                }
                "cells" => {
                    let first: usize = it.next().and_then(|x| x.parse().ok()).unwrap_or(0);
                    let n = console::cols() * console::rows();
                    for i in first..(first + 150).min(n) {
                        w.put(&console::cell_get64(i).to_le_bytes());
                    }
                }
                "screen" => match it.next().and_then(|x| x.parse::<usize>().ok()) {
                    None => {
                        let (fw, fh) = console::font_wh();
                        let _ = core::fmt::write(&mut w, format_args!("{}x{} font {}x{}", console::cols(), console::rows(), fw, fh));
                    }
                    Some(r) => {
                        let mut b = [0u8; 200];
                        let n = console::row_text(r, &mut b);
                        w.put(&b[..n]);
                    }
                },
                "usb" => match it.next() {
                    None => crate::usb::scan(&mut w),
                    Some(sub) => crate::usb::cmd(&mut w, &mut it, sub),
                },
                "battery" => match crate::ec::battery() {
                    Some((p, ac, c)) => { let _ = core::fmt::write(&mut w, format_args!("battery {}% ac {} charging {}", p, ac as u8, c as u8)); }
                    None => { let _ = core::fmt::write(&mut w, format_args!("battery none (or no EC answer)")); }
                },
                "acpi" => {
                    // acpi <SIG> <off>: raw table bytes (1200 per call); "acpi <SIG>" alone gives the length
                    let sig = it.next().unwrap_or("").as_bytes();
                    let off: Option<usize> = it.next().and_then(|x| x.parse().ok());
                    match (<[u8; 4]>::try_from(sig), off) {
                        (Ok(s4), Some(off)) => {
                            let mut b = [0u8; 1200];
                            if let Some((_, n)) = crate::acpi::read(&s4, off, &mut b) {
                                w.put(&b[..n]);
                            }
                        }
                        (Ok(s4), None) => match crate::acpi::table(&s4) {
                            Some((a, l)) => { let _ = core::fmt::write(&mut w, format_args!("{} at {:#x} length {}", sig.escape_ascii(), a, l)); }
                            None => { let _ = core::fmt::write(&mut w, format_args!("err no {}", sig.escape_ascii())); }
                        },
                        _ => { let _ = core::fmt::write(&mut w, format_args!("err acpi <SIG (4 chars)> [offset]")); }
                    }
                }
                "host" => {
                    let hs = crate::ssh::hosts();
                    match (it.next().unwrap_or("list"), it.next().and_then(|x| x.parse::<usize>().ok())) {
                        ("list", _) => {
                            for (i, h) in hs.iter().enumerate() {
                                if let Some(t) = h {
                                    let mut pin = heapless::String::<64>::new();
                                    hex(&t.pin[..8], &mut pin);
                                    let _ = core::fmt::write(&mut w, format_args!("{} {} {}:{} {} pin {}..\n", i, t.name, t.ip, t.port, t.user, pin));
                                }
                            }
                        }
                        ("del", Some(i)) if i < hs.len() => {
                            hs[i] = None;
                            let _ = core::fmt::write(&mut w, format_args!("ok deleted {}", i));
                        }
                        ("set", Some(i)) if i < hs.len() => match parse_host(&mut it) {
                            Ok(t) => {
                                hs[i] = Some(t);
                                let _ = core::fmt::write(&mut w, format_args!("ok set {}", i));
                            }
                            Err(e) => { let _ = core::fmt::write(&mut w, format_args!("err {}", e)); }
                        },
                        _ => { let _ = core::fmt::write(&mut w, format_args!("err host list | set <0-7> <name> <ip> <port> <user> <pin: 64 hex> | del <0-7>")); }
                    }
                }
                "key" => {
                    let rest = cmd.splitn(2, ' ').nth(1).unwrap_or("");
                    let n = crate::kbd::inject(rest.as_bytes());
                    let _ = core::fmt::write(&mut w, format_args!("ok injected {} of {} bytes", n, rest.len()));
                }
                "say" => {
                    console::status(cmd.splitn(2, ' ').nth(1).unwrap_or(""));
                    let _ = core::fmt::write(&mut w, format_args!("ok"));
                }
                "type" => {
                    let rest = cmd.splitn(2, ' ').nth(1).unwrap_or("");
                    let n = crate::hda::type_text(rest.as_bytes());
                    let _ = core::fmt::write(&mut w, format_args!("ok queued {} of {} bytes", n, rest.len()));
                }
                "rec" => {
                    let ms: i64 = it.next().and_then(|x| x.parse().ok()).unwrap_or(5000);
                    match crate::hda::rec_start(ms) {
                        Ok(id) => { let _ = core::fmt::write(&mut w, format_args!("ok recording clip {}", id)); }
                        Err(e) => { let _ = core::fmt::write(&mut w, format_args!("err {}", e)); }
                    }
                }
                "aud" => match it.next().and_then(|x| x.parse::<usize>().ok()) {
                    None => {
                        let (id, n, rec, peak, over) = crate::hda::status();
                        let (ctl, sts, lpib, gctl) = crate::hda::regs();
                        let _ = core::fmt::write(&mut w, format_args!("clip {} bytes {} recording {} peak {} fmt s16le 16000 mono sd_ctl {:#x} sd_sts {:#x} lpib {} gctl {:#x} overrun {}", id, n, rec as u8, peak, ctl, sts, lpib, gctl, over as u8));
                    }
                    Some(off) => {
                        let c = crate::hda::clip_bytes();
                        let st = off.min(c.len());
                        w.put(&c[st..st.saturating_add(1200).min(c.len())]);
                    }
                },
                "sleep" => {
                    // S3 did not wake on the real NC110 (2026-10-21 01:40, needed a power press; cause unknown): refused.
                    // sleep_at and src/sleep.rs stay for a future attempt, which must re-enable it here deliberately.
                    let _ = core::fmt::write(&mut w, format_args!("err sleep disabled: S3 hung the real netbook (see PROVISIONING.md)"));
                }
                "reboot" => {
                    let _ = core::fmt::write(&mut w, format_args!("rebooting in 500 ms"));
                    self.reboot_at = Some(t + 500);
                }
                _ => {
                    let _ = core::fmt::write(&mut w, format_args!("err unknown command (ping|stats|log <off>|peek ...|rng <n>|rec <ms>|aud [off]|type <text>|say <text>|key <text>|host list/set/del|gma|mouse|lbeg|ldat|lcom|lcomlinux|reboot)"));
                }
            }
            w.n
        };
        let pkt = &mut se.reply;
        pkt[..8].copy_from_slice(&seq.to_le_bytes());
        pkt[8..8 + len].copy_from_slice(&out[..len]);
        let tg = seal(&se.key, &nonce(b"K2C\0", seq), &self.boot_id, &mut pkt[8..8 + len]);
        pkt[8 + len..8 + len + 16].copy_from_slice(&tg);
        se.reply_len = 8 + len + 16;
        let _ = sock.send_slice(&se.reply[..se.reply_len], from);
        None
    }
}

struct Writer<'a> {
    b: &'a mut [u8],
    n: usize,
}
impl Writer<'_> {
    fn put(&mut self, s: &[u8]) {
        let k = s.len().min(self.b.len() - self.n);
        self.b[self.n..self.n + k].copy_from_slice(&s[..k]);
        self.n += k;
    }
}
impl core::fmt::Write for Writer<'_> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        self.put(s.as_bytes());
        Ok(())
    }
}

/// `host set` fields: name (<= 16), IPv4, port, user (<= 16), the host's ed25519 key as 64 hex digits (required: the
/// chooser never connects without a pin).
fn parse_host<'a>(it: &mut impl Iterator<Item = &'a str>) -> Result<crate::ssh::Target, &'static str> {
    let name = it.next().ok_or("name")?;
    let ip: Ipv4Address = it.next().and_then(|x| x.parse().ok()).ok_or("ip")?;
    let port: u16 = it.next().and_then(|x| x.parse().ok()).filter(|&p| p > 0).ok_or("port")?;
    let user = it.next().ok_or("user")?;
    let hexpin = it.next().filter(|x| x.len() == 64 && x.bytes().all(|c| c.is_ascii_hexdigit())).ok_or("pin must be 64 hex digits")?;
    let mut pin = [0u8; 32];
    for (i, b) in pin.iter_mut().enumerate() {
        *b = u8::from_str_radix(&hexpin[2 * i..2 * i + 2], 16).map_err(|_| "pin is not hex")?;
    }
    let mut t = crate::ssh::Target { name: heapless::String::new(), ip, port, user: heapless::String::new(), pin };
    t.name.push_str(name).map_err(|_| "name > 16")?;
    t.user.push_str(user).map_err(|_| "user > 16")?;
    Ok(t)
}

/// The boot config module's lines: `host <0-7> <name> <ip> <port> <user> <pin>` and `wg set ...` (bmctl syntax).
/// Results go to the log; the lines themselves are never printed (the wg line carries the private key).
pub fn boot_config(text: &str) {
    struct Log;
    impl core::fmt::Write for Log {
        fn write_str(&mut self, s: &str) -> core::fmt::Result {
            crate::kprint!("{}", s);
            Ok(())
        }
    }
    for (n, line) in text.lines().map(str::trim).enumerate() {
        let mut it = line.split_whitespace();
        match it.next() {
            None | Some("#") => continue,
            Some(c) if c.starts_with('#') => continue,
            Some("host") => match (it.next().and_then(|x| x.parse::<usize>().ok()), ()) {
                (Some(i), ()) if i < 8 => match parse_host(&mut it) {
                    Ok(t) => {
                        kprintln!("cfgmod: line {}: host {} = {}", n + 1, i, t.name);
                        crate::ssh::hosts()[i] = Some(t);
                    }
                    Err(e) => kprintln!("cfgmod: line {}: host: {}", n + 1, e),
                },
                _ => kprintln!("cfgmod: line {}: host index", n + 1),
            },
            Some("wg") => {
                crate::kprint!("cfgmod: line {}: wg: ", n + 1);
                crate::wg::cmd(&mut Log, &mut it);
                kprintln!("");
            }
            Some(_) => kprintln!("cfgmod: line {}: unknown", n + 1),
        }
    }
}
