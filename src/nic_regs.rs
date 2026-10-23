//! Register layouts of the two Realtek parts this kernel drives with the same 16-byte C+ descriptor rings.
//! Plain data with no dependencies, so `tools/nic_regs_test.rs` can check it on the host against the
//! pinned Linux v6.12 sources in third_party/linux-v6.12/ (file:line cited per field in `CITES_*`).
#![allow(dead_code)]

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Regs {
    pub name: &'static str,
    pub device: u16,
    pub mac0: u16,
    pub tx_ring: u16, // 64-bit, low dword here, high at +4
    pub chip_cmd: u16,
    pub tx_poll: u16,
    pub intr_mask: u16,
    pub intr_status: u16,
    pub tx_config: u16,
    pub rx_config: u16,
    pub cfg9346: u16,
    pub phyar: u16,     // 0 = no PHY access register (8139C+)
    pub phystatus: u16, // 0 = none
    pub rx_max_size: u16,
    pub cplus_cmd: u16,
    pub intr_mitigate: u16, // 0 = none
    pub rx_ring: u16,
    pub max_tx_packet_size: u16, // 0 = none
    // bits
    pub npq: u8,         // TxPoll: normal priority queue
    pub cmd_reset: u8,
    pub cmd_rx_enb: u8,
    pub cmd_tx_enb: u8,
    pub desc_own: u32,
    pub ring_end: u32,
    pub first_frag: u32,
    pub last_frag: u32,
    pub rx_err: u32,      // RX error summary bit in opts1
    pub rx_len_mask: u32, // RX frame length (incl. FCS) in opts1
}

/// QEMU `-device rtl8139` in C+ mode (Linux 8139cp.c). Port I/O BAR0 or MMIO BAR1.
pub const RTL8139CP: Regs = Regs {
    name: "rtl8139 C+ (10ec:8139)",
    device: 0x8139,
    mac0: 0x00,
    tx_ring: 0x20,
    chip_cmd: 0x37,
    tx_poll: 0xD9,
    intr_mask: 0x3C,
    intr_status: 0x3E,
    tx_config: 0x40,
    rx_config: 0x44,
    cfg9346: 0x50,
    phyar: 0,
    phystatus: 0,
    rx_max_size: 0xDA,
    cplus_cmd: 0xE0,
    intr_mitigate: 0,
    rx_ring: 0xE4,
    max_tx_packet_size: 0,
    npq: 1 << 6,
    cmd_reset: 0x10,
    cmd_rx_enb: 0x08,
    cmd_tx_enb: 0x04,
    desc_own: 1 << 31,
    ring_end: 1 << 30,
    first_frag: 1 << 29,
    last_frag: 1 << 28,
    rx_err: 1 << 20,
    rx_len_mask: 0x1FFF,
};

/// NC110 wired NIC, RTL8101E/8102E family (10ec:8136), Linux r8169_main.c. MMIO only in this kernel.
pub const RTL8101E: Regs = Regs {
    name: "RTL8101E family (10ec:8136)",
    device: 0x8136,
    mac0: 0x00,
    tx_ring: 0x20,
    chip_cmd: 0x37,
    tx_poll: 0x38,
    intr_mask: 0x3C,
    intr_status: 0x3E,
    tx_config: 0x40,
    rx_config: 0x44,
    cfg9346: 0x50,
    phyar: 0x60,
    phystatus: 0x6C,
    rx_max_size: 0xDA,
    cplus_cmd: 0xE0,
    intr_mitigate: 0xE2,
    rx_ring: 0xE4,
    max_tx_packet_size: 0xEC,
    npq: 0x40,
    cmd_reset: 0x10,
    cmd_rx_enb: 0x08,
    cmd_tx_enb: 0x04,
    desc_own: 1 << 31,
    ring_end: 1 << 30,
    first_frag: 1 << 29,
    last_frag: 1 << 28,
    rx_err: 1 << 21,
    rx_len_mask: 0x3FFF,
};

