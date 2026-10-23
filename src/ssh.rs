//! M2/M3: DHCP + TCP (smoltcp) + SSH client (Sunset) against a throwaway sshd on the host.
//!
//! Milestones are printed one per line so the furthest one reached is unambiguous:
//!   (a) TCP connect + server banner   (b) key exchange done, host key matched the pin
//!   (c) publickey auth OK             (d) channel output
//! M3: the session asks for a pty (xterm 80x25) and a shell; PS/2 keystrokes go to the channel and the
//! channel's output is drawn by the VGA terminal (`term`), and mirrored to COM1 with ESC shown as `^[`.
use crate::net::Nic;
use crate::nic::Rtl;
use crate::{bmctl, console, kbd, kprintln, rng, term, time, wdt};
use sha2::{Digest, Sha256};
use smoltcp::iface::{Config, Interface, SocketHandle, SocketSet, SocketStorage};
use smoltcp::socket::{dhcpv4, tcp, udp};
use smoltcp::time::Instant;
use smoltcp::wire::{EthernetAddress, HardwareAddress, IpAddress, IpCidr, Ipv4Address, Ipv4Cidr};
use sunset::{ChanData, ChanHandle, CliEvent, Event, Pty, PubKey, Runner, SignKey};

// QEMU slirp's host alias; the attended NC110 build talks to the throwaway Docker sshd on .179 instead.
const SERVER: (Ipv4Address, u16) = if cfg!(feature = "attended") {
    (Ipv4Address::new(192, 168, 50, 95), 2223)
} else {
    (Ipv4Address::new(10, 0, 2, 2), 2223)
};
const USER: &str = "evnchn";

/// The ONE host key we accept: raw ed25519 public key of the throwaway sshd's host key.
#[cfg(not(feature = "badpin"))]
const HOST_PIN: [u8; 32] = *include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/test-keys/host_pub.bin"));
/// Negative control: same pin with one bit flipped, must be rejected.
#[cfg(feature = "badpin")]
const HOST_PIN: [u8; 32] = {
    let mut k = *include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/test-keys/host_pub.bin"));
    k[0] ^= 0x01;
    k
};

/// An SSH destination: where, as whom, and the ONE ed25519 host key accepted there.
#[derive(Clone)]
pub struct Target {
    pub name: heapless::String<16>,
    pub ip: Ipv4Address,
    pub port: u16,
    pub user: heapless::String<16>,
    pub pin: [u8; 32],
}

/// The chooser's hosts (RAM only: the kernel never writes a disk). Slot 0 starts as the compiled-in placeholder
/// container; bmctl `host set/del` edits the table, and the agent re-pushes its list after every boot.
pub static mut HOSTS: [Option<Target>; 8] = [const { None }; 8];

fn placeholder() -> Target {
    let mut t = Target { name: heapless::String::new(), ip: SERVER.0, port: SERVER.1, user: heapless::String::new(), pin: HOST_PIN };
    let _ = t.name.push_str("placeholder");
    let _ = t.user.push_str(USER);
    t
}

pub fn hosts() -> &'static mut [Option<Target>; 8] {
    let h = unsafe { &mut *core::ptr::addr_of_mut!(HOSTS) };
    if h.iter().all(|x| x.is_none()) {
        h[0] = Some(placeholder());
    }
    h
}

