//! Hand-built Ethernet/IPv4/UDP/BOOTP DHCPDISCOVER, and a minimal OFFER parser.
use core::fmt;

pub struct Mac(pub [u8; 6]);
impl fmt::Display for Mac {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let m = self.0;
        write!(f, "{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}", m[0], m[1], m[2], m[3], m[4], m[5])
    }
}
pub struct Ip(pub [u8; 4]);
impl fmt::Display for Ip {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}.{}.{}.{}", self.0[0], self.0[1], self.0[2], self.0[3])
    }
}

pub(crate) fn csum(b: &[u8]) -> u16 {
    let mut s: u32 = 0;
    for c in b.chunks(2) {
        s += (c[0] as u32) << 8 | *c.get(1).unwrap_or(&0) as u32;
    }
    while s >> 16 != 0 {
        s = (s & 0xFFFF) + (s >> 16);
    }
    !(s as u16)
}

/// Writes a broadcast DHCPDISCOVER into `buf` (>= 342 bytes); returns frame length.
pub fn build_discover(buf: &mut [u8], mac: [u8; 6], xid: u32) -> usize {
    buf.fill(0);
    let bootp_len = 300; // classic minimum BOOTP size
    let udp_len = 8 + bootp_len;
    let ip_len = 20 + udp_len;
    // Ethernet
    buf[0..6].copy_from_slice(&[0xFF; 6]);
    buf[6..12].copy_from_slice(&mac);
    buf[12..14].copy_from_slice(&[0x08, 0x00]);
    // IPv4
    let ip = &mut buf[14..34];
    ip[0] = 0x45;
    ip[2..4].copy_from_slice(&(ip_len as u16).to_be_bytes());
    ip[4..6].copy_from_slice(&0x0110u16.to_be_bytes());
    ip[8] = 64;
    ip[9] = 17;
    ip[16..20].copy_from_slice(&[255; 4]);
    let c = csum(ip);
    ip[10..12].copy_from_slice(&c.to_be_bytes());
    // UDP (checksum 0 = none, legal over IPv4)
    let udp = &mut buf[34..42];
    udp[0..2].copy_from_slice(&68u16.to_be_bytes());
    udp[2..4].copy_from_slice(&67u16.to_be_bytes());
    udp[4..6].copy_from_slice(&(udp_len as u16).to_be_bytes());
    // BOOTP
    let b = &mut buf[42..42 + bootp_len];
    b[0] = 1; // BOOTREQUEST
    b[1] = 1; // Ethernet
    b[2] = 6;
    b[4..8].copy_from_slice(&xid.to_be_bytes());
    b[10..12].copy_from_slice(&0x8000u16.to_be_bytes()); // broadcast flag
    b[28..34].copy_from_slice(&mac);
    b[236..240].copy_from_slice(&[0x63, 0x82, 0x53, 0x63]);
    let opts: &[u8] = &[
        53, 1, 1, // DHCPDISCOVER
        12, 8, b'n', b'c', b'1', b'1', b'0', b'-', b'b', b'm', // hostname
        55, 3, 1, 3, 6, // want subnet, router, dns
        255,
    ];
    b[240..240 + opts.len()].copy_from_slice(opts);
    42 + bootp_len
}

/// If `f` is a DHCPOFFER for `xid`, returns (yiaddr, server id or siaddr).
pub fn parse_offer(f: &[u8], xid: u32) -> Option<([u8; 4], [u8; 4])> {
    if f.len() < 42 + 244 || f[12..14] != [0x08, 0x00] || f[23] != 17 {
        return None;
    }
    let ihl = (f[14] & 0xF) as usize * 4;
    let udp = 14 + ihl;
    if f[udp + 2..udp + 4] != 68u16.to_be_bytes() {
        return None;
    }
    let b = &f[udp + 8..];
    if b.len() < 244 || b[0] != 2 || b[4..8] != xid.to_be_bytes() || b[236..240] != [0x63, 0x82, 0x53, 0x63] {
        return None;
    }
    let yiaddr = [b[16], b[17], b[18], b[19]];
    let mut srv = [b[20], b[21], b[22], b[23]];
    let (mut i, mut is_offer) = (240, false);
    while i + 1 < b.len() && b[i] != 255 {
        if b[i] == 0 {
            i += 1;
            continue;
        }
        let (t, l) = (b[i], b[i + 1] as usize);
        let v = b.get(i + 2..i + 2 + l)?;
        if t == 53 && l == 1 && v[0] == 2 {
            is_offer = true;
        }
        if t == 54 && l == 4 {
            srv = [v[0], v[1], v[2], v[3]];
        }
        i += 2 + l;
    }
    is_offer.then_some((yiaddr, srv))
}