// r8169 values used only by the 8101E init path (r8169_main.c v6.12).
pub const CFG9346_UNLOCK: u8 = 0xC0;
pub const CFG9346_LOCK: u8 = 0x00;
pub const CPCMD_RXVLAN: u16 = 1 << 6;
pub const CPCMD_RXCHKSUM: u16 = 1 << 5;
pub const CPCMD_MASK: u16 = (1 << 13) | (1 << 6) | (1 << 5) | 0x3; // Normal_mode|RxVlan|RxChkSum|INTT_MASK
pub const TX_PACKET_MAX: u8 = (8064 >> 7) as u8;
pub const TX_DMA_BURST: u32 = 7; // << TxDMAShift (8)
pub const INTER_FRAME_GAP: u32 = 0x03; // << TxInterFrameGapShift (24)
pub const RX_FIFO_THRESH: u32 = 7 << 13;
pub const RX_DMA_BURST: u32 = 7 << 8;
pub const RX128_INT_EN: u32 = 1 << 15;
pub const ACCEPT_BROADCAST: u32 = 0x08;
pub const ACCEPT_MY_PHYS: u32 = 0x02;
pub const PHYAR_FLAG: u32 = 0x8000_0000;
pub const LINK_STATUS: u8 = 0x02;

/// (field, value, file, line, identifier on that line). The host test parses the value on that line.
pub const CITES_8101E: &[(&str, u32, &str, usize, &str)] = &[
    ("mac0", RTL8101E.mac0 as u32, "r8169_main.c", 173, "MAC0"),
    ("tx_ring", RTL8101E.tx_ring as u32, "r8169_main.c", 178, "TxDescStartAddrLow"),
    ("chip_cmd", RTL8101E.chip_cmd as u32, "r8169_main.c", 184, "ChipCmd"),
    ("tx_poll", RTL8101E.tx_poll as u32, "r8169_main.c", 185, "TxPoll"),
    ("intr_mask", RTL8101E.intr_mask as u32, "r8169_main.c", 186, "IntrMask"),
    ("intr_status", RTL8101E.intr_status as u32, "r8169_main.c", 187, "IntrStatus"),
    ("tx_config", RTL8101E.tx_config as u32, "r8169_main.c", 189, "TxConfig"),
    ("rx_config", RTL8101E.rx_config as u32, "r8169_main.c", 193, "RxConfig"),
    ("cfg9346", RTL8101E.cfg9346 as u32, "r8169_main.c", 205, "Cfg9346"),
    ("phyar", RTL8101E.phyar as u32, "r8169_main.c", 214, "PHYAR"),
    ("phystatus", RTL8101E.phystatus as u32, "r8169_main.c", 215, "PHYstatus"),
    ("rx_max_size", RTL8101E.rx_max_size as u32, "r8169_main.c", 216, "RxMaxSize"),
    ("cplus_cmd", RTL8101E.cplus_cmd as u32, "r8169_main.c", 217, "CPlusCmd"),
    ("intr_mitigate", RTL8101E.intr_mitigate as u32, "r8169_main.c", 218, "IntrMitigate"),
    ("rx_ring", RTL8101E.rx_ring as u32, "r8169_main.c", 228, "RxDescAddrLow"),
    ("max_tx_packet_size", RTL8101E.max_tx_packet_size as u32, "r8169_main.c", 234, "MaxTxPacketSize"),
    ("npq", RTL8101E.npq as u32, "r8169_main.c", 387, "NPQ"),
    ("cmd_reset", RTL8101E.cmd_reset as u32, "r8169_main.c", 380, "CmdReset"),
    ("cmd_rx_enb", RTL8101E.cmd_rx_enb as u32, "r8169_main.c", 381, "CmdRxEnb"),
    ("cmd_tx_enb", RTL8101E.cmd_tx_enb as u32, "r8169_main.c", 382, "CmdTxEnb"),
    ("desc_own", RTL8101E.desc_own, "r8169_main.c", 484, "DescOwn"),
    ("ring_end", RTL8101E.ring_end, "r8169_main.c", 485, "RingEnd"),
    ("first_frag", RTL8101E.first_frag, "r8169_main.c", 486, "FirstFrag"),
    ("last_frag", RTL8101E.last_frag, "r8169_main.c", 487, "LastFrag"),
    ("rx_err (RxRES)", RTL8101E.rx_err, "r8169_main.c", 374, "RxRES"),
    ("rx_len_mask", RTL8101E.rx_len_mask, "r8169_main.c", 4630, "pkt_size"),
    ("CFG9346_UNLOCK", CFG9346_UNLOCK as u32, "r8169_main.c", 392, "Cfg9346_Unlock"),
    ("CFG9346_LOCK", CFG9346_LOCK as u32, "r8169_main.c", 391, "Cfg9346_Lock"),
    ("CPCMD_RXVLAN", CPCMD_RXVLAN as u32, "r8169_main.c", 455, "RxVlan"),
    ("CPCMD_RXCHKSUM", CPCMD_RXCHKSUM as u32, "r8169_main.c", 456, "RxChkSum"),
    ("TX_PACKET_MAX", TX_PACKET_MAX as u32, "r8169_main.c", 236, "TxPacketMax"),
    ("TX_DMA_BURST", TX_DMA_BURST, "r8169_main.c", 61, "TX_DMA_BURST"),
    ("INTER_FRAME_GAP", INTER_FRAME_GAP, "r8169_main.c", 62, "InterFrameGap"),
    ("RX_FIFO_THRESH", RX_FIFO_THRESH, "r8169_main.c", 198, "RX_FIFO_THRESH"),
    ("RX_DMA_BURST", RX_DMA_BURST, "r8169_main.c", 203, "RX_DMA_BURST"),
    ("RX128_INT_EN", RX128_INT_EN, "r8169_main.c", 194, "RX128_INT_EN"),
    ("ACCEPT_BROADCAST", ACCEPT_BROADCAST, "r8169_main.c", 398, "AcceptBroadcast"),
    ("ACCEPT_MY_PHYS", ACCEPT_MY_PHYS, "r8169_main.c", 400, "AcceptMyPhys"),
    ("PHYAR_FLAG", PHYAR_FLAG, "r8169_main.c", 1151, "PHYAR"),
    ("LINK_STATUS", LINK_STATUS as u32, "r8169_main.c", 469, "LinkStatus"),
];