static mut SOCK_STORE: [SocketStorage<'static>; 3] = [SocketStorage::EMPTY; 3];
static mut TCP_RX: [u8; 16384] = [0; 16384];
static mut TCP_TX: [u8; 16384] = [0; 16384];
static mut SSH_IN: [u8; 36000] = [0; 36000];
static mut SSH_OUT: [u8; 36000] = [0; 36000];

fn now() -> Instant {
    Instant::from_millis(time::now_ms())
}

fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// OpenSSH-style "SHA256:<base64, no padding>" of the ssh-ed25519 public key blob.
fn fingerprint(key: &[u8; 32], out: &mut [u8; 43]) {
    let mut h = Sha256::new();
    h.update(11u32.to_be_bytes());
    h.update(b"ssh-ed25519");
    h.update(32u32.to_be_bytes());
    h.update(key);
    let d = h.finalize();
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut bits: u32 = 0;
    let mut n = 0;
    let mut o = 0;
    for &b in d.iter() {
        bits = (bits << 8) | b as u32;
        n += 8;
        while n >= 6 {
            n -= 6;
            out[o] = A[((bits >> n) & 63) as usize];
            o += 1;
        }
    }
    if n > 0 {
        out[o] = A[((bits << (6 - n)) & 63) as usize];
    }
}

fn fp_str(key: &[u8; 32]) -> [u8; 43] {
    let mut f = [0u8; 43];
    fingerprint(key, &mut f);
    f
}

/// `client_seed`: the ed25519 seed loaded from the multiboot key module (never compiled in, M2.5).
/// `bm = Some(info)`: serve bmctl on the same interface for the whole session and after it (attended `bmctl ssh`).
/// The control channel then bounds the session instead of a wall clock: no valid bmctl command for the idle time
/// (default 600 s) ends it and returns, so the caller resets.
pub fn run(nic: Rtl, client_seed: Option<[u8; 32]>, rng_dump: bool, bm: Option<u64>) {
    let Some(mut client_seed) = client_seed else {
        kprintln!("ssh: no client key module (GRUB `module` / QEMU -initrd) -> refusing to run SSH");
        return;
    };
    if rng::init(rng_dump).is_err() {
        return; // rng::init already printed why
    }

    let mac = nic.mac;
    let mut dev = Nic(nic);
    let mut cfg = Config::new(HardwareAddress::Ethernet(EthernetAddress(mac)));
    let mut seed = [0u8; 8];
    if rng::fill(&mut seed).is_err() {
        kprintln!("rng: entropy pool failed for the network seed -> refusing to run SSH");
        return;
    }
    cfg.random_seed = u64::from_le_bytes(seed);
    let mut iface = Interface::new(cfg, &mut dev, now());

    let mut sockets = SocketSet::new(unsafe { &mut *core::ptr::addr_of_mut!(SOCK_STORE) }[..].as_mut());
    let dhcp_h = sockets.add(dhcpv4::Socket::new());
    let udp_h = sockets.add(bmctl::socket());
    let tcp_h = sockets.add(tcp_socket());

    // ---- DHCP (smoltcp) ----
    let t0 = time::now_ms();
    let mut have_ip = false;
    while time::now_ms() - t0 < 10_000 {
        iface.poll(now(), &mut dev, &mut sockets);
        if let Some(dhcpv4::Event::Configured(c)) = sockets.get_mut::<dhcpv4::Socket>(dhcp_h).poll() {
            iface.update_ip_addrs(|a| {
                a.clear();
                let _ = a.push(IpCidr::Ipv4(c.address));
            });
            if let Some(gw) = c.router {
                let _ = iface.routes_mut().add_default_ipv4_route(gw);
            }
            kprintln!("dhcp: smoltcp lease {} router {:?}", c.address, c.router);
            have_ip = true;
            break;
        }
    }
    if !have_ip {
        kprintln!("dhcp: no lease in 10 s -> static slirp fallback 10.0.2.15/24 gw 10.0.2.2");
        iface.update_ip_addrs(|a| {
            let _ = a.push(IpCidr::Ipv4(Ipv4Cidr::new(Ipv4Address::new(10, 0, 2, 15), 24)));
        });
        let _ = iface.routes_mut().add_default_ipv4_route(Ipv4Address::new(10, 0, 2, 2));
    }

    let mut ctl = match bm {
        Some(info) => {
            let ip = iface.ipv4_addr().unwrap_or(Ipv4Address::new(10, 0, 2, 15));
            match bmctl::Ctl::new(&dev.0, client_seed, info, ip, sockets.get_mut::<udp::Socket>(udp_h)) {
                Some(c) => Some(c),
                None => return,
            }
        }
        None => None,
    };
    let lport = 49152 + (u16::from_le_bytes([seed[0], seed[1]]) % 16000);
    let signkey = SignKey::Ed25519(sunset::ed25519_dalek::SigningKey::from_bytes(&client_seed));
    for b in client_seed.iter_mut() {
        unsafe { core::ptr::write_volatile(b, 0) }; // our copy; the SigningKey keeps its own
    }
    let (bm_idle, _) = session(&mut iface, &mut dev, &mut sockets, tcp_h, udp_h, ctl.as_mut(), &signkey, lport, &placeholder());
    if let (Some(c), false) = (ctl.as_mut(), bm_idle) {
        home(c, &mut iface, &mut dev, &mut sockets, udp_h, tcp_h, &signkey, lport);
    }
}

/// One SSH session to `t` on an interface that is already up. `ctl`: bmctl served every pass (and its idle bound
/// replaces the attended 180 s cap). Returns (bmctl idle fired: the caller must reset, the shell produced output).
pub fn session<D: crate::net::Link>(iface: &mut Interface, dev: &mut D, sockets: &mut SocketSet, tcp_h: SocketHandle, udp_h: SocketHandle,
               mut ctl: Option<&mut bmctl::Ctl>, signkey: &SignKey, lport: u16, t: &Target) -> (bool, bool) {
    let mut bm_idle = false;
    let tcp = sockets.get_mut::<tcp::Socket>(tcp_h);
    tcp.abort(); // a previous session's socket

    // ---- TCP connect ----
    sockets
        .get_mut::<tcp::Socket>(tcp_h)
        .connect(iface.context(), (IpAddress::Ipv4(t.ip), t.port), lport)
        .expect("tcp connect");
    // Peer liveness: keepalive every 15 s; unacknowledged data or keepalives abort the socket after 60 s.
    sockets.get_mut::<tcp::Socket>(tcp_h).set_keep_alive(Some(smoltcp::time::Duration::from_secs(15)));
    sockets.get_mut::<tcp::Socket>(tcp_h).set_timeout(Some(smoltcp::time::Duration::from_secs(60)));
    kprintln!("tcp: connecting {} {}:{} as {} from :{}", t.name, t.ip, t.port, t.user, lport);

    let mut runner = Runner::new_client(unsafe { &mut *core::ptr::addr_of_mut!(SSH_IN) }, unsafe {
        &mut *core::ptr::addr_of_mut!(SSH_OUT)
    });

    let mut connected = false;
    let mut banner_shown = false;
    let mut banner = [0u8; 128];
    let mut banner_len = 0usize;
    let mut pubkey_tries = 0;
    let mut chan: Option<ChanHandle> = None;
    let mut kex_done = false;
    let mut got_output = false;
    let (mut out_bytes, mut out_t0) = (0u64, 0i64);
    let mut finished = false;
    let mut shell_open = false;
    let mut winch = false; // window-change (the screen's grid) still to send, once the session is up
    let mut keys = [0u8; 64];
    let mut nkeys = 0usize;
    let (mut line_start, mut tilde) = (true, false); // Enter ~ . hangs up (as in OpenSSH)
    let mut last_pet = time::now_ms();
    // 60 s to get as far as the shell's first output (pty/shell requests are sent want_reply=false, so output is
    // the first sign the shell really started); after that the session lasts until it closes or the peer dies.
    let deadline = time::now_ms() + 60_000;
    // Attended (unattended overnight) runs: an absolute bound, so a remote shell that hangs after its first
    // output cannot keep this loop petting the watchdog forever (Codex night review MUST-FIX 2).
    // `bmctl ssh` drops it: bmctl's idle bound (no fresh authenticated command) plays that role instead.
    let hard_end = time::now_ms() + 180_000;

    'outer: while (got_output || time::now_ms() < deadline) && (!cfg!(feature = "attended") || ctl.is_some() || crate::iwl::online() || time::now_ms() < hard_end) {
        if wdt::armed() && time::now_ms() - last_pet >= 1000 {
            wdt::pet(); // the main loop is the liveness signal: a hang anywhere in it stops the petting
            last_pet = time::now_ms();
        }
        if !shell_open {
            // still connecting: the keyboard can only cancel (Esc)
            let (mut k, mut n) = ([0u8; 16], 0usize);
            if kbd::poll(&mut k, &mut n) {
                wdt::hard_reset();
            }
            if k[..n].contains(&0x1b) {
                kprintln!("ssh: cancelled with Esc while connecting");
                unsafe { WHY = "cancelled" };
                break 'outer;
            }
        }
        if ctl.is_some() {
            crate::net::rx_budget(Some(64)); // bmctl's deadlines run every pass even under a flood
        }
        let polled = iface.poll(now(), dev, sockets);
        if let Some(c) = ctl.as_mut() {
            if let Some(a) = c.step(sockets.get_mut::<udp::Socket>(udp_h), time::now_ms()) {
                if !c.act(a, dev.rtl()) {
                    bm_idle = true;
                    break 'outer;
                }
            }
        }
        let sock = sockets.get_mut::<tcp::Socket>(tcp_h);
        if !connected {
            if sock.may_send() {
                connected = true;
                kprintln!("tcp: connected to {}:{}", t.ip, t.port);
            } else if !sock.is_open() {
                kprintln!("tcp: connection failed");
                break;
            }
        }

        // network -> runner
        if sock.can_recv() && runner.is_input_ready() {
            let r = sock.recv(|buf| {
                if !banner_shown {
                    for &b in buf.iter() {
                        if b == b'\n' {
                            banner_shown = true;
                            break;
                        }
                        if b != b'\r' && banner_len < banner.len() {
                            banner[banner_len] = b;
                            banner_len += 1;
                        }
                    }
                }
                match runner.input(buf) {
                    Ok(n) => (n, Ok(())),
                    Err(e) => (0, Err(e)),
                }
            });
            if banner_shown && banner_len > 0 {
                kprintln!("ssh(a): server banner: {}", core::str::from_utf8(&banner[..banner_len]).unwrap_or("?"));
                banner_len = 0;
            }
            if let Ok(Err(e)) = r {
                kprintln!("ssh: input error {:?}", e);
                break;
            }
        }
        // Peer sent FIN (CloseWait) or the socket died (reset/timeout): no more input once the buffer is drained.
        if connected && !sock.may_recv() && !sock.can_recv() {
            runner.close_input();
            if !sock.is_open() {
                kprintln!("\r\nssh: TCP connection gone ({:?}), ending session", sock.state());
                break 'outer;
            }
        }

        // runner events
        loop {
            let mut open_session = false;
            let mut stop_all = false;
            let mut idle = false;
            {
            let ev = match runner.progress() {
                Ok(ev) => ev,
                Err(e) => {
                    kprintln!("ssh: error {:?}", e);
                    break 'outer;
                }
            };
            match ev {
                Event::Cli(CliEvent::Hostkey(h)) => {
                    let pk = match h.hostkey() {
                        Ok(PubKey::Ed25519(k)) => Some(k.key.0),
                        _ => None,
                    };
                    match pk {
                        Some(k) if ct_eq(&k, &t.pin) => {
                            let f = fp_str(&k);
                            kprintln!(
                                "ssh: host key ssh-ed25519 SHA256:{} matches pin",
                                core::str::from_utf8(&f).unwrap_or("?")
                            );
                            if let Err(e) = h.accept() {
                                kprintln!("ssh: host key accept failed (exchange signature?) {:?}", e);
                                stop_all = true;
                            }
                        }
                        Some(k) => {
                            let f = fp_str(&k);
                            let p = fp_str(&t.pin);
                            kprintln!(
                                "ssh: hostkey MISMATCH: got SHA256:{} pinned SHA256:{} -> rejecting",
                                core::str::from_utf8(&f).unwrap_or("?"),
                                core::str::from_utf8(&p).unwrap_or("?")
                            );
                            let _ = h.reject();
                            stop_all = true;
                            unsafe { WHY = "host key does not match its pin" };
                        }
                        None => {
                            kprintln!("ssh: hostkey MISMATCH: not ssh-ed25519 -> rejecting");
                            let _ = h.reject();
                            stop_all = true;
                        }
                    }
                }
                Event::Cli(CliEvent::Banner(b)) => {
                    kprintln!("ssh: userauth banner: {}", b.banner().unwrap_or("?"));
                }
                Event::Cli(CliEvent::Username(u)) => {
                    if !kex_done {
                        kex_done = true;
                        kprintln!("ssh(b): key exchange done (curve25519-sha256, host sig verified, chacha20-poly1305)");
                    }
                    let _ = u.username(t.user.as_str());
                }
                Event::Cli(CliEvent::Password(p)) => {
                    let _ = p.skip();
                }
                Event::Cli(CliEvent::Pubkey(p)) => {
                    if pubkey_tries == 0 {
                        pubkey_tries += 1;
                        kprintln!("ssh: offering publickey ssh-ed25519 (throwaway test key)");
                        let _ = p.pubkey(signkey.clone());
                    } else {
                        // asked again after our key: the host refused it
                        unsafe { WHY = "host refused this netbook's key" };
                        let _ = p.skip();
                    }
                }
                Event::Cli(CliEvent::AgentSign(s)) => {
                    let _ = s.skip();
                }
                Event::Cli(CliEvent::Authenticated) => {
                    kprintln!("ssh(c): publickey auth OK as {}", t.user);
                    open_session = true;
                }
                Event::Cli(CliEvent::SessionOpened(mut o)) => {
                    let mut tn = heapless::String::new();
                    let _ = tn.push_str("xterm"); // xterm: its terminfo advertises the subset term.rs implements (vt100 has no colour, il/dl, ich/dch or alt screen)
                    let (pc, pr) = (console::cols() as u32, console::rows() as u32);
                    let pty = Pty { term: tn, cols: pc, rows: pr, width: 0, height: 0, modes: heapless::Vec::new() };
                    let rp = o.pty(pty);
                    let rs = o.shell();
                    kprintln!("ssh: session channel open, pty-req xterm {}x{} ({:?}), shell ({:?})", pc, pr, rp.is_ok(), rs.is_ok());
                    if rp.is_err() || rs.is_err() {
                        stop_all = true;
                    } else {
                        shell_open = true;
                        winch = true;
                        kbd::init();
                        console::vga_log_off();
                        term::init();
                        kprintln!("term: VGA now belongs to the remote shell; keyboard is live");
                    }
                }
                Event::Cli(CliEvent::SessionExit(x)) => {
                    kprintln!("ssh: remote exit {:?}", x);
                }
                Event::Cli(CliEvent::Defunct) => {
                    kprintln!("ssh: connection closed");
                    finished = true;
                    idle = true;
                }
                Event::Cli(CliEvent::PollAgain) | Event::Progressed => {}
                Event::None => idle = true,
                _ => {}
            }
            }
            if stop_all {
                break 'outer;
            }
            if idle && !open_session {
                break;
            }
            if open_session {
                match runner.open_client_session() {
                    Ok(c) => chan = Some(c),
                    Err(e) => {
                        kprintln!("ssh: open session failed {:?}", e);
                        break 'outer;
                    }
                }
            }
        }

        // channel data -> VGA terminal (+ serial mirror)
        if let Some(c) = &chan {
            if winch {
                winch = false;
                let w = sunset::packets::WinChange { cols: console::cols() as u32, rows: console::rows() as u32, width: 0, height: 0 };
                kprintln!("ssh: window-change {}x{} ({:?})", w.cols, w.rows, runner.term_window_change(c, &w).is_ok());
            }
            let mut drew = false;
            while let Some((_num, dt, _len)) = runner.read_channel_ready() {
                let mut buf = [0u8; 512];
                match runner.read_channel(c, dt, &mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        if !got_output {
                            kprintln!("ssh(d): first shell output ({} bytes)", n);
                            got_output = true;
                            out_t0 = time::now_ms();
                        }
                        out_bytes += n as u64;
                        for &b in &buf[..n] {
                            term::feed(b);
                            if b == 0x1B {
                                console::serial_raw(b'^');
                                console::serial_raw(b'[');
                            } else {
                                console::serial_raw(b);
                            }
                        }
                        drew = true;
                    }
                    Err(_) => break,
                }
            }
            if drew {
                term::flush();
            }
            // terminal replies (DSR/DA) go out ahead of keys typed later; they share the keyboard queue
            nkeys += term::take_reply(&mut keys[nkeys..]);

            // keyboard -> channel (whatever does not fit now stays queued; the i8042 holds the rest)
            if shell_open && !finished {
                let n0 = nkeys;
                if kbd::poll(&mut keys, &mut nkeys) {
                    kprintln!("\r\nkbd: Ctrl-Alt-Del -> hard reset");
                    wdt::hard_reset();
                }
                for &b in &keys[n0..nkeys] {
                    if tilde && b == b'.' {
                        kprintln!("\r\nssh: Enter ~ . -> hang up");
                        unsafe { WHY = "hung up (Enter ~ .)" };
                        break 'outer;
                    }
                    tilde = line_start && b == b'~';
                    line_start = b == b'\r' || b == b'\n';
                }
                if kbd::take_ptt() {
                    let _ = crate::hda::ptt_toggle();
                }
                if kbd::take_size() && console::is_fb() {
                    // F11: next text size; the terminal re-lays out and the remote side is told (SIGWINCH redraw)
                    if let Some((cw, ch)) = console::next_font() {
                        kprintln!("\r\nssh: F11 -> text size {}x{}", cw, ch);
                        term::regrid();
                        winch = true;
                    }
                }
                nkeys += crate::hda::take_typed(&mut keys[nkeys..]); // voice transcript, typed (no Enter)
                let _ = mouse_step(Some((&mut keys, &mut nkeys))); // one queued event per pass
                if nkeys > 0 {
                    match runner.write_channel(c, ChanData::Normal, &keys[..nkeys]) {
                        Ok(n) => {
                            keys.copy_within(n..nkeys, 0);
                            nkeys -= n;
                        }
                        Err(_) => nkeys = 0, // channel gone; the close check below ends the session
                    }
                }
            }

            if runner.is_channel_eof(c) || runner.is_channel_closed(c) {
                if !finished {
                    finished = true;
                    kprintln!("\r\nssh: shell channel closed, disconnecting ({} shell bytes in {} ms)", out_bytes, time::now_ms() - out_t0);
                    runner.close_output();
                }
            }
        }

        // runner -> network
        let sock = sockets.get_mut::<tcp::Socket>(tcp_h);
        while sock.can_send() {
            let out = runner.output_buf();
            if out.is_empty() {
                break;
            }
            match sock.send_slice(out) {
                Ok(0) | Err(_) => break,
                Ok(n) => runner.consume_output(n),
            }
        }
        if !finished && nkeys == 0 && matches!(polled, smoltcp::iface::PollResult::None) && runner.output_buf().is_empty() {
            crate::idle::nap(1); // quiet pass: halt until the next 1 ms tick (no-op unless the home screen set naps up)
        }
        if finished {
            sock.close();
            for _ in 0..50 {
                iface.poll(now(), dev, sockets);
            }
            break;
        }
    }
    kprintln!("ssh: session loop ended: {} shell bytes in {} ms, finished={}", out_bytes, if got_output { time::now_ms() - out_t0 } else { 0 }, finished);
    let (calls, reseeds, samples, failed) = rng::stats();
    kprintln!("kbd: {} keys dropped while the channel was not taking input", kbd::dropped());
    let rc = term::reply_counts();
    kprintln!("term: replies sent: DSR 6n x{}, DSR 5n x{}, DA1 x{}, DA2 x{}; {} dropped (reply buffer full)", rc[0], rc[1], rc[2], rc[3], rc[4]);
    kprintln!("rng: {} requests served, {} reseeds, {} raw samples health-tested, failure latched: {}", calls, reseeds, samples, failed);
    if got_output {
        kprintln!("ssh: M3 reached: interactive shell over SSH, drawn on VGA, typed on PS/2");
    } else {
        kprintln!("ssh: stopped before (d)");
    }
    (bm_idle, got_output)
}

