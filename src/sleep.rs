//! S3 standby with Wake-on-LAN, built so a wake can only end in a fresh boot. Nothing of this kernel is resumed: the
//! FACS waking vector points at a 12-byte real-mode stub that hard-resets (0xCF9), so the firmware's resume path lands
//! in POST -> GRUB -> the agent OS (the same path as any reboot). Every sleep also arms the CMOS RTC alarm, so the
//! netbook wakes by itself even if no magic packet comes. The firmware-specific parts (_PTS(3): SMI mailbox 0x4C, EC
//! trap 0xA4) are taken from this NC110's DSDT (proof/acpi) and only run if the live DSDT is that exact table.
use crate::port::{inb, inl, inw, outb, outl, outw};
use crate::{acpi, kprintln, mb, nic::Rtl, time, wdt};
use core::ptr::{read_volatile, write_volatile};

const STUB_AT: u64 = 0x6000;
// mov al,2; mov dx,0xCF9; out dx,al; mov al,6; out dx,al; hlt; jmp $-1
const STUB: [u8; 12] = [0xB0, 0x02, 0xBA, 0xF9, 0x0C, 0xEE, 0xB0, 0x06, 0xEE, 0xF4, 0xEB, 0xFD];
const NC110_DSDT: (u32, u8) = (24456, 0xBA); // length, checksum byte (proof/acpi/nc110-DSDT.aml)
const SLP_TYP_S3: u16 = 5; // \_S3 = {5, 5}

fn rtc(r: u8) -> u8 {
    unsafe {
        outb(0x70, r);
        inb(0x71)
    }
}
fn rtc_w(r: u8, v: u8) {
    unsafe {
        outb(0x70, r);
        outb(0x71, v)
    }
}

/// CMOS RTC alarm `minutes` from now (hour/minute/second alarm registers, alarm interrupt enabled, flags cleared).
fn arm_rtc_alarm(minutes: u32) -> Result<(u8, u8), &'static str> {
    let mut ok = false;
    for _ in 0..100_000 {
        if rtc(0x0A) & 0x80 == 0 {
            ok = true;
            break;
        }
    }
    if !ok {
        return Err("RTC update never settled");
    }
    let b = rtc(0x0B);
    let bin = b & 0x04 != 0;
    let h24 = b & 0x02 != 0;
    let dec = |v: u8| if bin { v } else { (v & 0x0F) + (v >> 4) * 10 };
    let enc = |v: u8| if bin { v } else { (v / 10) << 4 | v % 10 };
    if !h24 {
        return Err("RTC in 12-hour mode (not expected on the NC110)");
    }
    // read until two consecutive reads agree (an update between registers would mix minutes and hours)
    let read = || {
        for _ in 0..100_000 {
            if rtc(0x0A) & 0x80 == 0 {
                break;
            }
        }
        (dec(rtc(0x00)), dec(rtc(0x02)), dec(rtc(0x04)))
    };
    let mut now = read();
    for _ in 0..10 {
        let again = read();
        if again == now {
            break;
        }
        now = again;
    }
    let (s, m, h) = now;
    let total = (h as u32 * 60 + m as u32 + minutes + 1) % (24 * 60); // +1: never "now", always a future match
    let (ah, am) = ((total / 60) as u8, (total % 60) as u8);
    rtc_w(0x01, enc(s));
    rtc_w(0x03, enc(am));
    rtc_w(0x05, enc(ah));
    // FADT RTC Day Alarm Index = 0x0D: the date-alarm field (bits 5:0) must be 0 = "don't care", or a stale date
    // would also have to match and the alarm might never fire (bit 7 VRT is read-only, bit 6 reserved: kept)
    rtc_w(0x0D, rtc(0x0D) & 0xC0);
    let _ = rtc(0x0C); // clear pending flags
    rtc_w(0x0B, b | 0x20); // AIE
    Ok((ah, am))
}

