//! Attended build: broadcast the captured console log as UDP (192.168.50.118:5514 -> 255.255.255.255:5514).
//! Broadcast needs no ARP and no DHCP lease. Static source IP = the netbook's Slot A address.
use crate::nic::{self, Bus, Rtl};
use crate::nic_regs::Regs;
use crate::{console, dhcp, wdt};

const SRC_IP: [u8; 4] = [192, 168, 50, 118];
const PORT: u16 = 5514;

/// Copy of the NIC handles so the log can still be sent after ssh::run took the Rtl. Single-threaded and
/// tx_with is synchronous (waits for the descriptor), so interleaving with smoltcp between polls is safe.
static mut LOGNIC: Option<(Bus, &'static Regs, [u8; 6])> = None;

pub fn set_nic(n: &Rtl) {
    unsafe { LOGNIC = Some((n.bus, n.regs, n.mac)) };
}

fn send(data: &[u8]) {
    let Some((bus, regs, mac)) = (unsafe { *core::ptr::addr_of!(LOGNIC) }) else { return };
    if !nic::link_up(bus, regs) {
        return; // no cable: a TX that never completes would spin 2e9 cycles and wedge wired TX
    }
    let len = 42 + data.len();
    nic::tx_with(bus, regs, len, |f| {
        f[0..6].copy_from_slice(&[0xff; 6]);
        f[6..12].copy_from_slice(&mac);
        f[12..14].copy_from_slice(&[0x08, 0x00]);
        let ip = &mut f[14..34];
        ip.copy_from_slice(&[0x45, 0, 0, 0, 0, 0, 0x40, 0, 64, 17, 0, 0, 0, 0, 0, 0, 255, 255, 255, 255]);
        ip[2..4].copy_from_slice(&((20 + 8 + data.len()) as u16).to_be_bytes());
        ip[12..16].copy_from_slice(&SRC_IP);
        let c = dhcp::csum(ip);
        ip[10..12].copy_from_slice(&c.to_be_bytes());
        f[34..36].copy_from_slice(&PORT.to_be_bytes());
        f[36..38].copy_from_slice(&PORT.to_be_bytes());
        f[38..40].copy_from_slice(&((8 + data.len()) as u16).to_be_bytes());
        f[40..42].copy_from_slice(&[0, 0]); // UDP checksum 0 = none (IPv4)
        f[42..].copy_from_slice(data);
    });
}

/// Send the whole captured log, 1000-byte chunks, each prefixed "nc110 <round> <i>/<n> ".
pub fn dump(round: u8) {
    let log = console::captured();
    let total = log.len().div_ceil(1000).max(1);
    for (i, c) in log.chunks(1000).enumerate() {
        let mut b = [0u8; 1024];
        let h = [b'n', b'c', b'1', b'1', b'0', b' ', b'0' + round, b' ', b'0' + (i / 10) as u8, b'0' + (i % 10) as u8,
                 b'/', b'0' + (total / 10) as u8, b'0' + (total % 10) as u8, b' '];
        b[..h.len()].copy_from_slice(&h);
        b[h.len()..h.len() + c.len()].copy_from_slice(c);
        for _ in 0..2 { // chunks after the first went missing on the real LAN (k9, k11); the listener dedups
            send(&b[..h.len() + c.len()]);
            wdt::pet();
            wdt::delay_ms(50);
        }
    }
}