/// The SSH TCP socket (static buffers: create it once per boot).
pub fn tcp_socket() -> tcp::Socket<'static> {
    tcp::Socket::new(
        tcp::SocketBuffer::new(unsafe { &mut *core::ptr::addr_of_mut!(TCP_RX) }[..].as_mut()),
        tcp::SocketBuffer::new(unsafe { &mut *core::ptr::addr_of_mut!(TCP_TX) }[..].as_mut()),
    )
}

/// The agent OS's resting state: bmctl served every pass, the splash with a key menu. S opens an SSH session (bmctl
/// keeps running inside it), L toggles the log view, R restarts, Ctrl-Alt-Del resets. Returns only on bmctl's idle
/// bound (the caller resets); reboot and chain-load never return.
pub fn home(ctl: &mut bmctl::Ctl, iface: &mut Interface, dev: &mut Nic, sockets: &mut SocketSet, udp_h: SocketHandle,
            tcp_h: SocketHandle, signkey: &SignKey, lport: u16) {
    kbd::init();
    console::splash();
    console::status("ready");
    console::menu(if console::is_fb() { MENU_FB } else { MENU_TEXT });
    kprintln!("home: menu up, bmctl serving");
    unsafe {
        if HOME_AT_MS == 0 {
            HOME_AT_MS = time::since_reset_ms();
        }
    }
    let (mut log_view, mut n_sess) = (false, 0u16);
    let mut chooser: Option<usize> = None; // the host chooser is open, with this row selected
    let (mut last_input, mut blanked) = (time::now_ms(), false); // panel off after 5 min without a key or touch
    let mut attract = false; // the graphics screen was opened by the idle timer: it cycles effects on its own
    let napping = !ctl.busypoll && crate::idle::init(); // `busypoll`: the old spin loop, for A/B power runs
    loop {
        wdt::pet();
        crate::net::rx_budget(Some(32)); // bounded ingress per poll so bmctl's deadline checks always run
        let polled = iface.poll(now(), dev, sockets);
        if let Some(a) = ctl.step(sockets.get_mut::<udp::Socket>(udp_h), time::now_ms()) {
            if !ctl.act(a, Some(&dev.0)) {
                return;
            }
        }
        let (mut k, mut n) = ([0u8; 16], 0usize);
        if kbd::poll(&mut k, &mut n) {
            kprintln!("kbd: Ctrl-Alt-Del -> hard reset");
            wdt::hard_reset();
        }
        let p0 = unsafe { PTR };
        if let Some(c) = mouse_step(None) {
            if c == b'F' {
                if !log_view {
                    console::status(crate::hda::ptt_toggle()); // the F12 chip
                }
            } else if n < k.len() {
                k[n] = c;
                n += 1;
            }
        }
        if kbd::take_ptt() && !log_view {
            console::status(crate::hda::ptt_toggle());
        }
        if n > 0 || unsafe { PTR } != p0 {
            last_input = time::now_ms();
            if blanked {
                blanked = false;
                let _ = crate::gma::panel(true);
                kprintln!("home: input -> panel on");
                n = 0; // the waking key or click only wakes
            }
        } else if !blanked && !log_view && chooser.is_none() && console::is_fb() && time::now_ms() - last_input > 120_000 {
            // attract mode (LinuxJam booth): an idle home screen opens the live show-and-tell screen by itself
            kprintln!("home: 2 min without input -> graphics");
            last_input = time::now_ms();
            k[0] = b'g';
            n = 1;
            attract = true;
        } else if !blanked && console::is_fb() && time::now_ms() - last_input > 300_000 {
            blanked = crate::gma::panel(false).is_some(); // the panel is ~0.7 W of the 7.4 W
            kprintln!("home: 5 min without input -> panel off ({})", blanked);
            if !blanked {
                last_input = time::now_ms(); // no GMA (VGA text, q35): try again in 5 min, not every pass
            }
        }
        let mut heard = [0u8; 76];
        let hn = crate::hda::take_typed(&mut heard[6..]);
        if hn > 0 && !log_view {
            heard[..6].copy_from_slice(b"heard ");
            console::status(core::str::from_utf8(&heard[..6 + hn]).unwrap_or("heard (not UTF-8)"));
        }
        if napping && n == 0 && matches!(polled, smoltcp::iface::PollResult::None) {
            crate::idle::nap(2); // nothing arrived: sleep until the next 2 ms tick (or any interrupt)
        }
        let slots: heapless::Vec<usize, 8> = hosts().iter().enumerate().filter(|(_, h)| h.is_some()).map(|(i, _)| i).collect();
        let mut connect: Option<usize> = None; // index into `slots`
        let mut i = 0;
        while i < n {
            let b = k[i];
            i += 1;
            if let Some(sel) = chooser {
                if b == 0x1b && i + 1 < n + 1 && i < n && (k[i] == b'[' || k[i] == b'O') && i + 1 < n {
                    let dir = k[i + 1];
                    i += 2;
                    let sel = match dir {
                        b'A' => sel.saturating_sub(1),
                        b'B' => (sel + 1).min(slots.len().saturating_sub(1)),
                        _ => sel,
                    };
                    chooser = Some(sel);
                    draw_chooser(&slots, sel);
                    continue;
                }
                match b {
                    b'1'..=b'8' if ((b - b'1') as usize) < slots.len() => connect = Some((b - b'1') as usize),
                    b'\r' | b'\n' => connect = Some(sel),
                    0x1b | b'q' | b'Q' => {
                        chooser = None;
                        console::splash();
                        console::status("ready");
                        console::menu(if console::is_fb() { MENU_FB } else { MENU_TEXT });
                    }
                    _ => {}
                }
                continue;
            }
            match (b | 0x20, log_view) {
                (b's', false) if slots.len() == 1 => connect = Some(0),
                (b's', false) => {
                    chooser = Some(0);
                    draw_chooser(&slots, 0);
                }
                (b'l', _) => {
                    log_view = !log_view;
                    if log_view {
                        console::show_log();
                    } else {
                        console::splash();
                        console::status("ready");
                        console::menu(if console::is_fb() { MENU_FB } else { MENU_TEXT });
                    }
                }
                (b'z', false) => {
                    if let Some((c, r)) = console::next_font() {
                        kprintln!("home: Z -> text size {}x{}", c, r);
                        console::wipe(); // the big title of the old size is pixels, not cells
                        console::splash();
                        console::status("text size changed (Z again for the next)");
                        console::menu(if console::is_fb() { MENU_FB } else { MENU_TEXT });
                    }
                }
                (b'w', false) => {
                    kprintln!("home: W -> WiFi");
                    if !wifi_screen(ctl, iface, dev, sockets, udp_h) {
                        return;
                    }
                    console::splash();
                    console::status(&wifi_status());
                    console::menu(if console::is_fb() { MENU_FB } else { MENU_TEXT });
                }
                (b't', false) => {
                    kprintln!("home: T -> try it");
                    let font = console::big_font(); // laid out for 12x24
                    let ok = try_screen(ctl, iface, dev, sockets, udp_h);
                    console::wipe();
                    console::restore_font(font);
                    if !ok {
                        return;
                    }
                    last_input = time::now_ms();
                    console::wipe(); // big numbers / QR are pixels, not cells
                    console::splash();
                    console::status(&wifi_status());
                    console::menu(if console::is_fb() { MENU_FB } else { MENU_TEXT });
                }
                (b'g', false) => {
                    kprintln!("home: G -> graphics");
                    let font = console::big_font(); // laid out for 12x24
                    let ok = gfx_screen(ctl, iface, dev, sockets, udp_h, attract);
                    console::wipe();
                    console::restore_font(font);
                    if !ok {
                        return;
                    }
                    attract = false;
                    last_input = time::now_ms();
                    console::splash();
                    console::status(&wifi_status());
                    console::menu(if console::is_fb() { MENU_FB } else { MENU_TEXT });
                }
                (b'd', false) => {
                    kprintln!("home: D -> show and tell");
                    let font = console::big_font(); // laid out for 12x24
                    let ok = demo(ctl, iface, dev, sockets, udp_h);
                    console::wipe();
                    console::restore_font(font);
                    if !ok {
                        return;
                    }
                    last_input = time::now_ms();
                    console::wipe(); // big numbers / QR are pixels, not cells
                    console::splash();
                    console::status(&wifi_status());
                    console::menu(if console::is_fb() { MENU_FB } else { MENU_TEXT });
                }
                (b'r', false) => {
                    kprintln!("home: R -> restart");
                    console::status("restarting");
                    wdt::hard_reset();
                }
                _ => {}
            }
        }
        if let Some(t) = connect.and_then(|c| slots.get(c)).and_then(|&s| hosts()[s].clone()) {
            chooser = None;
            console::splash();
            let mut st = heapless::String::<64>::new();
            let _ = core::fmt::write(&mut st, format_args!("connecting to {} ({}@{}:{})", t.name, t.user, t.ip, t.port));
            console::status(&st);
            kprintln!("home: SSH session -> {}", t.name);
            n_sess = n_sess.wrapping_add(1);
            unsafe { WHY = "host did not answer" };
            let wired = !ctl.wired_ip().is_unspecified();
            let (idle, shown) = if crate::wg::routes(t.ip) && !(wired && wired_reaches(ctl.wired_ip(), t.ip)) {
                kprintln!("home: session through the WireGuard tunnel (bmctl served over it)");
                let bh = crate::wg::bm_handle();
                crate::wg::with_net(|wi, wd, ws, th| match bh {
                    Some(bh) => session(wi, wd, ws, th, bh, Some(&mut *ctl), signkey, lport.wrapping_add(n_sess), &t),
                    None => session(wi, wd, ws, th, th, None, signkey, lport.wrapping_add(n_sess), &t),
                }).unwrap_or((false, false))
            } else if crate::iwl::online() && !(wired && wired_reaches(ctl.wired_ip(), t.ip)) {
                // over WiFi: its own interface and TCP socket; bmctl (Ethernet) pauses for the session
                kprintln!("home: session over WiFi");
                crate::iwl::with_net(|wi, wd, ws, th| session(wi, wd, ws, th, th, None, signkey, lport.wrapping_add(n_sess), &t)).unwrap_or((false, false))
            } else {
                session(iface, dev, sockets, tcp_h, udp_h, Some(&mut *ctl), signkey, lport.wrapping_add(n_sess), &t)
            };
            if idle {
                return;
            }
            console::splash();
            st.clear();
            let why = unsafe { WHY };
            let _ = core::fmt::write(&mut st, format_args!("{}: {}", t.name, if why.starts_with("hung") || !shown { why } else { "session ended" }));
            console::status(&st);
            console::menu(if console::is_fb() { MENU_FB } else { MENU_TEXT });
        }
    }
}