/// Enter S3. Returns only if the platform did not go to sleep (the caller keeps serving bmctl).
pub fn enter(minutes: u32, nic: &Rtl) -> Result<(), &'static str> {
    let (dsdt, len) = acpi::table(b"DSDT").ok_or("no DSDT")?;
    let ck = unsafe { read_volatile((dsdt + 9) as *const u8) };
    if (len, ck) != NC110_DSDT {
        return Err("not the NC110 DSDT this sleep path was written for");
    }
    let (fadt, _) = acpi::table(b"FACP").ok_or("no FADT")?;
    let (facs, _) = acpi::table(b"FACS").ok_or("no FACS")?;
    let rd = |o: u64| unsafe { read_volatile((fadt + o) as *const u32) };
    let smi_cmd = rd(48) as u16;
    let acpi_enable = rd(52) as u8;
    let pm1a_evt = rd(56) as u16;
    let pm1a_cnt = rd(64) as u16;
    let gpe0 = rd(80) as u16;
    let evt_len = (rd(88) & 0xFF) as u16;
    let gpe_len = ((rd(92) & 0xFF) / 2) as u16;
    if pm1a_cnt == 0 || pm1a_evt == 0 || evt_len != 4 || gpe_len != 4 || rd(68) != 0 {
        return Err("FADT is not the expected single-PM1A/8-byte-GPE0 layout");
    }
    if !mb::ram_contains(STUB_AT, 0x1000) {
        return Err("wake stub page is not RAM");
    }
    // 1. wake stub + FACS waking vector (32-bit real-mode vector; the 64-bit X vector off)
    unsafe {
        core::ptr::copy_nonoverlapping(STUB.as_ptr(), STUB_AT as *mut u8, STUB.len());
        write_volatile((facs + 12) as *mut u32, STUB_AT as u32);
        write_volatile((facs + 24) as *mut u64, 0);
    }
    // 2. ACPI mode (SCI_EN), as an OS does before sleeping
    unsafe {
        if inw(pm1a_cnt) & 1 == 0 && smi_cmd != 0 {
            outb(smi_cmd, acpi_enable);
            for _ in 0..300 {
                if inw(pm1a_cnt) & 1 != 0 {
                    break;
                }
                wdt::delay_ms(10);
            }
        }
    }
    let sci = unsafe { inw(pm1a_cnt) } & 1;
    // 3. wake sources: RTC alarm (always), magic packet
    let (ah, am) = arm_rtc_alarm(minutes.clamp(2, 12 * 60))?;
    let wol = nic.arm_wol();
    let pmcsr = wol.ok();
    // 4. _PTS(3), as this DSDT does it: debug port, SMI mailbox (BCMD 0x4C via port 0xFE00), EC trap (SECI 0xA4, port 0x5000)
    unsafe {
        outb(0x80, 3);
        write_volatile(0x3F5C_2EBD as *mut u8, 0x4C);
        outb(0xFE00, 0); // _PTS(3) leaves BCMD as written
        write_volatile(0x3F5C_0C7D as *mut u8, 0xA4);
        outb(0x5000, 0);
    }
    // 5. status clear (RW1C), enables: PM1 RTC_EN (10) + PWRBTN_EN (8), PCIEXP_WAKE_DIS (14) clear; GPE 9 (PCIe wake)
    unsafe {
        outw(pm1a_evt, 0xFFFF);
        outw(pm1a_evt + 2, (1 << 10) | (1 << 8));
        outl(gpe0, 0xFFFF_FFFF);
        outl(gpe0 + gpe_len, inl(gpe0 + gpe_len) | (1 << 9));
    }
    kprintln!("sleep: S3 now (SCI_EN {}, RTC alarm {:02}:{:02}, WoL {:?}, wake = fresh boot via {:#x})", sci, ah, am, wol, STUB_AT);
    crate::udplog::dump(6);
    wdt::pet();
    // 6. NIC to D3hot (PME armed: it can still assert wake), then SLP_TYP and SLP_EN, caches written back first.
    // From here the NIC is unusable for bmctl, so a failed entry must end in a reset, never a return.
    if let Some(pm) = pmcsr {
        let v = crate::pci::read32(nic.bdf, pm);
        crate::pci::write32(nic.bdf, pm, (v & !0x3 & !(1 << 15)) | 3);
    }
    unsafe {
        core::arch::asm!("wbinvd", options(nostack));
        let c = inw(pm1a_cnt) & !(7 << 10);
        outw(pm1a_cnt, c | (SLP_TYP_S3 << 10));
        outw(pm1a_cnt, c | (SLP_TYP_S3 << 10) | (1 << 13));
    }
    let t = time::now_ms();
    while time::now_ms() - t < 3000 {
        wdt::pet();
    }
    kprintln!("sleep: still running 3 s after SLP_EN: S3 not entered -> hard reset (the NIC is in D3)");
    crate::udplog::dump(6);
    wdt::hard_reset()
}
