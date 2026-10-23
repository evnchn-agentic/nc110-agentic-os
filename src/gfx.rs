//! Full-screen real-time graphics (home key G): a plasma computed per pixel at the panel's native 1024x600, with a
//! ripple centred on the trackpad pointer, paced to the GMA's vertical blank. Integer only (the kernel has no SSE).
use crate::console::Fb;

static mut SIN: [i32; 256] = [0; 256]; // 0..=254, one period over 256 steps
static mut PAL: [u32; 256] = [0; 256];
static mut READY: bool = false;

/// sin over 256 steps scaled to 0..254, from Bhaskara's approximation (no floating point).
fn isin(i: i32) -> i32 {
    let d = (i & 255) * 360 / 256; // degrees
    let (d, neg) = if d >= 180 { (d - 180, true) } else { (d, false) };
    let s = 4 * d * (180 - d) * 127 / (40500 - d * (180 - d)); // 0..127
    if neg { 127 - s } else { 127 + s }
}

/// 0..254 sine wave over 256 steps (motion paths).
pub fn wave(i: i32) -> i32 {
    isin(i)
}

fn init(fb: &Fb) {
    unsafe {
        let (sin, pal) = (&mut *core::ptr::addr_of_mut!(SIN), &mut *core::ptr::addr_of_mut!(PAL));
        for i in 0..256 {
            sin[i] = isin(i as i32);
        }
        for i in 0..256 {
            let r = isin(i as i32) as u32;
            let g = isin(i as i32 + 85) as u32;
            let b = isin(i as i32 + 170) as u32;
            pal[i] = crate::console::rgb(fb, r << 16 | g << 8 | b);
        }
        READY = true;
    }
}

/// One frame from row `y0` down: t = frame number, (mx, my) = ripple centre in pixels.
pub fn plasma(fb: &Fb, y0: u32, t: u32, mx: i32, my: i32) {
    if !unsafe { READY } {
        init(fb);
    }
    let (sin, pal) = unsafe { (&*core::ptr::addr_of!(SIN), &*core::ptr::addr_of!(PAL)) };
    let t = t as i32;
    let w = fb.w.min(1024) as usize;
    let mut colx = [0i32; 1024];
    for (x, c) in colx.iter_mut().enumerate().take(w) {
        *c = sin[((x as i32 >> 1) + t * 2) as usize & 255] + sin[((x as i32 >> 2) - t) as usize & 255];
    }
    for y in y0..fb.h {
        let yi = y as i32;
        let ry = sin[((yi >> 1) + t * 3) as usize & 255];
        let dy = (yi - my).abs();
        let line = (fb.addr + y as u64 * fb.pitch as u64) as *mut u32;
        for x in 0..w {
            let xi = x as i32;
            let ripple = sin[(((xi - mx).abs() + dy) - t * 4) as usize & 255];
            let v = colx[x] + ry + ripple + sin[((xi + yi) >> 2) as usize & 255];
            unsafe { core::ptr::write_volatile(line.add(x), pal[((v >> 2) + t) as usize & 255]) };
        }
    }
}

// ---- tunnel: per-pixel (distance, angle, shade) over a 2x grid so the centre can move; a 256x256 XOR texture ----
const GW: usize = 2048;
const GH: usize = 1200;
static mut TDIST: [u8; GW * GH] = [0; GW * GH];
static mut TANG: [u8; GW * GH] = [0; GW * GH];
static mut TSHADE: [u8; GW * GH] = [0; GW * GH];
static mut TEX: [[u32; 65536]; 4] = [[0; 65536]; 4]; // four brightness levels
static mut TREADY: bool = false;

fn isqrt(n: u32) -> u32 {
    if n < 2 {
        return n;
    }
    let mut x = n;
    let mut y = (x + 1) / 2;
    while y < x {
        x = y;
        y = (x + n / x) / 2;
    }
    x
}

/// atan2 in 256 steps per turn, integer (octants + a quadratic fit of atan on [0, 1], max error ~0.3 degree).
fn iatan2(dy: i32, dx: i32) -> u8 {
    let (ax, ay) = (dx.unsigned_abs() as i64, dy.unsigned_abs() as i64);
    if ax == 0 && ay == 0 {
        return 0;
    }
    let (mn, mx) = (ax.min(ay), ax.max(ay));
    let r = mn * 1024 / mx; // 0..1024
    // atan(r) in 256ths of a turn: 32 * r + 5.6 * r * (1 - r), r in [0, 1]
    let mut a = (32 * r + 23 * r * (1024 - r) / 4096) / 1024; // 0..32
    if ay > ax {
        a = 64 - a;
    }
    if dx < 0 {
        a = 128 - a;
    }
    if dy < 0 {
        a = 256 - a;
    }
    (a & 255) as u8
}

fn tinit(fb: &Fb) {
    unsafe {
        let (d, an, sh) = (&mut *core::ptr::addr_of_mut!(TDIST), &mut *core::ptr::addr_of_mut!(TANG), &mut *core::ptr::addr_of_mut!(TSHADE));
        for y in 0..GH {
            crate::wdt::pet();
            for x in 0..GW {
                let (dx, dy) = (x as i32 - GW as i32 / 2, y as i32 - GH as i32 / 2);
                let r = isqrt((dx * dx + dy * dy) as u32).max(1);
                let i = y * GW + x;
                d[i] = (64 * 256 / r) as u8; // perspective: depth into the tube
                an[i] = iatan2(dy, dx).wrapping_mul(2); // the texture wraps twice around the tube
                sh[i] = (r / 60).min(3) as u8; // dark in the distance, bright at the rim
            }
        }
        let tex = &mut *core::ptr::addr_of_mut!(TEX);
        for v in 0..256u32 {
            for u in 0..256u32 {
                let c = (u ^ v) as i32; // the classic XOR pattern, coloured through a hue ramp
                let r = isin(c) as u32;
                let g = isin(c + 64) as u32;
                let b = isin(c + 128) as u32 | 0x40;
                for (l, t) in tex.iter_mut().enumerate() {
                    let k = l as u32 + 1;
                    let rgb = (r * k / 4) << 16 | (g * k / 4) << 8 | (b.min(255) * k / 4);
                    t[(v << 8 | u) as usize] = crate::console::rgb(fb, rgb);
                }
            }
        }
        TREADY = true;
    }
}

/// One tunnel frame from row `y0`; (cx, cy) = where the tube's far end sits on screen.
pub fn tunnel(fb: &Fb, y0: u32, t: u32, cx: i32, cy: i32) {
    if !unsafe { TREADY } {
        tinit(fb);
    }
    let (d, an, sh, tex) = unsafe {
        (&*core::ptr::addr_of!(TDIST), &*core::ptr::addr_of!(TANG), &*core::ptr::addr_of!(TSHADE), &*core::ptr::addr_of!(TEX))
    };
    let w = fb.w.min(1024) as usize;
    let ox = (GW as i32 / 2 - cx).clamp(0, (GW - w) as i32) as usize;
    let oy = (GH as i32 / 2 - cy).clamp(0, (GH - fb.h as usize) as i32) as usize;
    let (du, dv) = ((t * 3) as u8, t as u8);
    for y in y0..fb.h {
        let row = (y as usize + oy) * GW + ox;
        let line = (fb.addr + y as u64 * fb.pitch as u64) as *mut u32;
        for x in 0..w {
            let i = row + x;
            let u = d[i].wrapping_add(du);
            let v = an[i].wrapping_add(dv);
            let c = tex[sh[i] as usize][(v as usize) << 8 | u as usize];
            unsafe { core::ptr::write_volatile(line.add(x), c) };
        }
    }
}