/// The host chooser on the splash: one row per host in `slots`, `sel` highlighted.
fn draw_chooser(slots: &[usize], sel: usize) {
    let mut lines: heapless::Vec<heapless::String<64>, 8> = heapless::Vec::new();
    for (n, &s) in slots.iter().enumerate() {
        if let Some(t) = &hosts()[s] {
            let mut l = heapless::String::<64>::new();
            let _ = core::fmt::write(&mut l, format_args!(" {}  {:<12} {}@{}:{} ", n + 1, t.name, t.user, t.ip, t.port));
            let _ = lines.push(l);
        }
    }
    let refs: heapless::Vec<&str, 8> = lines.iter().map(|l| l.as_str()).collect();
    console::chooser(&refs, sel);
}

// Mouse pointer in sub-cell units (MX per column, MY per row): touchpad counts -> cells. Tune here if it feels slow/fast.
const MX: i32 = 6;
const MY: i32 = 12;
static mut PTR: (i32, i32, u8) = (40 * MX, 12 * MY, 0); // x, y (sub-cell units), buttons held

/// One mouse event. Moves the pointer; in an SSH session with the remote's mouse tracking on (?1000/1002/1003),
/// appends xterm mouse reports (SGR ?1006 or legacy X10 bytes) to `keys`; at home, returns the menu key under a left
/// click. `keys` = None at home.
fn mouse_step(keys: Option<(&mut [u8], &mut usize)>) -> Option<u8> {
    let (dx, dy, wheel, btn) = kbd::take_mouse()?;
    let (x0, y0, old) = unsafe { PTR };
    let (cw, ch) = (console::cols() as i32, console::rows() as i32);
    let (x, y) = ((x0 + dx).clamp(0, cw * MX - 1), (y0 - dy).clamp(0, ch * MY - 1));
    unsafe { PTR = (x, y, btn) };
    let (col, row) = ((x / MX) as usize, (y / MY) as usize);
    let moved = (col, row) != ((x0 / MX) as usize, (y0 / MY) as usize);
    console::pointer(Some(row * cw as usize + col));
    let Some((buf, n)) = keys else {
        // act on the release, so no button is still held when a session starts
        return if btn & 1 == 0 && old & 1 != 0 { console::menu_hit(col, row) } else { None };
    };
    let (mode, sgr) = term::mouse();
    if mode == 0 {
        return None;
    }
    let mut emit = |b: u8, press: bool| {
        let mut s = heapless::Vec::<u8, 24>::new();
        if sgr {
            let mut t = heapless::String::<24>::new();
            let _ = core::fmt::write(&mut t, format_args!("\x1b[<{};{};{}{}", b, col + 1, row + 1, if press { 'M' } else { 'm' }));
            let _ = s.extend_from_slice(t.as_bytes());
        } else {
            // X10: raw bytes 32 + value; columns past 95 are bytes >= 0x80, not UTF-8 (Codex review)
            let code = if press { b } else { 3 };
            let _ = s.extend_from_slice(&[0x1b, b'[', b'M', 32 + code, 32 + 1 + col.min(222) as u8, 32 + 1 + row.min(222) as u8]);
        }
        if buf.len() - *n >= s.len() {
            buf[*n..*n + s.len()].copy_from_slice(&s);
            *n += s.len();
        }
    };
    // PS/2 bits: 0 left, 1 right, 2 middle; xterm buttons: 0 left, 1 middle, 2 right
    for (bit, b) in [(1u8, 0u8), (4, 1), (2, 2)] {
        if (btn ^ old) & bit != 0 {
            emit(b, btn & bit != 0);
        }
    }
    if moved && (mode == 1003 || (mode == 1002 && btn != 0)) {
        let held = [(1u8, 0u8), (4, 1), (2, 2)].iter().find(|(bit, _)| btn & bit != 0).map_or(3, |p| p.1);
        emit(32 + held, true);
    }
    for _ in 0..wheel.unsigned_abs().min(5) {
        emit(if wheel < 0 { 64 } else { 65 }, true); // PS/2 Z < 0 = wheel up
    }
    None
}