pub const CITES_8139CP: &[(&str, u32, &str, usize, &str)] = &[
    ("mac0", RTL8139CP.mac0 as u32, "8139cp.c", 138, "MAC0"),
    ("tx_ring", RTL8139CP.tx_ring as u32, "8139cp.c", 141, "TxRingAddr"),
    ("chip_cmd", RTL8139CP.chip_cmd as u32, "8139cp.c", 143, "Cmd"),
    ("intr_mask", RTL8139CP.intr_mask as u32, "8139cp.c", 144, "IntrMask"),
    ("intr_status", RTL8139CP.intr_status as u32, "8139cp.c", 145, "IntrStatus"),
    ("tx_config", RTL8139CP.tx_config as u32, "8139cp.c", 146, "TxConfig"),
    ("rx_config", RTL8139CP.rx_config as u32, "8139cp.c", 148, "RxConfig"),
    ("cfg9346", RTL8139CP.cfg9346 as u32, "8139cp.c", 150, "Cfg9346"),
    ("tx_poll", RTL8139CP.tx_poll as u32, "8139cp.c", 162, "TxPoll"),
    ("rx_max_size", RTL8139CP.rx_max_size as u32, "8139cp.c", 163, "RxMaxSize"),
    ("cplus_cmd", RTL8139CP.cplus_cmd as u32, "8139cp.c", 164, "CpCmd"),
    ("rx_ring", RTL8139CP.rx_ring as u32, "8139cp.c", 166, "RxRingAddr"),
    ("desc_own", RTL8139CP.desc_own, "8139cp.c", 172, "DescOwn"),
    ("ring_end", RTL8139CP.ring_end, "8139cp.c", 173, "RingEnd"),
    ("first_frag", RTL8139CP.first_frag, "8139cp.c", 174, "FirstFrag"),
    ("last_frag", RTL8139CP.last_frag, "8139cp.c", 175, "LastFrag"),
    ("rx_err (RxError)", RTL8139CP.rx_err, "8139cp.c", 180, "RxError"),
    ("npq (NormalTxPoll)", RTL8139CP.npq as u32, "8139cp.c", 189, "NormalTxPoll"),
    ("rx_len_mask", RTL8139CP.rx_len_mask, "8139cp.c", 487, "len"),
];

/// True if the NIC register window [base, base+256) cannot overlap any storage-controller memory BAR.
/// A BAR is naturally aligned to its size, so a BAR at `s` spans at most [s, s + lowest_set_bit(s)): an
/// upper bound on its size obtained by READING the base only (sizing would need a config write to the
/// storage controller, which this kernel never does).
pub fn clear_of_storage(base: u64, storage: &[u64]) -> bool {
    storage.iter().all(|&s| {
        let span = 1u64 << s.trailing_zeros().min(32);
        base + 0x100 <= s || s + span <= base
    })
}
