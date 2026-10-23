//! WPA2-PSK host-side crypto for the WiFi driver: PSK (PBKDF2-HMAC-SHA1, 4096 rounds), the 802.11 PRF (HMAC-SHA1)
//! for the PTK, the EAPOL-Key MIC (HMAC-SHA1-128), and AES key unwrap (RFC 3394) for the GTK. `selftest` checks the
//! IEEE 802.11i Annex H vectors and RFC 3394 §4.1; bmctl `wpa selftest`.
use aes::cipher::{BlockCipherDecrypt, KeyInit};
use hmac::{Hmac, KeyInit as _, Mac};
use sha1::Sha1;

type HmacSha1 = Hmac<Sha1>;

pub fn psk(passphrase: &[u8], ssid: &[u8]) -> [u8; 32] {
    let mut out = [0u8; 32];
    pbkdf2::pbkdf2_hmac::<Sha1>(passphrase, ssid, 4096, &mut out);
    out
}

/// PRF-n (IEEE 802.11-2016 12.7.1.2): HMAC-SHA1(key, label || 0 || data || i), i = 0, 1, ... concatenated.
pub fn prf(key: &[u8], label: &[u8], data: &[u8], out: &mut [u8]) {
    let mut i = 0u8;
    let mut off = 0;
    while off < out.len() {
        let mut m = <HmacSha1 as hmac::KeyInit>::new_from_slice(key).unwrap();
        m.update(label);
        m.update(&[0]);
        m.update(data);
        m.update(&[i]);
        let r = m.finalize().into_bytes();
        let n = (out.len() - off).min(20);
        out[off..off + n].copy_from_slice(&r[..n]);
        off += n;
        i += 1;
    }
}

/// PTK for CCMP (PRF-384): KCK 0..16, KEK 16..32, TK 32..48.
pub fn ptk(pmk: &[u8; 32], aa: &[u8; 6], spa: &[u8; 6], anonce: &[u8; 32], snonce: &[u8; 32]) -> [u8; 48] {
    let mut d = [0u8; 76];
    let (a, b) = if aa < spa { (aa, spa) } else { (spa, aa) };
    d[..6].copy_from_slice(a);
    d[6..12].copy_from_slice(b);
    let (x, y) = if anonce < snonce { (anonce, snonce) } else { (snonce, anonce) };
    d[12..44].copy_from_slice(x);
    d[44..76].copy_from_slice(y);
    let mut out = [0u8; 48];
    prf(pmk, b"Pairwise key expansion", &d, &mut out);
    out
}

/// EAPOL-Key MIC (key descriptor version 2): HMAC-SHA1(KCK, frame with the MIC field zeroed)[..16].
pub fn mic(kck: &[u8], frame: &[u8]) -> [u8; 16] {
    let mut m = <HmacSha1 as hmac::KeyInit>::new_from_slice(kck).unwrap();
    m.update(frame);
    let r = m.finalize().into_bytes();
    let mut o = [0u8; 16];
    o.copy_from_slice(&r[..16]);
    o
}

/// RFC 3394 key unwrap with a 128-bit KEK; `out` gets len(wrapped) - 8 bytes. False on an integrity failure.
pub fn aes_unwrap(kek: &[u8; 16], wrapped: &[u8], out: &mut [u8]) -> bool {
    if wrapped.len() < 24 || wrapped.len() % 8 != 0 || out.len() < wrapped.len() - 8 {
        return false;
    }
    let n = wrapped.len() / 8 - 1;
    let c = aes::Aes128::new_from_slice(kek).unwrap();
    let mut a = [0u8; 8];
    a.copy_from_slice(&wrapped[..8]);
    out[..n * 8].copy_from_slice(&wrapped[8..]);
    for j in (0..6).rev() {
        for i in (1..=n).rev() {
            let t = (n * j + i) as u64;
            let mut b = [0u8; 16];
            for (k, x) in a.iter().zip(t.to_be_bytes()).map(|(p, q)| p ^ q).enumerate() {
                b[k] = x;
            }
            b[8..].copy_from_slice(&out[(i - 1) * 8..i * 8]);
            let mut blk = aes::Block::from(b);
            c.decrypt_block(&mut blk);
            a.copy_from_slice(&blk[..8]);
            out[(i - 1) * 8..i * 8].copy_from_slice(&blk[8..]);
        }
    }
    a == [0xA6; 8]
}

fn hex(s: &str, out: &mut [u8]) {
    for i in 0..out.len() {
        out[i] = u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap_or(0);
    }
}

/// Known-answer tests; (passed, total).
pub fn selftest() -> (u32, u32) {
    let mut ok = 0;
    // IEEE 802.11i-2004 H.4.1: passphrase "password", SSID "IEEE"
    let mut want = [0u8; 32];
    hex("f42c6fc52df0ebef9ebb4b90b38a5f902e83fe1b135a70e23aed762e9710a12e", &mut want);
    ok += (psk(b"password", b"IEEE") == want) as u32;
    // IEEE 802.11i-2004 H.3 PRF test 1: key 0x0b x 20, prefix "prefix", data "Hi There", 64 bytes
    let mut want = [0u8; 64];
    hex("bcd4c650b30b9684951829e0d75f9d54b862175ed9f00606e17d8da35402ffee75df78c3d31e0f889f012120c0862beb67753e7439ae242edb8373698356cf5a", &mut want);
    let mut got = [0u8; 64];
    prf(&[0x0b; 20], b"prefix", b"Hi There", &mut got);
    ok += (got == want) as u32;
    // RFC 3394 4.1: 128-bit key data under a 128-bit KEK
    let (mut kek, mut wrapped, mut key) = ([0u8; 16], [0u8; 24], [0u8; 16]);
    hex("000102030405060708090A0B0C0D0E0F", &mut kek);
    hex("1FA68B0A8112B447AEF34BD8FB5A7B829D3E862371D2CFE5", &mut wrapped);
    hex("00112233445566778899AABBCCDDEEFF", &mut key);
    let mut out = [0u8; 16];
    ok += (aes_unwrap(&kek, &wrapped, &mut out) && out == key) as u32;
    // the unwrap must reject a corrupted blob
    wrapped[5] ^= 1;
    ok += (!aes_unwrap(&kek, &wrapped, &mut out)) as u32;
    (ok, 4)
}