const MENU_FB: &[(&str, &str)] = &[("S", "SSH"), ("W", "WiFi"), ("G", "graphics"), ("D", "demo"), ("T", "try it"), ("F12", "voice"), ("R", "reset")];
const MENU_TEXT: &[(&str, &str)] = &[("S", "SSH"), ("W", "WiFi"), ("F12", "voice"), ("D", "demo"), ("L", "log"), ("R", "restart")];

static mut WHY: &str = "host did not answer"; // why the last session showed nothing (home status line)
static mut HOME_AT_MS: u64 = 0; // ms after CPU reset when the home screen first came up (the demo's boot figure)

extern "C" {
    static __load_start: u8;
    static __load_end: u8;
}

/// Show and tell (home key D): the trackpad draws, the keyboard echoes, and the screen, power and kernel facts are
/// live. Esc returns home. False = bmctl's idle bound fired (the caller resets), as in the home loop.
fn demo(ctl: &mut bmctl::Ctl, iface: &mut Interface, dev: &mut Nic, sockets: &mut SocketSet, udp_h: SocketHandle) -> bool {
    let (cw, rh) = (console::cols(), console::rows());
    let mid = cw / 2;
    let (pr0, pc0, pr1, pc1) = (3, 2, rh - 4, mid - 2);
    let x = mid + 1;
    let tw = (cw - x - 2) / 2; // tile width
    let x2 = x + tw + 1;
    let fb = console::is_fb();
    let draw = || {
        console::demo_clear();
        console::text(1, 2, console::D_TEXT, b"NC110 agentic OS");
        console::text(1, 19, console::d_dim(), b"show and tell");
        console::text(1, cw - 13, console::d_dim(), b"Esc: go back");
        console::text(2, 3, console::D_KEY, b"Trackpad: move to draw, C clears");
        console::frame(pr0, pc0, pr1, pc1);
        for (r, c, l) in [(2, x, &b"CPU ASLEEP"[..]), (2, x2, b"POWER"), (7, x, b"BOOT"), (7, x2, b"KERNEL")] {
            console::text(r, c, console::d_dim(), l);
        }
        let h = unsafe { HOME_AT_MS };
        let mut l = heapless::String::<24>::new();
        let _ = if h < 60_000 { core::fmt::write(&mut l, format_args!("{}.{} s", h / 1000, h / 100 % 10)) } else { l.push_str("n/a").map_err(|_| core::fmt::Error) }; // over a minute: loaded live into RAM, the clock counts from the first power-on
        console::big(8, x, if fb { 2 } else { 1 }, 9, tw, l.as_bytes());
        console::text(10, x, console::d_dim(), b"power-on to home");
        l.clear();
        let kb = (core::ptr::addr_of!(__load_end) as u64 - core::ptr::addr_of!(__load_start) as u64) / 1024;
        let _ = core::fmt::write(&mut l, format_args!("{}.{} MB", kb / 1024, kb % 1024 * 10 / 1024));
        console::big(8, x2, if fb { 2 } else { 1 }, 13, tw, l.as_bytes());
        console::text(10, x2, console::d_dim(), b"Rust, no OS below");
        console::text(12, x, console::d_dim(), b"BUILT FROM SCRATCH, ON THIS MACHINE");
        console::text(16, x + 2, console::D_TEXT, if fb { b"Screen    1024x600 panel, 3 text sizes" } else { b"Screen    VGA text" });
        console::text(17, x + 2, console::D_TEXT, b"Input     keyboard + trackpad, PS/2");
        for r in 13..=17 {
            console::dot(r, x, console::D_OK, 0x07);
        }
        console::text(19, x, console::d_dim(), b"KEYBOARD");
        if rh >= 24 {
            console::text(rh - 2, 3, console::D_KEY, b"No Linux underneath: one small Rust kernel drives all of this hardware.");
        }
    };
    // live rows: WiFi, tunnel, network stack; refreshed every second with the numbers
    let live = |pct: u64| {
        let mut l = heapless::String::<80>::new();
        let _ = core::fmt::write(&mut l, format_args!("{}.{}%", pct / 10, pct % 10));
        console::big(3, x, if fb { 2 } else { 1 }, 10, tw, l.as_bytes());
        console::meter(5, x, tw - 1, pct as u32, 10);
        l.clear();
        let (big, sub, col): (&str, &str, u8) = match (crate::ec::battery(), crate::ec::power_mw()) {
            (Some((_, false, _)), Some(mw)) => {
                let _ = core::fmt::write(&mut l, format_args!("{}.{} W", mw / 1000, mw % 1000 / 100));
                ("", "whole netbook, live", 14)
            }
            (Some((p, true, _)), _) => {
                let _ = core::fmt::write(&mut l, format_args!("AC {}%", p));
                ("", "unplug: live watts", 8)
            }
            _ => ("n/a", "no battery reading", 8),
        };
        console::big(3, x2, if fb { 2 } else { 1 }, col, tw, if big.is_empty() { l.as_bytes() } else { big.as_bytes() });
        console::text_field(5, x2, tw, console::d_dim(), sub.as_bytes());
        l.clear();
        let wifi = crate::iwl::address();
        match &wifi {
            Some((ssid, ip)) => { let _ = core::fmt::write(&mut l, format_args!("WiFi      {} {}", ssid, ip.address())); }
            None => { let _ = l.push_str("WiFi      Intel 130 driver: W at home"); }
        }
        console::text_field(13, x + 2, cw - x - 3, console::D_TEXT, l.as_bytes());
        console::dot(13, x, if wifi.is_some() { console::D_OK } else { console::d_dim() }, 0x07);
        console::text_field(14, x + 2, cw - x - 3, console::D_TEXT,
            if crate::wg::up() { b"Tunnel    WireGuard home: up" } else { b"Tunnel    WireGuard, after WiFi" });
        console::dot(14, x, if crate::wg::up() { console::D_OK } else { console::d_dim() }, 0x07);
        console::text_field(15, x + 2, cw - x - 3, console::D_TEXT, b"Network   TCP/IP, DHCP, WPA2, SSH");
    };
    draw();
    let mut echo: heapless::Vec<u8, 64> = heapless::Vec::new();
    let (mut t_stat, mut tsc0, mut slept0) = (0i64, crate::port::rdtsc(), crate::idle::slept());
    loop {
        wdt::pet();
        crate::net::rx_budget(Some(32));
        let polled = iface.poll(now(), dev, sockets);
        if let Some(a) = ctl.step(sockets.get_mut::<udp::Socket>(udp_h), time::now_ms()) {
            if !ctl.act(a, Some(&dev.0)) {
                return false;
            }
        }
        let (mut k, mut n) = ([0u8; 16], 0usize);
        if kbd::poll(&mut k, &mut n) {
            wdt::hard_reset();
        }
        let mut i = 0;
        while i < n {
            let b = k[i];
            i += 1;
            match b {
                0x1b if i + 1 < n && (k[i] == b'[' || k[i] == b'O') => {
                    let name: &[u8] = match k[i + 1] { b'A' => b"<up>", b'B' => b"<down>", b'C' => b"<right>", b'D' => b"<left>", _ => b"<key>" };
                    i += 2;
                    for &c in name {
                        if echo.is_full() { echo.remove(0); }
                        let _ = echo.push(c);
                    }
                }
                0x1b => return true,
                b'c' | b'C' => {
                    console::frame(pr0, pc0, pr1, pc1);
                    if echo.is_full() { echo.remove(0); }
                    let _ = echo.push(b);
                }
                0x20..=0x7E => {
                    if echo.is_full() { echo.remove(0); }
                    let _ = echo.push(b);
                }
                b'\r' | b'\n' => {
                    for &c in b" <enter> " {
                        if echo.is_full() { echo.remove(0); }
                        let _ = echo.push(c);
                    }
                }
                _ => {}
            }
        }
        if n > 0 {
            let w = cw - x - 2;
            let s = &echo[echo.len().saturating_sub(w)..];
            console::text_field(20, x, w, console::D_TEXT, s);
        }
        let _ = mouse_step(None); // moves the pointer; a click on the demo screen does nothing
        let (px, py, _) = unsafe { PTR };
        let (col, row) = ((px / MX) as usize, (py / MY) as usize);
        if row > pr0 && row < pr1 && col > pc0 && col < pc1 {
            const RAINBOW: [u16; 7] = [0x0C00, 0x0E00, 0x0A00, 0x0B00, 0x0900, 0x0D00, 0x0500];
            console::dot(row, col, RAINBOW[(time::now_ms() / 150) as usize % 7], 0x07);
        }
        let t = time::now_ms();
        if t - t_stat >= 1000 {
            t_stat = t;
            let (tsc, sl) = (crate::port::rdtsc(), crate::idle::slept());
            let pct = (sl.wrapping_sub(slept0) as u128 * 1000 / tsc.wrapping_sub(tsc0).max(1) as u128) as u64;
            (tsc0, slept0) = (tsc, sl);
            live(pct);
            // the trackpad box text never overlaps the tiles; redrawing the fixed big numbers every second also
            // repairs any slice the pointer erased
            let h = unsafe { HOME_AT_MS };
            let mut l = heapless::String::<24>::new();
            let _ = if h < 60_000 { core::fmt::write(&mut l, format_args!("{}.{} s", h / 1000, h / 100 % 10)) } else { l.push_str("n/a").map_err(|_| core::fmt::Error) }; // over a minute: loaded live into RAM, the clock counts from the first power-on
            console::big(8, x, if fb { 2 } else { 1 }, 9, tw, l.as_bytes());
            l.clear();
            let kb = (core::ptr::addr_of!(__load_end) as u64 - core::ptr::addr_of!(__load_start) as u64) / 1024;
            let _ = core::fmt::write(&mut l, format_args!("{}.{} MB", kb / 1024, kb % 1024 * 10 / 1024));
            console::big(8, x2, if fb { 2 } else { 1 }, 13, tw, l.as_bytes());
        }
        if n == 0 && matches!(polled, smoltcp::iface::PollResult::None) {
            crate::idle::nap(2);
        }
    }
}

/// Full-screen graphics (home key G): the plasma at native resolution, paced to the panel's vertical blank, a ripple
/// that follows the trackpad, FPS and render time per frame in the top bar. Esc returns. False = bmctl's idle bound.
fn gfx_screen(ctl: &mut bmctl::Ctl, iface: &mut Interface, dev: &mut Nic, sockets: &mut SocketSet, udp_h: SocketHandle, auto: bool) -> bool {
    let Some(fb) = console::fb_info() else { return true };
    let len = fb.pitch as u64 * fb.h as u64;
    // the effects write 32-bit pixels, at most 1024 wide (plasma) and 1200 high (tunnel tables)
    if fb.bytes != 4 || fb.h > 1200 || fb.pitch < fb.w.min(1024) * 4 {
        return true;
    }
    // else stay uncached: slower, still correct; already WC from boot on the NC110 (console::use_framebuffer)
    let wc = !console::fb_is_wc() && crate::gma::aperture_holds(fb.addr, len);
    if wc {
        crate::nic::fb_wc(fb.addr, len, true);
    }
    let bar = 2; // text rows on top; the plasma below them
    let y0 = (bar * console::fb_cell_h()) as u32;
    let (mut t, mut frames, mut t1, mut busy) = (0u32, 0u32, time::now_ms(), 0u64);
    let mut last = crate::gma::frame();
    let (mut effect, mut switched, mut p_last, mut moved_at) = (0u8, time::now_ms(), unsafe { PTR }, 0i64);
    let header = || {
        console::demo_clear();
        console::text(0, 2, console::D_TEXT, b"NC110 bare metal: every pixel live, 1024x600   Space: next effect");
        console::text(0, console::cols() - 13, console::d_dim(), b"Esc: go back");
    };
    header();
    console::status("building the tunnel tables");
    let ok = loop {
        wdt::pet();
        crate::net::rx_budget(Some(32));
        iface.poll(now(), dev, sockets);
        if let Some(a) = ctl.step(sockets.get_mut::<udp::Socket>(udp_h), time::now_ms()) {
            if !ctl.act(a, Some(&dev.0)) {
                break false;
            }
        }
        let (mut k, mut n) = ([0u8; 16], 0usize);
        if kbd::poll(&mut k, &mut n) {
            wdt::hard_reset();
        }
        if k[..n].contains(&0x1b) {
            break true;
        }
        if k[..n].iter().any(|&c| c == b' ' || c == b'\r') || (auto && time::now_ms() - switched > if effect == 2 { 12_000 } else { 15_000 }) {
            // keys: tunnel <-> plasma; attract mode also shows the Try-it card
            effect = match (effect, auto && n == 0) { (0, _) => 1, (1, true) => 2, _ => 0 };
            switched = time::now_ms();
            if effect == 2 {
                draw_try();
            } else if effect == 0 && auto {
                header();
            }
        }
        if effect == 2 {
            continue;
        }
        let _ = mouse_step(None);
        // wait for the next vertical blank (no GMA, as in q35: 16 ms)
        let tw = time::now_ms();
        loop {
            let f = crate::gma::frame();
            if (last.is_some() && f != last) || (last.is_none() && time::now_ms() - tw >= 16) || time::now_ms() - tw > 50 {
                last = f;
                break;
            }
        }
        let (px, py, _) = unsafe { PTR };
        let s0 = crate::port::rdtsc();
        let (mx, my) = ((px / MX) as i32 * fb.w as i32 / console::cols() as i32, (py / MY) as i32 * fb.h as i32 / console::rows() as i32);
        if unsafe { PTR } != p_last {
            (p_last, moved_at) = (unsafe { PTR }, time::now_ms());
        }
        // the pointer steers; idle, the centre drifts on its own
        let (cx, cy) = if time::now_ms() - moved_at < 5000 { (mx, my) } else {
            let i = t as i32;
            (fb.w as i32 / 2 + (crate::gfx::wave(i) - 127) * 2, fb.h as i32 / 2 + (crate::gfx::wave(i * 3 / 2 + 64) - 127))
        };
        if effect == 0 { crate::gfx::tunnel(&fb, y0, t, cx, cy) } else { crate::gfx::plasma(&fb, y0, t, cx, cy) }
        busy += crate::port::rdtsc() - s0;
        t = t.wrapping_add(1);
        frames += 1;
        let now_t = time::now_ms();
        if now_t - t1 >= 1000 {
            let ms10 = crate::time::tsc_to_ms(busy * 10) / frames.max(1) as u64; // tenths of a ms per frame
            let mut l = heapless::String::<80>::new();
            let _ = core::fmt::write(&mut l, format_args!("{} fps   render {}.{} ms of the 16.7 ms frame   integer only, no GPU",
                frames * 1000 / (now_t - t1) as u32, ms10 / 10, ms10 % 10));
            console::text_field(1, 2, console::cols() - 4, console::D_OK, l.as_bytes());
            (frames, t1, busy) = (0, now_t, 0);
        }
    };
    console::wipe(); // the plasma/tunnel pixels are not in the cell shadow
    if wc {
        crate::nic::fb_wc(fb.addr, len, false);
    }
    ok
}

/// The Try-it card: why and how to do this on your own machine, and the repository as a QR code.
fn draw_try() {
    console::wipe();
    console::demo_clear();
    console::big(1, 3, 2, 9, console::cols() - 6, b"Try this on your own machine");
    let t = console::D_TEXT;
    let d = console::d_dim();
    let lines: [(usize, u16, &[u8]); 13] = [
        (5, d, b"WHY"),
        (6, t, b"This OS was written by AI coding agents in six days."),
        (7, t, b"One person steered and approved each step."),
        (8, t, b"Your old laptop has hardware nobody will write a"),
        (9, t, b"driver for again. Your agent can."),
        (11, d, b"HOW"),
        (12, t, b"1  Get the repository: scan the code, or the address below"),
        (13, t, b"2  Start small: serial hello, a watchdog, then"),
        (14, t, b"   one device at a time"),
        (15, t, b"3  Keep a way back: a watchdog, a fallback boot,"),
        (16, t, b"   test kernels loaded into RAM, never onto the disk"),
        (18, d, b"Built with Claude Code, reviewed by Codex."),
        (20, console::D_KEY, crate::qr::URL.as_bytes()),
    ];
    for (r, a, s) in lines {
        console::text(r, 3, a, s);
    }
    let c = console::cols().saturating_sub(22);
    console::qr(5, c, 5);
    console::text(console::rows() - 2, 3, d, b"Esc: go back");
}

/// Home key T: the Try-it card until Esc. False = bmctl's idle bound fired.
fn try_screen(ctl: &mut bmctl::Ctl, iface: &mut Interface, dev: &mut Nic, sockets: &mut SocketSet, udp_h: SocketHandle) -> bool {
    draw_try();
    let mut k = [0u8; 16];
    loop {
        let Some(n) = wifi_pass(ctl, iface, dev, sockets, udp_h, &mut k) else { return false };
        if k[..n].contains(&0x1b) {
            return true;
        }
    }
}

/// A host on the wired /24 is reached over the cable, where bmctl (and so voice) keeps running during the session.
fn wired_reaches(wired: Ipv4Address, host: Ipv4Address) -> bool {
    wired.octets()[..3] == host.octets()[..3]
}

fn wifi_status() -> heapless::String<64> {
    let mut s = heapless::String::new();
    match crate::iwl::address() {
        Some((ssid, ip)) => { let _ = core::fmt::write(&mut s, format_args!("WiFi: {} {}{}", ssid, ip, if crate::wg::up() { "  tunnel up" } else { "" })); }
        None => { let _ = s.push_str("ready"); }
    }
    s
}

/// Keys and bmctl while a WiFi step waits (the bmctl step also drains the radio). Returns the typed bytes; None =
/// bmctl's idle bound fired.
fn wifi_pass(ctl: &mut bmctl::Ctl, iface: &mut Interface, dev: &mut Nic, sockets: &mut SocketSet, udp_h: SocketHandle, k: &mut [u8; 16]) -> Option<usize> {
    wdt::pet();
    crate::net::rx_budget(Some(32));
    iface.poll(now(), dev, sockets);
    if let Some(a) = ctl.step(sockets.get_mut::<udp::Socket>(udp_h), time::now_ms()) {
        if !ctl.act(a, Some(&dev.0)) {
            return None;
        }
    }
    let mut n = 0;
    if kbd::poll(k, &mut n) {
        wdt::hard_reset();
    }
    let _ = mouse_step(None);
    if n == 0 {
        crate::idle::nap(2);
    }
    Some(n)
}

/// The WiFi screen (home key W): radio up, scan, pick a network, type its passphrase (masked), join, DHCP.
/// False = bmctl's idle bound fired (the caller resets).
fn wifi_screen(ctl: &mut bmctl::Ctl, iface: &mut Interface, dev: &mut Nic, sockets: &mut SocketSet, udp_h: SocketHandle) -> bool {
    console::splash();
    console::status("WiFi: starting the radio");
    if let Err(e) = crate::iwl::bring_up() {
        return wifi_hold(ctl, iface, dev, sockets, udp_h, if e.starts_with("no 8086") { "WiFi: no Centrino card in this machine" } else { e });
    }
    console::status("WiFi: scanning");
    if crate::iwl::scan_start().is_err() {
        return wifi_hold(ctl, iface, dev, sockets, udp_h, "WiFi: scan refused");
    }
    let mut k = [0u8; 16];
    let t0 = time::now_ms();
    while crate::iwl::scanning() && time::now_ms() - t0 < 5000 {
        if wifi_pass(ctl, iface, dev, sockets, udp_h, &mut k).is_none() {
            return false;
        }
    }
    let mut nets = heapless::Vec::new();
    crate::iwl::networks(&mut nets);
    if nets.is_empty() {
        return wifi_hold(ctl, iface, dev, sockets, udp_h, "WiFi: no networks heard");
    }
    let lines: heapless::Vec<heapless::String<72>, 16> = nets.iter().enumerate().map(|(i, (s, ch, dbm, sec))| {
        let mut l = heapless::String::<72>::new();
        let key = if i < 9 { (b'1' + i as u8) as char } else { ' ' };
        let _ = core::fmt::write(&mut l, format_args!(" {}  {:<32} ch {:<2} {:>4} dBm  {} ", key, s.as_str(), ch, dbm, if *sec { "WPA2" } else { "open" }));
        l
    }).collect();
    let mut sel = 0usize;
    // a page of its own: up to 16 networks (the home chooser has room for 8)
    let page = |sel: usize| {
        console::demo_clear();
        let mut t = heapless::String::<64>::new();
        let _ = core::fmt::write(&mut t, format_args!("WiFi: choose a network ({} heard, strongest first)", lines.len()));
        console::text(1, 3, console::D_TEXT, t.as_bytes());
        for (i, l) in lines.iter().enumerate() {
            console::text_field(3 + i, 3, console::cols() - 6, if i == sel { 0x7000 } else { console::D_TEXT }, l.as_bytes());
        }
        console::text(console::rows() - 2, 3, console::d_dim(), b"1-9 or arrows + Enter: join    R: scan again    Esc: back");
    };
    page(sel);
    let pick = loop {
        let Some(n) = wifi_pass(ctl, iface, dev, sockets, udp_h, &mut k) else { return false };
        let mut i = 0;
        let mut done = None;
        while i < n {
            let b = k[i];
            i += 1;
            match b {
                0x1b if i + 1 < n && (k[i] == b'[' || k[i] == b'O') => {
                    sel = match k[i + 1] { b'A' => sel.saturating_sub(1), b'B' => (sel + 1).min(nets.len() - 1), _ => sel };
                    i += 2;
                    page(sel);
                }
                0x1b => return true,
                b'r' | b'R' => return wifi_screen(ctl, iface, dev, sockets, udp_h),
                b'\r' | b'\n' => done = Some(sel),
                b'1'..=b'9' if ((b - b'1') as usize) < nets.len() => done = Some((b - b'1') as usize),
                _ => {}
            }
        }
        if let Some(d) = done {
            break d;
        }
    };
    let (ssid, _, _, secured) = nets[pick].clone();
    let mut pass: heapless::String<64> = heapless::String::new();
    if secured {
        let mut redraw = true;
        loop {
            if redraw {
                let mut l = heapless::String::<96>::new();
                let _ = core::fmt::write(&mut l, format_args!("passphrase for {}: ", ssid.as_str()));
                for _ in 0..pass.len() {
                    let _ = l.push('*');
                }
                console::splash();
                console::status(&l);
            }
            let Some(n) = wifi_pass(ctl, iface, dev, sockets, udp_h, &mut k) else { return false };
            redraw = n > 0;
            let mut enter = false;
            for &b in &k[..n] {
                match b {
                    0x1b => return true,
                    b'\r' | b'\n' => enter = true,
                    0x08 | 0x7F => { pass.pop(); }
                    0x20..=0x7E => { let _ = pass.push(b as char); }
                    _ => {}
                }
            }
            if enter && pass.len() >= 8 {
                break;
            }
        }
    }
    console::splash();
    let mut l = heapless::String::<96>::new();
    let _ = core::fmt::write(&mut l, format_args!("WiFi: joining {}", ssid.as_str()));
    console::status(&l);
    let pmk = if secured { crate::wpa::psk(pass.as_bytes(), ssid.as_bytes()) } else { [0; 32] };
    pass.clear();
    if let Err(e) = crate::iwl::join_with(ssid.as_str(), &pmk) {
        l.clear();
        let _ = core::fmt::write(&mut l, format_args!("WiFi: {}", e));
        return wifi_hold(ctl, iface, dev, sockets, udp_h, &l);
    }
    console::status("WiFi: joined, getting an address");
    if crate::iwl::dhcp_wait(8000).is_some() && crate::wg::configured() {
        console::status("WiFi: joined, starting the home tunnel");
        if !crate::wg::bring_up() {
            return wifi_hold(ctl, iface, dev, sockets, udp_h, "WiFi: joined, but the home tunnel did not answer");
        }
    }
    wifi_hold(ctl, iface, dev, sockets, udp_h, &wifi_status())
}

/// Leave a WiFi result on screen until a key or 5 s.
fn wifi_hold(ctl: &mut bmctl::Ctl, iface: &mut Interface, dev: &mut Nic, sockets: &mut SocketSet, udp_h: SocketHandle, msg: &str) -> bool {
    kprintln!("wifi screen: {}", msg);
    console::status(msg);
    let mut k = [0u8; 16];
    let t0 = time::now_ms();
    while time::now_ms() - t0 < 5000 {
        match wifi_pass(ctl, iface, dev, sockets, udp_h, &mut k) {
            None => return false,
            Some(0) => {}
            Some(_) => break,
        }
    }
    true
}
