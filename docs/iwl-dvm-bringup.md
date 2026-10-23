# Intel Centrino Wireless-N 130 (8086:0896) — bare-metal DVM bring-up recipe

Traced from Linux 6.18.44, `drivers/net/wireless/intel/iwlwifi/` (all paths below are
relative to that directory). Firmware: `iwlwifi-6000g2b-6.ucode` (v18.168.6.1), whose TLV
headers were dumped from `~/nc110-baremetal/fw/iwlwifi-6000g2b-6.ucode`
(results in §7.1).

Conventions:

- `CSR[x]` means a 32-bit MMIO access at BAR0+x. `PRPH[x]` is a periphery register reached
  through `HBUS_TARG_PRPH_*` (§2.2). `SRAM[x]` is device SRAM reached through `HBUS_TARG_MEM_*` (§2.3).
- "set bits M in R" means read-modify-write `R = R | M`. "clear" means `R = R & ~M`.
  "set_bits_mask(R, mask, val)" means `R = (R & ~mask) | val` (`pcie/utils.h:13-26`).
- `poll(R, bits, mask, T)` means: loop reading R until `(R & mask) == (bits & mask)`, 10 µs
  per step, give up after T µs (`iwl-io.c:48-63`). `poll_bits(R, b, T)` is the same with mask = b
  (`iwl-io.h:28-31`).
- Every value is the one Linux uses on this device's real path. Anything I could not prove from
  source is marked **UNVERIFIED**.

---

## 0. Which config Linux picks for 0x0896

| What | Value | Source |
|---|---|---|
| PCI ID table | `0x0896` with subsystem `0x5005, 0x5007, 0x5025, 0x5027` → `iwl6030_mac_cfg`. Other subsystem IDs do not bind. | `pcie/drv.c:183-189` |
| `iwl6030_mac_cfg` | `.device_family = IWL_DEVICE_FAMILY_6030`, `.base = &iwl6000_g2_base` | `cfg/6000.c:142-145` |
| RF cfg | `IWL_DEV_INFO` table, searched **from the end backwards** (`pcie/drv.c:1104`). `SUBDEV_MASKED(0x5, 0xF)` = `(subsys & 0xF) == 5` → `iwl130_bgn_cfg`; `(subsys & 0xF) == 7` → `iwl130_bg_cfg` | `pcie/drv.c:750-753`, macro `pcie/drv.c:575-577`, matcher `pcie/drv.c:1112-1117` |
| `iwl130_bgn_cfg` | `IWL_DEVICE_6030` + `ht_params{greenfield, use_rts_for_aggregation=true, ht40_bands=2G|5G}`, `rx_with_siso_diversity=true` | `cfg/6000.c:202-210` |
| `iwl130_bg_cfg` | `IWL_DEVICE_6030` + `rx_with_siso_diversity=true` (no HT params) | `cfg/6000.c:214-217` |
| `IWL_DEVICE_6030` | `fw_name_pre "iwlwifi-6000g2b"`, API max 6 / min 5, `max_inst_size = IWL60_RTC_INST_SIZE = 0x40000`, `max_data_size = IWL60_RTC_DATA_SIZE = 0x14000`, `nvm_ver 0x709`, `nvm_calib_ver 6`, `eeprom_params = &iwl6000_eeprom_params` | `cfg/6000.c:147-156`, `iwl-agn-hw.h:23-30`, `cfg/6000.c:35-36` |
| `iwl6000_g2_base` | `eeprom_size = 2048` (OTP_LOW_IMAGE_SIZE_2K), `num_of_queues = 20`, `max_tfd_queue_size = 256`, `max_ll_items = 4`, `shadow_ram_support = 1`, `shadow_reg_enable = 0`, `scd_chain_ext_wa = 1`, `wd_timeout = 10000` | `cfg/6000.c:78-89`, `iwl-config.h:316`, `iwl-agn-hw.h:52,57` |
| DVM lib | `IWL_DEVICE_FAMILY_6030` → `iwl_dvm_6030_cfg`: `nic_config = iwl6000_nic_config`, `bt_params = &iwl6000_bt_params` (advanced BT coex, `bt_sco_disable`, `bt_prio_boost 0xF0`, **no** `bt_session_2`), `need_temp_offset_calib = 1` (**not** `temp_offset_v2`), `adv_pm = 1`, `support_ct_kill_exit = 1`; `no_xtal_calib` not set | `dvm/main.c:1303-1305`, `dvm/devices.c:654-675` |

Fields that are **zero** for this device because neither `iwl6000_g2_base` (`cfg/6000.c:78-89`)
nor `IWL_DEVICE_6030` / `iwl130_*_cfg` (`cfg/6000.c:147-156, 202-217`) set them (definitions:
`iwl-config.h:183-209` and `iwl-config.h:406-452`):

| Field | Consequence |
|---|---|
| `pll_cfg = 0` | no `CSR_ANA_PLL_CFG` write in APM init (`pcie/gen1_2/trans.c:267-268`) |
| `apmg_not_supported = 0` | all APMG writes happen (`trans.c:187, 303, 1218`) |
| `apmg_wake_up_wa = 0` | no MAC_ACCESS hold around host commands (`pcie/gen1_2/tx.c:1016-1017, 197-198`) |
| `pcie_l1_allowed = 0` | Linux calls `pci_disable_link_state(L0S|L1|CLKPM)` (`trans.c:3856-3865`) |
| `min_txq_size = min_ba_txq_size = 0` | cmd queue = 32 slots, data queues = 256 slots (`tx.c:854-859`, `fw/api/txq.h:91,93`) |
| `host_interrupt_operation_mode = 0`, `lp_xtal_workaround = 0` | no OSC_CLK dance (`trans.c:274-294`), no LP-XTAL path on stop (`trans.c:471-474`) |
| `mq_rx_supported = 0`, `gen2 = 0` | legacy single RX queue, 128-byte TFDs, 36-bit DMA (`trans.c:3777-3781`, `rx.c:836-901`) |
| `trans->conf.scd_set_active = 0`, `wide_cmd_header = 0` | DVM never sets them (search of `dvm/` finds no writer; users at `tx.c:1168-1170, 1246-1248, 1365`) |

---

## 1. PCI-side setup

Linux's PCI-config-space actions for this device, in order:

1. If BAR0 reads back as just `PCI_BASE_ADDRESS_MEM_TYPE_64` (0x4), re-assign it (`pcie/drv.c:1149-1154`). BAR0 is a 64-bit memory BAR. Map it (Linux maps the whole BAR, `drv.c:1168`).
2. Enable memory decoding (`pcim_enable_device`, `drv.c:1156`) and **bus master** (`pci_set_master`, `drv.c:1160`): PCI_COMMAND (cfg 0x04) |= 0x0002 (MEM) | 0x0004 (BUS MASTER).
3. Read `CSR_HW_REV` (BAR0+0x028). `0xFFFFFFFF` means the device is not reachable (`drv.c:1175-1179`). The low bits feed `hw_rev_step = (hw_rev & 0xC) >> 2` (`trans.c:3891-3894`) and NIC config (§4.1). For this family `CSR_HW_REV_TYPE(hw_rev) == 0xB` (`CSR_HW_REV_TYPE_6x30 = 0xB0`, `iwl-csr.h:341-342`; type macro `iwl-csr.h:310`).
4. ASPM: `pcie_l1_allowed = 0` → `pci_disable_link_state(pdev, L0S | L1 | CLKPM)` (`trans.c:3856-3865`). Bare-metal equivalent: in the device's PCIe capability Link Control register clear bits [1:0] (ASPM control) and bit 8 (Enable Clock PM). That bit mapping is from the PCIe spec, not from iwlwifi; the kernel helper also touches the upstream port (**UNVERIFIED** whether the parent needs it on your platform).
5. DMA mask 36 bits (`trans.c:3867-3876`, TFD address width 36 `trans.c:3778`). With identity-mapped memory below 4 GiB the high nibble is always 0.
6. **Retry-timeout byte: config offset `0x41` := `0x00`** (`trans.c:3878-3880`; also on resume `drv.c:1218-1222`; offset defined `trans.c:201`, `drv.c:1092`).
7. Interrupts: the 6030 has `mq_rx_supported = 0`, so Linux skips MSI-X and calls `pci_enable_msi` (`trans.c:1604-1605, 1657-1668`). **Not needed for polling.** One quirk: if MSI fails, Linux clears `PCI_COMMAND_INTX_DISABLE` "to enable rfkill interrupt: hw bug w/a" (`trans.c:1660-1667`). With polling you can leave INTx disabled.
8. Early probe touch (`trans.c:4189-4202`): `prepare_card_hw` (§2.1) → `activate_nic` (§3) → grab/release NIC access. Optional for a port; the real init repeats it.

Also read once for later use (`trans.c:216-220`): PCIe Link Control. `pm_support = !(LNKCTL & ASPM_L0S)`. After step 4 L0s is off, so `pm_support = true`, which later sets `IWL_POWER_PCI_PM_MSK` in POWER_TABLE_CMD (§9.6).

---

## 2. NIC access protocol

### 2.1 Taking ownership: `prepare_card_hw` (`pcie/gen1_2/trans.c:522-598`)

```
set bits 0x00400000 (HW_IF_CONFIG_REG_PCI_OWN_SET) in CSR[0x000]
poll_bits(CSR[0x000], 0x00400000, 50 µs)               # HW_READY_TIMEOUT = 50
if ok: set bits 0x20 (MBOX_SET_REG_OS_ALIVE) in CSR[0x088]; done
else:
  set bits 0x80000000 (RESET_LINK_PWR_MGMT_DISABLED) in CSR[0x250]; sleep 1-2 ms
  repeat up to 10 times:
     set bits 0x08000000 (HW_IF_CONFIG_REG_WAKE_ME) in CSR[0x000]
     for up to 150 ms (200 µs steps): retry the PCI_OWN_SET step above
     sleep 25 ms
```

Registers: `CSR_HW_IF_CONFIG_REG = 0x000`, `CSR_MBOX_SET_REG = 0x088`, `CSR_DBG_LINK_PWR_MGMT_REG = 0x250`
(`iwl-csr.h:29, 86, 152, 182, 185, 189, 246`). The MEI/CSME branch (`trans.c:577-587`) only matters
when Intel ME owns the device.

### 2.2 Grab / release NIC access (`trans.c:2330-2457`)

Grab (pre-BZ path):

```
set bits 0x00000008 (GP_CNTRL_REG_FLAG_MAC_ACCESS_REQ) in CSR[0x024]
poll(CSR[0x024], bits = 0x00000001, mask = 0x00000011, 15000 µs)
     # wait until MAC_CLOCK_READY (bit0) = 1 AND GOING_TO_SLEEP (bit4) = 0
on timeout Linux writes CSR[0x020] = 0x00000002 (FORCE_NMI) and fails
```

Release: clear bits 0x00000008 in CSR[0x024] (`trans.c:2446-2447`).

No udelay after the request for this family (that's 8000+ only, `trans.c:2355-2356`).
Hold access around every PRPH and SRAM access, and around FH register blocks Linux brackets
(`iwl_read_direct32`/`iwl_write_direct32`, `iwl-io.c:66-86`).

### 2.3 Periphery (PRPH) access (`trans.c:1881-1905`), with NIC access held

```
read:  CSR[0x448] (HBUS_TARG_PRPH_RADDR) = (reg & 0x000FFFFF) | (3 << 24);  val = CSR[0x450] (RDAT)
write: CSR[0x444] (HBUS_TARG_PRPH_WADDR) = (reg & 0x000FFFFF) | (3 << 24);  CSR[0x44C] (WDAT) = val
```

Mask 0x000FFFFF for pre-AX210 (`trans.c:1883-1886`). Addresses: `iwl-csr.h:539-542`.
`iwl_set_bits_prph` / `iwl_clear_bits_prph` / `iwl_set_bits_mask_prph` are a read-modify-write
under one grab (`iwl-io.c:171-200`). `iwl_set_bits_mask_prph(ofs, bits, mask)` writes
`(read & mask) | bits`. Note the mask here is "bits to keep", the opposite of the CSR helper.

### 2.4 Device SRAM (TARG_MEM), with NIC access held

```
read  N dwords at A:  CSR[0x40C] (HBUS_TARG_MEM_RADDR) = A;  then N × read CSR[0x41C] (RDAT)
write N dwords at A:  CSR[0x410] (HBUS_TARG_MEM_WADDR) = A;  then N × write CSR[0x418] (WDAT)
```

The address is written once and the data register auto-increments by 4 per access
(`trans.c:2472-2490`, `iwl-trans.c:470-475`). A write with a NULL buffer writes zeros (`iwl-trans.c:473-474`).
Addresses: `iwl-csr.h:522-525`.

### 2.5 Shared (SHR) indirect registers

`CSR[0x0F4] = val; CSR[0x0EC] = (reg & 0xFFFF) | (3 << 28)` for writes (`trans.c:178-183`).
Only used by the LP-XTAL path, which this device does not take (`lp_xtal_workaround = 0`).

### 2.6 RF-kill

HW RF-kill is asserted when `CSR[0x024] & 0x08000000` (GP_CNTRL `HW_RF_KILL_SW`) is **0**
(`pcie/gen1_2/internal.h:1051-1062`, `iwl-csr.h:297`). If set to kill, Linux refuses to start
firmware (`trans.c:1314-1319`).

---

## 3. Power-up: `start_hw` and APM init

### 3.1 `iwl_trans_pcie_start_hw` (`trans.c:1786-1832`)

1. `prepare_card_hw` (§2.1).
2. Clear persistence bit: no-op for this family (`trans.c:1738-1747`, only 9000/22000).
3. **SW reset**: set bits 0x00000080 (`CSR_RESET_REG_FLAG_SW_RESET`) in `CSR[0x020]` (CSR_RESET); sleep 5-6 ms; then `prepare_card_hw` again (`trans.c:78-95`).
4. **APM init** (§3.2).
5. `iwl_pcie_init_msix` (no-op without MSI-X), enable the RF-kill interrupt mask: `CSR[0x00C]` (CSR_INT_MASK) = 0x00000080 (`internal.h:1022-1030`).
6. Check RF-kill (§2.6).

### 3.2 `iwl_pcie_apm_init` (`trans.c:231-320`), exact order for this device

| # | Access | Value | Source |
|---|---|---|---|
| 1 | set bits in `CSR[0x100]` (GIO_CHICKEN_BITS) | `0x20000000` (DIS_L0S_EXIT_TIMER) — family < 8000 | `trans.c:243-245`, `iwl-csr.h:450` |
| 2 | set bits in `CSR[0x100]` | `0x00800000` (L1A_NO_L0S_RX) | `trans.c:251-252`, `iwl-csr.h:449` |
| 3 | set bits in `CSR[0x240]` (DBG_HPET_MEM_REG) | `0xFFFF0000` | `trans.c:255`, `iwl-csr.h:151,461` |
| 4 | set bits in `CSR[0x000]` (HW_IF_CONFIG) | `0x00080000` (HAP_WAKE) | `trans.c:261-262`, `iwl-csr.h:179` |
| 5 | `apm_config`: set bits in `CSR[0x03C]` (GIO_REG) | `0x00000002` (L0S_DISABLED); read LNKCTL/DEVCTL2 for `pm_support`/LTR | `trans.c:203-224`, `iwl-csr.h:73,402` |
| 6 | (skipped: `pll_cfg = 0`) | — | `trans.c:267-268` |
| 7 | `activate_nic`: set bits in `CSR[0x024]` | `0x00000004` (INIT_DONE) | `trans.c:4332-4334` |
| 8 | `poll_bits(CSR[0x024], 0x00000001 MAC_CLOCK_READY, 25000 µs)` | — | `trans.c:4345` |
| 9 | (skipped: `host_interrupt_operation_mode = 0`) | — | `trans.c:274-294` |
| 10 | `PRPH[0x3004]` (APMG_CLK_EN_REG) := | `0x00000200` (DMA_CLK_RQT); then udelay 20 | `trans.c:304-306`, `iwl-prph.h:21,32` |
| 11 | set bits in `PRPH[0x3010]` (APMG_PCIDEV_STT_REG) | `0x00000800` (L1_ACT_DIS) | `trans.c:309-310`, `iwl-prph.h:24,44` |
| 12 | `PRPH[0x301C]` (APMG_RTC_INT_STT_REG) := | `0x10000000` (clear RF-kill int in APMG) | `trans.c:313-314`, `iwl-prph.h:26,47` |

Steps 10-12 need NIC access (they go through §2.3).

### 3.3 Power source (`iwl_pcie_set_pwr(trans, false)`, `trans.c:185-198`)

Called from `nic_init` right after APM init (`trans.c:498`) with `vaux = false`:

```
PRPH[0x300C] (APMG_PS_CTRL_REG) = (PRPH[0x300C] & ~0x03000000) | 0x00000000   # VMAIN
```

(`iwl-prph.h:23, 37-38`).

### 3.4 Stopping the device (needed between INIT and runtime, §9.2) — `_iwl_trans_pcie_stop_device` (`trans.c:1184-1267`)

1. Interrupts off: `CSR[0x00C] = 0`, `CSR[0x008] = 0xFFFFFFFF`, `CSR[0x010] = 0xFFFFFFFF` (`internal.h:835-842`).
2. TX stop (`tx.c:609-618`, `576-604`): `PRPH[0xA02C10]` (SCD_TXFACT) = 0; with NIC access, for ch 0..7 `CSR[0x1D00 + 0x20*ch] = 0`; `poll_bits(CSR[0x1EB0] FH_TSSR_TX_STATUS_REG, 0x00FF0000, 5000 µs)`.
3. RX stop (`rx.c:156-160`): `CSR[0x1C00] = 0` (direct); `poll` until `CSR[0x1C44] & 0x01000000` (RX channel idle), 1000 µs.
4. `PRPH[0x3008]` (APMG_CLK_DIS_REG) = 0x00000200; udelay 5 (`trans.c:1218-1222`).
5. Clear 0x00000008 (MAC_ACCESS_REQ) in `CSR[0x024]` (`trans.c:1230-1231`).
6. `apm_stop(false)` (`trans.c:440-483`): stop master — set bits 0x00000200 (STOP_MASTER) in `CSR[0x020]`, `poll_bits(CSR[0x020], 0x00000100 MASTER_DISABLED, 100 µs)` (`trans.c:427-432`); SW reset (`CSR[0x020] |= 0x80`, sleep 5 ms, no re-own); clear 0x00000004 (INIT_DONE) in `CSR[0x024]`.
7. SW reset again with re-own (`trans.c:1237`) → §2.1.
8. Interrupts off again, RF-kill mask on (`trans.c:1255-1266`).

---

## 4. NIC config and EEPROM/OTP

### 4.1 `iwl_nic_config` (DVM, `dvm/main.c:1997-2045`)

Called from `nic_init` right after `set_pwr` (`trans.c:500`), so on **every** firmware start.
It needs `radio_cfg_*` from the EEPROM, so the EEPROM must be read first (§4.2).

| # | Access | Value | Source |
|---|---|---|---|
| 1 | set_bits_mask(`CSR[0x000]`, mask `0x0000000F`, `hw_rev & 0xF`) | MAC step/dash | `dvm/main.c:2002-2004`, `iwl-csr.h:162,309` |
| 2 | if `radio_cfg_type <= 3`: set_bits_mask(`CSR[0x000]`, mask `0x0000FC00`, `type<<10 \| step<<14 \| dash<<12`) | from EEPROM word 0x48 (§4.3) | `dvm/main.c:2007-2020`, masks `iwl-csr.h:168-177`, max `dvm/main.c:1995` |
| 3 | set bits in `CSR[0x000]` | `0x00000300` (RADIO_SI \| MAC_SI) | `dvm/main.c:2031-2033`, `iwl-csr.h:165-166` |
| 4 | `PRPH[0x300C] = (PRPH[0x300C] & ~0x00400000) \| 0x00400000` | EARLY_PWR_OFF_RESET_DIS | `dvm/main.c:2039-2041`, `iwl-prph.h:35` |
| 5 | `iwl6000_nic_config`: for `IWL_DEVICE_FAMILY_6030` **nothing** (no `CSR_GP_DRIVER_REG` write) | — | `dvm/devices.c:484-490` |

So there is no GP_DRIVER (`CSR[0x050]`) write for 6000g2b/6030. The 2x2-IPA and
CALIB_VERSION6 bits are for 6000i/6050/6150 only (`dvm/devices.c:491-509`).

### 4.2 Reading the NVM (`iwl_read_eeprom`, `dvm/eeprom.c:918-1022`)

Linux reads it during op-mode start: `start_hw` (§3.1) → read → `stop_device` (`dvm/main.c:1389-1402`).
Size `sz = 2048` bytes (`eeprom_size`, `cfg/6000.c:79`).

1. **EEPROM or OTP?** `hw_rev` type is not 5x00, so read `CSR[0x034]` (CSR_OTP_GP_REG): bit16 (`0x00010000` DEVICE_SELECT) set → OTP (`dvm/eeprom.c:743-763`, `iwl-csr.h:388`). Expect OTP on this part (**UNVERIFIED** for your unit; read the bit).
2. **Signature**: `gp = CSR[0x030] & 7` (CSR_EEPROM_GP). OTP needs `gp == 1`; EEPROM needs `gp == 2 or 4`; anything else fails (`dvm/eeprom.c:699-727`, `iwl-csr.h:380-385`).
3. **Semaphore**: up to 1000 times: set bits `0x00200000` (EEPROM_OWN_SEM) in `CSR[0x000]`, `poll_bits(..., 0x00200000, 10 µs)` (`dvm/eeprom.c:664-691`).
4. **OTP only** (`dvm/eeprom.c:956-975`):
   - `iwl_init_otp_access` (`dvm/eeprom.c:765-788`): `activate_nic` (§3.2 steps 7-8); set bits `0x04000000` (APMG_PS_CTRL_VAL_RESET_REQ) in `PRPH[0x300C]`; udelay 5; clear them; then (shadow_ram_support = 1) set bits `0x80000000` in `CSR[0x250]`.
   - `CSR[0x030] = CSR[0x030] & ~0x00000180` (clear IF_OWNER) (`iwl-csr.h:381`).
   - set bits `0x00300000` in `CSR[0x034]` (write-1 to clear both ECC status bits) (`iwl-csr.h:390-391`).
   - `shadow_ram_support = 1` → **no** linked-list walk; image starts at OTP address 0 (`dvm/eeprom.c:970-975`).
5. **Word read loop**, for byte address `a = 0, 2, 4, … 2046` (`dvm/eeprom.c:976-1004`, OTP word read `790-826`):
   ```
   CSR[0x02C] (CSR_EEPROM_REG) = (a << 1) & 0x0000FFFC
   poll_bits(CSR[0x02C], 0x00000001 READ_VALID, 5000 µs)
   word = CSR[0x02C] >> 16
   OTP only: if CSR[0x034] & 0x00200000 (uncorrectable ECC): set that bit, abort
             if CSR[0x034] & 0x00100000 (correctable):   set that bit, continue
   image[a/2] = word   (little-endian 16-bit)
   ```
6. Release semaphore: clear `0x00200000` in `CSR[0x000]` (`dvm/eeprom.c:693-697`).

### 4.3 What lives where in the 2048-byte image (`dvm/eeprom.c:16-82, 1058-1144`)

Offsets are byte offsets into the image. "Indirect" offsets add `link_word * 2` where `link_word`
is the 16-bit value at the given link location (`dvm/eeprom.c:157-205`).

| Item | Location | Source |
|---|---|---|
| MAC address (6 bytes) | `0x2A` (2*0x15), direct | `dvm/eeprom.c:44, 1076-1079` |
| Number of MACs | u16 at `0x98` (2*0x4C) | `:51, 1080-1081` |
| NVM version | u16 at `0x88` (2*0x44). Must be `>= 0x709` **or** calib version `>= 6` | `:47, 1123-1124`, check `dvm/main.c:1216-1224` |
| SKU cap | u16 at `0x8A` (2*0x45): bit4 2.4 GHz, bit5 5.2 GHz, bit6 11n, bit7 AMT, **bit8 IPAN** | `:48, 68-74, 1113-1119` |
| Radio config | u16 at `0x90` (2*0x48): bits[1:0] type, [3:2] step, [5:4] dash, [7:6] pnum, **[11:8] valid TX ant**, **[15:12] valid RX ant** (ANT_A=1, B=2, C=4) | `:50, 77-82, 1104-1111` |
| Link table | u16 word offsets at 0xC8 (HOST), 0xCA (GENERAL), 0xCC (REGULATORY), **0xCE (CALIBRATION)**, 0xD0 (PROCESS_ADJST), 0xD2 (OTHERS), 0xD4 (TXP_LIMIT), 0xD6 (TXP_LIMIT_SIZE) | `:32-39` |
| Calib header | at `(link[0xCE] << 1) + 0`: `u8 version; u8 pa_type; le16 voltage` | `:54-60, 218-230` |
| XTAL calib | 2×u16 at `(link[0xCE] << 1) + 0x250` (2*0x128) | `:61, 1086-1089` |
| Kelvin temp / voltage | 2×u16 at `(link[0xCE] << 1) + 0x254` (2*0x12A) | `:64, 1097-1102` |
| Raw temperature | u16 at `(link[0xCE] << 1) + 0x256` (2*0x12B) | `:65, 1091-1095` |
| Channel tables | `(link[0xCC] << 1) + {0x08 (2.4 GHz ch 1-14), 0x26, 0x42, 0x5C, 0x74, 0x80 (2.4 HT40), 0x92 (5 HT40)}`; each entry `{u8 flags; s8 max_power_avg}`; flags bit0 VALID, bit1 IBSS, bit3 ACTIVE, bit4 RADAR | `cfg/6000.c:91-102`, `iwl-config.h:305-312`, `dvm/eeprom.c:241-258, 416-462, 510-588` |
| Enhanced TX power | entries of 8 bytes at `link[0xD4] << 1`, count = `u16 at (link[0xD6] << 1)` * 2 / 8; `max_tx_pwr_half_dbm` = max over valid entries of the best chain value | `dvm/eeprom.c:285-294, 322-324, 357-414`, enabled by `cfg/6000.c:101` |

A channel is scannable if VALID; it is passive-only (`NO_IR`) unless both IBSS and ACTIVE are set
(`dvm/eeprom.c:536-562`).

PAN: `cmd_queue = 9` only if firmware has `IWL_UCODE_TLV_FLAGS_PAN` **and** EEPROM SKU bit8 (IPAN) is
set; otherwise `cmd_queue = 4` (`dvm/main.c:1352-1358, 1438-1447`). This firmware has the PAN flag
(§7.1), so **the EEPROM decides**. See §6.6.

---

## 5. RX

Done inside `nic_init` on every firmware start: `rx_init` then `tx_init` (`trans.c:502-511`).

### 5.1 Memory (legacy single-queue, `mq_rx_supported = 0`)

| Object | Size / alignment | Format | Source |
|---|---|---|---|
| RBD ring | 256 entries × 4 bytes = 1024 B, **256-byte aligned** | `le32 = rb_phys >> 8` (RB physical address bits [35:8]) | `rx.c:727, 659-666, 137-140, 322`; `iwl-fh.h:160-171, 564-566` |
| RB (receive buffer) | 4096 B each (`rx_buf_size = IWL_AMSDU_4K`, default for DVM), **256-byte aligned** (Linux uses pages) | packets, see §5.4 | `dvm/main.c:1325-1329`, `rx.c:841-844` |
| RB status | `struct iwl_rb_status`, 12 B used (Linux allocates `sizeof` = 12), **16-byte aligned** | `le16 closed_rb_num @0; le16 closed_fr_num @2; le16 finished_rb_num @4; le16 finished_fr_num @6; le32 spare @8` | `iwl-fh.h:581-587`, `rx.c:703-712, 797-801` |

### 5.2 Hardware init (`iwl_pcie_rx_hw_init`, `rx.c:836-901`), NIC access held

| # | Register | Value |
|---|---|---|
| 1 | `CSR[0x1C00]` FH_MEM_RCSR_CHNL0_CONFIG_REG | `0` (stop RX DMA) |
| 2 | `CSR[0x1C08]` FH_MEM_RCSR_CHNL0_RBDCB_WPTR | `0` |
| 3 | `CSR[0x1C10]` FH_MEM_RCSR_CHNL0_FLUSH_RB_REQ | `0` |
| 4 | `CSR[0x1BCC]` FH_RSCSR_CHNL0_RDPTR | `0` |
| 5 | `CSR[0x1BC8]` FH_RSCSR_CHNL0_RBDCB_WPTR_REG | `0` |
| 6 | `CSR[0x1BC4]` FH_RSCSR_CHNL0_RBDCB_BASE_REG | `rbd_ring_phys >> 8` |
| 7 | `CSR[0x1BC0]` FH_RSCSR_CHNL0_STTS_WPTR_REG | `rb_status_phys >> 4` |
| 8 | `CSR[0x1C00]` | **`0x80801114`** = ENABLE `0x80000000` \| IGNORE_RXF_EMPTY `0x4` \| IRQ_DEST_INT_HOST `0x1000` \| RB_SIZE_4K `0` \| (RX_RB_TIMEOUT `0x11` << 4) \| (log2(256)=8 << 20) |
| 9 | (NIC access released) `CSR8[0x004]` CSR_INT_COALESCING (8-bit write) | `0x40` |

Register addresses: `iwl-fh.h:215-301` (RSCSR base 0x1BC0, RCSR base 0x1C00, field values 286-301,
`RX_RB_TIMEOUT` 288). Use 8K instead by setting bits 17:16 = 01 (`0x00010000`) and 8 KiB RBs.

### 5.3 Filling the ring and the write pointer (`rx.c:1083-1190, 292-336, 167-202`)

- After `rx_init` Linux restocks: while free slots `(read - write - 1) & 255 > 0` put a buffer in
  `bd[write]`, `write = (write+1) & 255`. Starting from `read = write = 0` this fills **255** RBDs
  (indices 0..254), never all 256 (`rx.c:120-132, 309-326`).
- Tell the device: `CSR[0x1BC8] = write & ~7` (**multiple of 8**, so 248 initially). This is a
  plain `write32`, no NIC-access grab (`rx.c:193-195`; FH comment `iwl-fh.h:184-213`).
- The device writes `closed_rb_num` (bits [11:0]) = index of the next RBD it will fill, i.e.
  RBDs `read .. closed_rb_num-1` are complete (`internal.h:195-207`, `rx.c:1517-1527`).

### 5.4 Polling RX (bare-metal replacement for `iwl_pcie_rx_handle`, `rx.c:1501-1628`)

```
r = le16(rb_status.closed_rb_num) & 0xFFF & 255          # rx.c:1517-1521
while (read != r):
    rb = buffer at RBD index `read`
    off = 0
    while off + 4 + 4 < 4096:                            # rx.c:1314 (u32 + iwl_cmd_header)
        pkt = rb + off
        if le32(pkt.len_n_flags) == 0x55550000: break    # FH_RSCSR_FRAME_INVALID, iwl-trans.h:64
        len = le32(pkt.len_n_flags) & 0x3FFF             # FH_RSCSR_FRAME_SIZE_MSK, iwl-trans.h:63,94-97
        total = len + 4                                  # + the len_n_flags word, rx.c:1350-1351
        off += align_up(total, 0x40)                     # FH_RSCSR_FRAME_ALIGN, iwl-trans.h:65
        if total < 8 or off > 4096: break                # rx.c:1356-1357
        dispatch(pkt)   # pkt.hdr.cmd, pkt.hdr.sequence; payload = pkt + 8, payload_len = len - 4
    re-arm: bd[write] = rb_phys >> 8; write = (write+1) & 255
    read = (read + 1) & 255
if (write & ~7) changed: CSR[0x1BC8] = write & ~7
```

Packet header (`iwl-trans.h:71-102`, `fw/api/cmdhdr.h:65-98`):

| Offset | Field |
|---|---|
| 0 | `le32 len_n_flags` — bits[13:0] length of (hdr + payload); bits[21:16] RX queue (0) |
| 4 | `u8 cmd` |
| 5 | `u8 group_id` (0 for DVM) |
| 6 | `le16 sequence` — bit15 `SEQ_RX_FRAME` (0x8000) = unsolicited/uCode-originated; else bits[12:8] TX queue, bits[7:0] TFD index of the command being answered |
| 8 | payload |

Several packets can share one RB (DVM is pre-AX210, so the loop does not break after one,
`rx.c:1411-1412`).

**Command completion / reclaim** (`rx.c:1367-1408`, `tx.c:1624-1691, 1082-1119`): a packet is a
command response when `!(sequence & 0x8000)` and `cmd` is not in DVM's no-reclaim list
`{0xC0 REPLY_RX_PHY_CMD, 0xC1 REPLY_RX_MPDU_CMD, 0xC5 REPLY_COMPRESSED_BA, 0x9D STATISTICS_NOTIFICATION, 0x1C REPLY_TX}`
(`dvm/main.c:1243-1249`). Then `SEQ_TO_QUEUE(seq)` must equal the cmd queue and the command
queue's `read_ptr` becomes `SEQ_TO_INDEX(seq) + 1` (mod 256).

**Interrupt status while polling.** Linux reads `CSR[0x008]` (CSR_INT) and acks by writing the
read bits back (`rx.c:1720-1733, 1967`). Its own debug code prints "We got a masked interrupt",
i.e. CSR_INT shows causes outside CSR_INT_MASK (`rx.c:1920-1923`), so polling CSR_INT with the
mask at 0 should work, but that the hardware latches **every** cause while masked is
**UNVERIFIED**. Mirroring Linux's mask writes at each phase (§3.1, §7.3, §8) is cheap. For RX,
the authoritative signal is `closed_rb_num` in DRAM, not CSR_INT. Ack RX causes with
`CSR[0x010] = 0x40030000` (CSR_FH_INT_RX_MASK = HI_PRIOR | RX_CHNL1 | RX_CHNL0, `iwl-csr.h:222-230`, `rx.c:2066-2067`).

CSR_INT bits (`iwl-csr.h:196-207`): bit31 FH_RX, bit29 HW_ERR, bit28 RX_PERIODIC, bit27 FH_TX,
bit26 SCD, bit25 SW_ERR, bit7 RF_KILL, bit6 CT_KILL, bit3 SW_RX, bit2 RESET_DONE, bit1 WAKEUP,
bit0 ALIVE.

ICT. Linux does take the MSI path on this device and allocates an ICT table
(`trans.c:1604-1605, 1657, 3910-3914`), so after `fw_alive` it switches to ICT
(`rx.c:2205-2232`). But whenever `use_ict` is false (before `fw_alive`, after every stop) the
ISR reads `CSR_INT` directly (`rx.c:1909-1912, 1720-1733`), so that read path is exercised on real
hardware. ICT is an extra delivery mechanism for interrupts, not a prerequisite. A polling port
can skip it: never write `CSR[0x0A0]` (CSR_DRAM_INT_TBL_REG).

Per-start reset: every `nic_init` (so both the INIT and the runtime `start_fw`) re-runs
`_iwl_pcie_rx_init`, which zeroes the RB status block and sets `read = write = 0`
(`rx.c:1109-1127`). Do the same before the second start, or a stale `closed_rb_num` from the
INIT run will look like new data.

---

## 6. TX (DVM, gen1)

### 6.1 Memory

| Object | Size / alignment | Source |
|---|---|---|
| TFD ring per queue | 256 TFDs × 128 B = 32 KiB, **256-byte aligned** (register takes `phys >> 8`). All 20 queues get a ring and a CBBC register in Linux. | `tx.c:721-737`, `iwl-fh.h:48-60, 662-697`, `cfg/6000.c:80-81` |
| First-TB bounce buffers | one 64-byte buffer per **slot** (cmd queue: 32 slots; data queue: 256) | `tx.c:770-774`, `iwl-trans.h:712-724` |
| Byte-count tables | 20 queues × 320 entries × 2 B = 12800 B, **1 KiB aligned** (`SCD_DRAM_BASE_ADDR` takes `phys >> 10`); table q starts at `q * 640` bytes | `tx.c:801, 812-817, 503-504`, `iwl-fh.h:590-594, 731-733` |
| Keep-warm buffer | 4096 B, **4 KiB aligned** (register takes `phys >> 4`) | `tx.c:833-838, 961-962`, `iwl-fh.h:28-44, 714` |
| "Invalid" filler cmd | 8 B: wide header `{cmd 0x06 INVALID_WR_PTR_CMD, group 0x0F DEBUG_GROUP, seq 0xFFFF, len 0, rsvd 0, ver 0}`; every TFD in every ring is initialised to 1 TB pointing at it | `trans.c:1950-1968`, `tx.c:280-289, 778-785`, `fw/api/debug.h:37`, `fw/api/commands.h:51` |

### 6.2 TFD / TB format (`iwl-fh.h:620-697`, `tx.c:144-191`)

```
struct iwl_tfd {                 // 128 bytes
  u8   reserved[3];              // @0
  u8   num_tbs;                  // @3   bits[4:0] = number of TBs in use
  struct { le32 lo;              //      DMA addr [31:0]  (unaligned for odd entries)
           le16 hi_n_len; }      //      bits[3:0] = addr[35:32], bits[15:4] = length (max 0xFFF)
       tbs[20];                  // @4 + 6*i
  le32 pad;                      // @124
};
```

Building a TB i: `tbs[i].lo = addr; tbs[i].hi_n_len = (len << 4) | (addr >> 32 & 0xF); num_tbs = i + 1`.

### 6.3 `tx_init` (`tx.c:941-1005`), during `nic_init`

1. `PRPH[0xA02C10]` (SCD_TXFACT) = 0 (deactivate FIFOs, `iwl-scd.h:41-44`).
2. `CSR[0x197C]` (FH_KW_MEM_ADDR_REG) = `kw_phys >> 4` (direct, NIC access).
3. Every queue q = 0..19: `write_ptr = read_ptr = 0` (`tx.c:881-903`); `CSR[CBBC(q)] = tfd_ring_phys(q) >> 8` (direct) where `CBBC(q) = 0x19D0 + 4q` for q < 16 and `0x1BF0 + 4(q-16)` for 16..19 (`iwl-fh.h:61-84`).
4. set bits `0x00040000` (SCD_GP_CTRL_AUTO_ACTIVE_MODE) in `PRPH[0xA02DA8]` (SCD_GP_CTRL). The 31-queue bit is not set (20 queues, `tx.c:994-997`).

### 6.4 Scheduler setup at ALIVE: `iwl_pcie_tx_start` (`tx.c:480-534`)

Called through `iwl_trans_fw_alive` → `iwl_trans_pcie_fw_alive` (`iwl-trans.c:564-574`,
`trans.c:1372-1376`) at the start of DVM's ALIVE processing (`dvm/ucode.c:226`), for **both**
the INIT and the runtime image. SCD registers: `SCD_BASE = 0xA02C00` (`iwl-prph.h:223-235`,
`iwl-scd.h:52-74`).

| # | Access | Value |
|---|---|---|
| 1 | `scd_base = PRPH[0xA02C00]` (SCD_SRAM_BASE_ADDR) | read |
| 2 | SRAM write of **130 dwords of zero** at `scd_base + 0x600` | `(SCD_TRANS_TBL_OFFSET_QUEUE(20) - 0x600)/4 = (0x808-0x600)/4` (`iwl-prph.h:203-221`) |
| 3 | `PRPH[0xA02C08]` (SCD_DRAM_BASE_ADDR) | `bc_tables_phys >> 10` |
| 4 | `PRPH[0xA02E44]` (SCD_CHAINEXT_EN) | `0` (`scd_chain_ext_wa`) |
| 5 | enable cmd queue on FIFO 7 (§6.5) | `iwl_trans_ac_txq_enable(cmd_queue, 7, …)` (`iwl-trans.h:988-1000`, `dvm/main.c:1343`, `dvm/commands.h:164`) |
| 6 | `PRPH[0xA02C10]` (SCD_TXFACT) | `0x000000FF` (`IWL_MASK(0,7)`, `iwl-scd.h:36-39`, `iwl-trans.h:262`) |
| 7 | for ch 0..7: `CSR[0x1D00 + 0x20*ch]` (FH_TCSR_CHNL_TX_CONFIG_REG) | `0x80000008` (DMA_CHNL_ENABLE \| DMA_CREDIT_ENABLE) |
| 8 | set bits in `CSR[0x1E98]` (FH_TX_CHICKEN_BITS_REG) | `0x00000002` (SCD_AUTO_RETRY_EN) |
| 9 | clear bits in `PRPH[0x3010]` (APMG_PCIDEV_STT_REG) | `0x00000800` (re-enable L1-active; family < 8000) |

### 6.5 Enabling one queue: `iwl_trans_pcie_txq_enable` with a cfg (`tx.c:1150-1260`)

`iwl_trans_ac_txq_enable` passes `{fifo, sta_id = -1, tid = 8, frame_limit = 64, aggregate = false}`
and `ssn = 0` (`iwl-trans.h:988-1000`). For queue `q` on FIFO `f`:

1. (`scd_set_active = 0` → no `SCD_EN_CTRL` write.)
2. `PRPH[0xA02D0C + 4q]` (SCD_QUEUE_STATUS_BITS) = `0x00080000` (inactive, SCD_ACT_EN) (`iwl-scd.h:76-82`).
3. If `q != cmd_queue`: set bit `BIT(q)` in `PRPH[0xA02CE8]` (SCD_QUEUECHAIN_SEL).
4. Clear bit `BIT(q)` in `PRPH[0xA02E48]` (SCD_AGGR_SEL); `ssn = read_ptr` (= 0 after init).
5. `read_ptr = write_ptr = ssn & 0xFF`; `CSR[0x460]` (HBUS_TARG_WRPTR) = `(ssn & 0xFF) | (q << 8)` (direct).
6. `PRPH[0xA02C68 + 4q]` (SCD_QUEUE_RDPTR) = `ssn`.
7. SRAM `scd_base + 0x600 + 8q` = `0`; SRAM `scd_base + 0x600 + 8q + 4` = `0x00400040` (WIN_SIZE 64 in bits[6:0], FRAME_LIMIT 64 in bits[22:16], `iwl-prph.h:195-197`).
8. `PRPH[0xA02D0C + 4q]` = `0x017F0018 | f` (ACTIVE bit3, WSL bit4, `SCD_QUEUE_STTS_REG_MSK 0x017F0000`, FIFO in bits[2:0]) (`iwl-prph.h:185-189`). For the cmd queue on FIFO 7: **`0x017F001F`**.

### 6.6 Which queue is the command queue, and the data-queue map

| Case | cmd queue | Data queues enabled at ALIVE (queue → FIFO) | Source |
|---|---|---|---|
| EEPROM SKU bit8 (IPAN) **clear** | **4** | 0→3 (VO), 1→2 (VI), 2→1 (BE), 3→0 (BK) | `dvm/ucode.c:198-203, 228-240`, FIFO numbers `dvm/commands.h:152-161` |
| IPAN **set** (and firmware PAN flag, which this file has) | **9** | 0→3, 1→2, 2→1, 3→0, 4→0, 5→4, 6→2, 7→5, 8→4, (9 skipped: `IWL_TX_FIFO_UNUSED`), 10→5 | `dvm/ucode.c:205-217, 228-240` |

The cmd queue is enabled separately in §6.4 step 5. Data queues go through §6.5 with `f` from
the table (queue enable is called with wdg timeout 0, irrelevant here). The no-PAN layout is a
real Linux path (the one taken when the EEPROM disables IPAN), so a port may always take it only
if the EEPROM bit is clear. If the bit is set, Linux uses queue 9 and I did not find evidence the
firmware accepts queue 4 then (**UNVERIFIED**). Safest: follow the rule.

### 6.7 Placing a host command (`iwl_pcie_enqueue_hcmd`, `tx.c:1346-1618`)

Command header (`fw/api/cmdhdr.h:65-98`, short form for DVM since `group_id = 0`, `tx.c:1370-1376, 1484-1493`):

```
u8   cmd        // command id, e.g. 0x10 REPLY_RXON
u8   group_id   // 0
le16 sequence   // QUEUE_TO_SEQ(cmd_queue) | INDEX_TO_SEQ(write_ptr) = ((q & 0x1F) << 8) | (write_ptr & 0xFF)
payload...      // max 320 bytes when copied (DEF_CMD_PAYLOAD_SIZE, iwl-trans.h:121-148)
```

Steps for queue q (4 or 9), `slot = write_ptr & 31` (`internal.h:667-670`; the TFD ring index is
`write_ptr` itself, 0..255, `internal.h:672-681`):

1. Build the command contiguously: `buf = [hdr(4) | payload]`, total `T = 4 + payload_len`.
2. TB0: copy `min(T, 20)` bytes into the slot's 64-byte first-TB buffer (`tx.c:1540-1545`, `IWL_FIRST_TB_SIZE = 20`). Reset the TFD (`memset 0`) and set TB0 = `first_tb_phys + 64*slot`, len `min(T,20)`.
3. TB1 (if `T > 20`): the remaining `T - 20` bytes, mapped in place (`tx.c:1547-1585`). For copied commands Linux points TB1 at `out_cmd + 20`; for large NOCOPY commands (scan, calibration results) at `payload + 16`. Either way the device sees bytes [20, T) of the same logical buffer.
4. `write_ptr = (write_ptr + 1) & 255` (`internal.h:714-718`).
5. Kick: `CSR[0x460]` (HBUS_TARG_WRPTR) = `write_ptr | (q << 8)` — plain `write32`, no grab for the cmd queue (`tx.c:80-122`). HBUS registers normally need MAC_ACCESS_REQ held (`iwl-csr.h:496-509`), and this device has no `apmg_wake_up_wa` hold (§0). The bare write works because the NIC stays awake, which holds because Linux sends CAM (§9.6). If you ever enable power save, this kick is what breaks first.
6. Wait for the response packet whose `sequence` equals what you sent (bit15 clear); Linux waits up to `HOST_COMPLETE_TIMEOUT` and DVM commands are synchronous (`tx.c:2553-2628`, `dvm/lib.c:1247-1258`). Then `read_ptr` advances (§5.4).

The byte-count table is **not** written for host commands, only for `REPLY_TX` data frames
(`tx.c:2062-2109, 2275`). Linux always splits TB0 (20 B) + TB1. A single TB for the whole
command was common in older iwlagn drivers, but whether the hardware accepts it here is
**UNVERIFIED**; mirror the split.

Command payload limit when copied: 320 B. The scan command (≈1 KiB) and calibration results
use NOCOPY in Linux (`dvm/scan.c:891`, `dvm/calib.c:44-46`). In bare metal that just means "TB1
points at your buffer". TB length max is 0xFFF.

---

## 7. Firmware file and load

### 7.1 TLV parsing (`iwl-drv.c:788-1010`, `fw/file.h:47-156`)

File header (`struct iwl_tlv_ucode_header`, `fw/file.h:135-156`), 88 bytes:

| Off | Field | This file |
|---|---|---|
| 0 | `le32 zero` | 0 |
| 4 | `le32 magic` = `0x0A4C5749` | ok |
| 8 | `u8 human_readable[64]` | "6000g2b fw v18.168.6.1 build 0" |
| 72 | `le32 ver` (major<<24 \| minor<<16 \| API<<8 \| serial, `fw/file.h:582-585`) | `0x12A80601` → **API 6** |
| 76 | `le32 build` | 0 |
| 80 | `le64 ignore` | — |
| 88 | TLVs | — |

Each TLV: `le32 type; le32 length; data[length]`, next TLV at `+8 + align4(length)` (`iwl-drv.c:841-855`).

Mapping (`iwl-drv.c:858-998`) and the actual TLVs in your file (dumped):

| Type | Meaning | → | This file |
|---|---|---|---|
| 1 INST | runtime (`IWL_UCODE_REGULAR`) code, dest `0x000000` | sec INST | len 0x270BC @file 0x60 |
| 2 DATA | runtime data, dest `0x800000` | sec DATA | len 0x14000 @0x27124 |
| 16 WOWLAN_INST / 17 WOWLAN_DATA | WoWLAN image (ignore) | — | 0x22F98 / 0x14000 |
| 3 INIT | INIT code, dest `0x000000` | sec INST | len 0x1FCB4 @0x720D4 |
| 4 INIT_DATA | INIT data, dest `0x800000` | sec DATA | len 0x14000 @0x91D90 |
| 6 PROBE_MAX_LEN | `max_probe_length` | | 0x12C (300) |
| 7 PAN (len 0) | `flags \|= PAN` | | present |
| 18 FLAGS | `capa.flags = first u32` (**assignment**, so it overrides type 7) | | **0x0000000B** = PAN (bit0) \| NEWSCAN (bit1) \| bit3 |
| 8/9/10 | runtime event-log ptr/size, error-log ptr | | 0x8053D8 / 0x1C / 0x805344 |
| 11/12/13 | INIT event-log ptr/size, error-log ptr | | 0x80472C / 0x1C / 0x804698 |
| 14 ENHANCE_SENS_TBL (len 0) | enhanced sensitivity table format | | present |
| 15 PHY_CALIBRATION_SIZE | `standard_phy_calibration_size` | | 0x16 = 22, clamped to 19 (`iwl-drv.c:1838-1841`) |

Destinations: `IWLAGN_RTC_INST_LOWER_BOUND = 0x000000`, `IWLAGN_RTC_DATA_LOWER_BOUND = 0x800000`
(`iwl-agn-hw.h:12-16`). Size limits 0x40000 inst / 0x14000 data (`iwl-drv.c:1483-1530`, §0).

**Section order matters and is DATA first.** `enum iwl_ucode_sec { IWL_UCODE_SECTION_DATA = 0, IWL_UCODE_SECTION_INST = 1 }`
(`fw/img.h:40-43`), so each image is `sec[0] = DATA → 0x800000`, `sec[1] = INST → 0x000000`
(`iwl-drv.c:433-488, 1458-1481`), and the loader walks `sec[0..num_sec)` in order (`trans.c:798-819`).

Flag bits the port needs: PAN (bit0) → §6.6; NEWSCAN (bit1) → passive-scan `good_CRC_th = 0`
(`dvm/main.c:1483-1484`, `dvm/scan.c:790-795`). API 6 (> 1) → TX power is cmd **0x95**
(`dvm/lib.c:56-59`) and `TX_ANT_CONFIGURATION_CMD` **0x98** is sent (`dvm/main.c:726-730`); note
0x98 is also `REPLY_TX_POWER_DBM_CMD_V1` (`dvm/commands.h:82-83`).

### 7.2 Before loading: `iwl_trans_pcie_start_fw` (`trans.c:1283-1370`)

1. `prepare_card_hw` (§2.1).
2. `CSR[0x00C] = 0x00000080` (RF-kill mask); `CSR[0x008] = 0xFFFFFFFF`; then interrupts off (`CSR[0x00C] = 0`, `CSR[0x008] = 0xFFFFFFFF`, `CSR[0x010] = 0xFFFFFFFF`).
3. RF-kill check (§2.6); abort if killed.
4. `CSR[0x05C]` (UCODE_DRV_GP1_CLR) = `0x00000002` (SW_BIT_RFKILL); `CSR[0x05C] = 0x00000004` (CMD_BLOCKED) (`iwl-csr.h:83, 433-434`).
5. `CSR[0x008] = 0xFFFFFFFF`.
6. `nic_init` (`trans.c:485-520`): APM init (§3.2) → set_pwr (§3.3) → nic_config (§4.1) → rx_init (§5) → tx_init (§6.3). (`shadow_reg_enable = 0` → no `CSR[0x0A8]` write.)
7. `CSR[0x00C] = 0x08000000` (FH_TX only, `internal.h:945-952`).
8. `CSR[0x05C] = 0x00000002` twice.
9. Load the image (§7.3).

### 7.3 Loading sections over the FH service channel (`trans.c:603-713, 935-971`)

For each section in order (DATA, then INST), split into chunks of at most
`FH_MEM_TB_MAX_LENGTH = 0x20000` bytes (`iwl-fh.h:327`, `trans.c:663, 680-709`). Chunk at
section offset `o`: `dst = section_dst + o`. Your runtime INST (0x270BC) needs two chunks
(0x20000 + 0x70BC); DATA and INIT INST fit in one. The extended-address window
`0x40000..0x57FFF` (`trans.c:75-76, 687-701`) is never hit by this firmware.

Copy the chunk into a DMA buffer (Linux uses `dma_alloc_coherent` of the chunk size; required
alignment of this source buffer is **UNVERIFIED**, page-aligned is safe), then with NIC access
held (`trans.c:640-645`) write, channel 9 = `FH_SRVC_CHNL` (`iwl-fh.h:544-548`):

| # | Register | Value |
|---|---|---|
| 1 | `CSR[0x1E20]` FH_TCSR_CHNL_TX_CONFIG_REG(9) | `0x00000000` (pause) |
| 2 | `CSR[0x19C8]` FH_SRVC_CHNL_SRAM_ADDR_REG(9) | `dst` |
| 3 | `CSR[0x1948]` FH_TFDIB_CTRL0_REG(9) | `phys & 0xFFFFFFFF` |
| 4 | `CSR[0x194C]` FH_TFDIB_CTRL1_REG(9) | `((phys >> 32) & 0xF) << 28 \| byte_cnt` (= `byte_cnt` below 4 GiB) |
| 5 | `CSR[0x1E28]` FH_TCSR_CHNL_TX_BUF_STS_REG(9) | `0x00101003` = BIT(20) TB_NUM \| BIT(12) TB_IDX \| 3 TFDB_VALID |
| 6 | `CSR[0x1E20]` | `0x80100000` = DMA_CHNL_ENABLE \| CREDIT_DISABLE \| CIRQ_HOST_ENDTFD |

Addresses: `iwl-fh.h:436-499, 543-548`; values `trans.c:607-628`.

**Completion.** Linux waits up to 5 s for `ucode_write_complete`, set by the ISR on
`CSR_INT_BIT_FH_TX` (bit27) (`trans.c:647-653`, `rx.c:2110-2124`). Polling replacement:

```
until (CSR[0x008] & 0x08000000): spin (timeout 5 s)
CSR[0x010] = 0x00000003        # ack FH_INT TX_CHNL1|TX_CHNL0 (CSR_FH_INT_TX_MASK), rx.c:2112, iwl-csr.h:225-233
CSR[0x008] = 0x08000000        # ack CSR_INT (Linux acks all read bits, rx.c:1967)
```

That CSR_INT bit27 latches with this mask setting is consistent with Linux (mask = FH_TX here).

### 7.4 Starting the uCode (`trans.c:965-968`)

After the last section:

```
CSR[0x00C] = 0xBA00008F        # enable interrupts: CSR_INI_SET_MASK (iwl-csr.h:209-218, internal.h:898-906)
CSR[0x020] = 0x00000000        # CSR_RESET: release CPU reset → uCode runs
```

No `FH_UCODE_LOAD_STATUS` handshake for this family (that's the 8000 secure path, `trans.c:715-783`, chosen only for `device_family >= 8000`, `trans.c:1357-1360`).
No dual-CPU sections (no TLV 27).

### 7.5 What signals ALIVE

- `CSR[0x008]` bit0 (`CSR_INT_BIT_ALIVE`) — Linux only counts it (`rx.c:1997-2010`).
- The real signal is a `REPLY_ALIVE` (cmd 0x01) packet in the RX ring (bit31 FH_RX / bit3 SW_RX in CSR_INT). Linux waits for that packet with a 1 s timeout (`dvm/ucode.c:289, 305-321`).
- Errors: CSR_INT bit25 (SW_ERR) or bit29 (HW_ERR) (`rx.c:1976-1988, 2033-2045`), or a `REPLY_ERROR` (0x02) packet (`dvm/commands.h:426-433`). The firmware error log is at the `error_event_table_ptr` from ALIVE (SRAM, layout `dvm/commands.h:362-407`).

---

## 8. ALIVE

Packet: `cmd = 0x01` (REPLY_ALIVE, `dvm/commands.h:19`), `sequence & 0x8000` set (uCode-originated).
Payload = `struct iwl_alive_resp` (`dvm/commands.h:409-421`), 32 bytes, at packet offset 8:

| Payload off | Packet off | Field |
|---|---|---|
| 0 | 8 | `u8 ucode_minor` |
| 1 | 9 | `u8 ucode_major` |
| 2 | 10 | `le16 reserved1` |
| 4 | 12 | `u8 sw_rev[8]` |
| 12 | 20 | `u8 ver_type` |
| 13 | 21 | `u8 ver_subtype` — header comment: "not 9 for runtime alive" |
| 14 | 22 | `le16 reserved2` |
| 16 | 24 | `le32 log_event_table_ptr` (SRAM) |
| 20 | 28 | `le32 error_event_table_ptr` (SRAM) |
| 24 | 32 | `le32 timestamp` |
| 28 | 36 | `le32 is_valid` |

**Validity:** `is_valid == 0x00000001` (`UCODE_VALID_OK`, `dvm/commands.h:314`, `dvm/ucode.c:284`), else Linux fails with "Loaded ucode is not valid!" (`dvm/ucode.c:327-331`).

**INIT vs runtime:** Linux does **not** look at the packet to tell them apart. It records which
image it just loaded (`priv->cur_ucode`, `dvm/ucode.c:301-302`) and only stores `ver_subtype`
(`dvm/ucode.c:283`). Your port should do the same. The specific `ver_subtype` value INIT reports
(the old `INITIALIZE_SUBTYPE = 9` convention) is **UNVERIFIED** in this tree; only the comment at
`dvm/commands.h:322, 415` mentions 9.

After a valid ALIVE Linux sleeps 5 ms ("give rfkill time to run", `dvm/ucode.c:335-338`), then
runs `iwl_alive_notify` (§9.1, step A).

---

## 9. DVM command sequence: INIT → calibration → runtime → ready to scan

Driver entry is `__iwl_up` (`dvm/mac80211.c:218-265`):
`start_hw → iwl_run_init_ucode → start_hw → load REGULAR + wait ALIVE → iwl_alive_start`.
All commands below are sent synchronously (flags 0) through the cmd queue (§6.7) and you wait for
each response. Struct offsets are computed from the `__packed` definitions cited.

### 9.0 Command payloads used below

| Id | Name | Payload (bytes) | Layout | Source |
|---|---|---|---|---|
| 0x5A | COEX_PRIORITY_TABLE_CMD | 68 | all zero (`u8 flags; u8 rsvd[3]; {u8,u8,u8,u8} sta_prio[16]`) | `dvm/ucode.c:137-147`, `dvm/commands.h:3329, 3337-3359` |
| 0xB0 | REPLY_PHY_CALIBRATION_CMD (XTAL) | 8 | `0F 00 01 01 pin1 pin2 00 00`; `hdr = {op_code, first_group 0, groups_num 1, data_valid 1}`; `pin1 = (u8)xtal[0]`, `pin2 = (u8)xtal[1]` from §4.3 | `dvm/ucode.c:30-39`, `dvm/agn.h:56-62`, `dvm/commands.h:3072-3078, 3137-3154` |
| 0xB0 | (TEMP_OFFSET, v1) | 8 | `12 00 01 01 off_lo off_hi 00 00`; `off` = raw_temperature (§4.3), or 2700 (0x0A8C) if 0 | `dvm/ucode.c:41-54`, `dvm/commands.h:3156-3161` |
| 0xB0 | (stored INIT results) | = notification payload length | the `CALIBRATION_RES_NOTIFICATION` payload, byte for byte | `dvm/ucode.c:362-366`, `dvm/calib.c:34-85` |
| 0xCD | REPLY_BT_COEX_PROT_ENV | 4 | `{u8 action (1 open / 0 close); u8 type; u8 rsvd[2]}` | `dvm/ucode.c:183-196`, `dvm/commands.h:3680-3690` |
| 0x65 | CALIBRATION_CFG_CMD | 92 | `ucd_calib_cfg {once{is_enable@0, start@4, send_res@8, apply_res@12, rsvd@16}, perd@20..39, flags@40}`, `drv_calib_cfg@44..87`, `rsvd@88` (all le32) | `dvm/commands.h:3117-3135` |
| 0x9B | REPLY_BT_CONFIG (v1, advanced) | 72 | see §9.4 | `dvm/lib.c:201-306`, `dvm/commands.h:1873-1902` |
| 0xCC | REPLY_BT_COEX_PRIO_TABLE | 16 | `06 07 02 03 04 05 06 08 0A 00 00 00 00 00 00 00` | `dvm/ucode.c:149-181`, `dvm/commands.h:3631-3678` |
| 0x98 | TX_ANT_CONFIGURATION_CMD | 4 | `le32 valid = valid_tx_ant` | `dvm/main.c:720-735`, `dvm/commands.h:304-306` |
| 0x10 | REPLY_RXON | 50 | see §9.5 | `dvm/commands.h:555-577` |
| 0x18 | REPLY_ADD_STA | 92 | see §9.5 | `dvm/commands.h:780-875` |
| 0x4E | REPLY_TX_LINK_QUALITY_CMD | 88 | see §9.5 | `dvm/commands.h:1477-1544, 1734-1752` |
| 0xA4 | REPLY_CT_KILL_CONFIG_CMD | 12 | `le32 exit = 95; le32 rsvd = 0; le32 enter = 114` (°C) | `dvm/main.c:658-685`, `dvm/commands.h:2164-2168`, `dvm/devices.c:476-481`, `dvm/dev.h:38-39` |
| 0x77 | POWER_TABLE_CMD | 36 | `le16 flags; u8 keep_alive_s; u8 debug; le32 rx_to; le32 tx_to; le32 sleep[5]; le32 keep_alive_beacons` | `dvm/commands.h:2083-2105` |
| 0x95 | REPLY_TX_POWER_DBM_CMD | 4 | `s8 global_lmt (½ dBm); u8 flags = 0x40; s8 srv_chan_lmt = 0x7F; u8 rsvd` | `dvm/lib.c:26-63`, `dvm/commands.h:288-296` |

Payload size checks: alive 32; `iwl_calib_cfg_cmd` = 2×(20+20+4)+4 = 92; `iwl_wimax_coex_cmd` = 4+16×4 = 68;
`iwl_rxon_cmd` = 50; `iwl_addsta_cmd` = 4+12+56+4+4+2+2+1+1+2+2+2 = 92; `iwl_link_quality_cmd` = 4+8+8+64+4 = 88;
`iwl_powertable_cmd` = 2+1+1+4+4+20+4 = 36.

### 9.1 INIT image (`iwl_run_init_ucode`, `dvm/ucode.c:371-416`)

Linux registers a waiter for `{0x66 CALIBRATION_RES_NOTIFICATION, 0x67 CALIBRATION_COMPLETE_NOTIFICATION}`
**before** loading (`dvm/ucode.c:386-388`), so collect 0x66 packets from the moment you start
INIT.

1. `start_hw` (§3.1). In `__iwl_up` this has just been done (`dvm/mac80211.c:238`).
2. Load the INIT image (TLVs 4 then 3, §7) with §7.2-7.4. Wait for a valid ALIVE (1 s).
3. Sleep 5 ms. Step A, `iwl_alive_notify` (`dvm/ucode.c:219-256`):
   1. `tx_start` (§6.4), which enables the cmd queue.
   2. Enable data queues 0..3 (or the PAN set) (§6.5, §6.6).
   3. Send **0x5A** COEX_PRIORITY_TABLE_CMD, 68 zero bytes.
   4. Store XTAL calibration (op 15) in the results list, then send every stored result as **0xB0**. At INIT time the list holds only XTAL.
4. `iwl_init_alive_start` (`dvm/ucode.c:100-135`):
   1. Send **0xCD** `{01, 01, 00, 00}` (OPEN, `BT_COEX_PRIO_TBL_EVT_INIT_CALIB2 = 1`) — advanced BT coex is on for this device (`dvm/devices.c:654-661, 672`).
   2. Send **0x65** CALIBRATION_CFG_CMD: `once.is_enable = once.start = once.send_res = 0x0000005F` (IWL_CALIB_INIT_CFG_ALL = RX_BB|DC|LO|TX_IQ|RX_IQ|CRYSTAL, `dvm/commands.h:3084-3103`), `ucd_calib_cfg.flags (@40) = 0x00000001` (SEND_COMPLETE_NTFY), everything else 0 (`dvm/ucode.c:81-98`).
   3. Store TEMP_OFFSET (op 18, v1 format) in the results list. **Not sent now** (`dvm/ucode.c:123-132`).
5. Wait up to 2 s (`UCODE_CALIB_TIMEOUT`, `dvm/ucode.c:290, 403-404`): for each **0x66**, store its payload (header `{u8 op_code, u8 first_group, u8 groups_num, u8 data_valid}` + data), replacing any earlier entry with the same `op_code` in place, else appending (`dvm/calib.c:58-85`). Stop on **0x67**.
6. Stop the device (§3.4) whatever happened (`dvm/ucode.c:411-413`).

### 9.2 Runtime image

7. `start_hw` (§3.1) (`dvm/mac80211.c:250`).
8. Load the REGULAR image (TLVs 2 then 1) with §7.2-7.4 (this re-runs `nic_init`, so RX and TX rings are re-initialised). Wait for a valid ALIVE.
9. Sleep 5 ms, then step A again: `tx_start`, data queues, **0x5A**, XTAL re-stored (replaces in place), and then **every stored result is sent as 0xB0**, in list order: XTAL, TEMP_OFFSET, then the INIT results in arrival order.

### 9.3 `iwl_alive_start` (`dvm/main.c:765-860`)

10. Abort if RF-kill.
11. Advanced BT: send **0x9B** BT config (§9.4), then **0xCC** prio table, then **0xCD** `{01,01,00,00}` (OPEN), then **0xCD** `{00,01,00,00}` (CLOSE) (`dvm/main.c:784-809`).
12. Send **0x65** runtime calibration config: `once.is_enable = 0x000005DF` (IWL_CALIB_RT_CFG_ALL, `dvm/commands.h:3105-3113`), `once.start = 0x00000002` (DC), all else 0 (`dvm/main.c:703-717, 820`).
13. Send **0x98** TX_ANT_CONFIGURATION with `valid_tx_ant` (API 6 > 1) (`dvm/main.c:825`).
14. Build the BSS context RXON (§9.5) with **no interface**: `dev_type = 3`, `filter_flags = 0`, `node_addr = 00:00:00:00:00:00` (`dvm/rxon.c:18-77`, `dvm/main.c:833-840`).
15. Optional: async **0x9C** REPLY_STATISTICS_CMD with clear flag (`dvm/calib.c:1042-1056`, `dvm/main.c:354-370`). Not needed for scan.
16. `commit_rxon` (§9.5): send **0x10** RXON (assoc bit clear) → **0x18** ADD_STA broadcast (id 15) → **0x4E** LQ for id 15. No `REPLY_WIPAN_PARAMS`: it is only sent when the PAN context is active, and nothing activates it without a P2P/AP interface (`dvm/rxon.c:509-533`, `dvm/main.c:611-612`).
17. Write `CSR[0x05C]` (GP1_CLR) = `0x00000008` (CT_KILL_EXIT), then send **0xA4** CT-kill config (`dvm/main.c:658-685, 855`).
18. Send **0x77** POWER_TABLE_CMD (§9.6) (`dvm/main.c:859`).

### 9.4 BT config payload (0x9B, `iwl_bt_cmd_v1`, 72 bytes)

`bt_session_2` is not set for 6000 BT params, so v1 (`dvm/lib.c:223-235, 292-302`). Values from
`dvm/lib.c:203-285`, `dvm/main.c:787-796, 1374`, `iwl-drv.c:2019` (bt_coex_active = true):

| Off | Field | Value |
|---|---|---|
| 0 | `u8 flags` | `0x91` = 3-wire mode (2 << 3) \| SYNC_2_BT_DISABLE (0x80, because `bt_sco_disable`) \| CHANNEL_INHIBITION (0x01, `bt_ch_announce = true`) |
| 1 | `u8 ledtime` | 0 |
| 2 | `u8 max_kill` | 5 |
| 3 | `u8 bt3_timer_t7_value` | 1 |
| 4 | `le32 kill_ack_mask` | 0xFFFF0000 |
| 8 | `le32 kill_cts_mask` | 0xFFFF0000 |
| 12 | `u8 bt3_prio_sample_time` | 2 |
| 13 | `u8 bt3_timer_t2_value` | 0x0C |
| 14 | `le16 bt4_reaction_time` | 0 |
| 16 | `le32 bt3_lookup_table[12]` | `AAAAAAAA AAAAAAAA AEAAAAAA AAAAAAAA CC00FF28 0000AAAA CC00AAAA 0000AAAA C0004000 00004000 F0005000 F0005000` (`dvm/lib.c:170-183`) |
| 64 | `u8 reduce_txpower` | 0 |
| 65 | `u8 reserved` | 0 |
| 66 | `le16 valid` | 0x00FF (IWLAGN_BT_ALL_VALID_MSK, `dvm/commands.h:1851-1867`) |
| 68 | `u8 prio_boost` | 0xF0 |
| 69 | `u8 tx_prio_boost` | 0 |
| 70 | `le16 rx_prio_boost` | 0 |

`iwl_basic_bt_cmd` and `iwl_bt_cmd_v1` are not `__packed`, but every member is naturally aligned,
so the offsets above equal the packed ones.

### 9.5 RXON, broadcast station, LQ (`dvm/rxon.c:1023-1132, 340-380`, `dvm/sta.c`)

`commit_rxon` adjusts the staged RXON before sending (`dvm/rxon.c:1041-1058, 914-1003, 739-749`):
`flags |= 0x00008000` (TSF2HOST); basic rates (no vif) `ofdm_basic_rates = 0x15`, `cck_basic_rates = 0x0F`;
`flags |= 0x40000000` (SELF_CTS_EN) **only for the BG variant** (BGN has `use_rts_for_aggregation = true`,
`dvm/main.c:1143-1148`, `cfg/6000.c:206`); SHORT_SLOT cleared on 2.4 GHz; `filter_flags &= ~0x08` (HW decrypt).
It must pass `iwl_check_rxon_cmd` (`dvm/rxon.c:752-822`): non-zero channel, 1M or 6M basic, no CCK+AUTO_DETECT.

**REPLY_RXON (0x10) payload, 50 bytes** (`dvm/commands.h:555-577`), with the values Linux ends
up with after the station interface is added (§9.7) for channel 1:

| Off | Field | Value |
|---|---|---|
| 0 | `u8 node_addr[6]` | your MAC (zeros at step 14, before an interface exists) |
| 6 | `le16 reserved1` | 0 |
| 8 | `u8 bssid_addr[6]` | 0 |
| 14 | `le16 reserved2` | 0 |
| 16 | `u8 wlap_bssid_addr[6]` | 0 |
| 22 | `le16 reserved3` | 0 |
| 24 | `u8 dev_type` | 3 (RXON_DEV_TYPE_ESS, `dvm/commands.h:445-453`, `dvm/main.c:626-627`) |
| 25 | `u8 air_propagation` | 0 |
| 26 | `le16 rx_chain` | `(ant << 1) \| (min(2, n) << 12) \| (min(1, n) << 10)` where `ant = valid_rx_ant`, `n = popcount(ant)`; e.g. ant 0x3 → `0x2406`, ant 0x1 → `0x1402` (initial SMPS is STATIC: `dvm/main.c:1107`, `dvm/lib.c:695-834`, bit positions `dvm/commands.h:456-469`) |
| 28 | `u8 ofdm_basic_rates` | 0x15 (6/12/24M) |
| 29 | `u8 cck_basic_rates` | 0x0F (1/2/5.5/11M) |
| 30 | `le16 assoc_id` | 0 |
| 32 | `le32 flags` | `0x00008005` (BAND_24G 0x1 \| AUTO_DETECT 0x4 \| TSF2HOST 0x8000); BG variant: `0x40008005` (`dvm/rxon.c:716-737`) |
| 36 | `le32 filter_flags` | `0x00000004` (ACCEPT_GRP) with the station interface; `0` at step 14 |
| 40 | `le16 channel` | 1 (any valid channel; Linux uses mac80211's current channel) |
| 42 | `u8 ofdm_ht_single_stream_basic_rates` | 0xFF |
| 43 | `u8 ofdm_ht_dual_stream_basic_rates` | 0xFF |
| 44 | `u8 ofdm_ht_triple_stream_basic_rates` | 0xFF |
| 45 | `u8 reserved5` | 0 |
| 46 | `le16 acquisition_data` | 0 |
| 48 | `le16 reserved6` | 0 |

Each unassociated RXON wipes the firmware station table (`dvm/commands.h:826-831`), so Linux
then re-adds the broadcast station (`dvm/rxon.c:364-371`, `dvm/sta.c:642-718`). It was prepared
in `__iwl_up` (`dvm/mac80211.c:230-236`, `dvm/sta.c:1266-1297, 237-317`).

**REPLY_ADD_STA (0x18) payload, 92 bytes** (`dvm/commands.h:807-875`):
`mode@0 = 0` (add); `sta.addr@4 = FF FF FF FF FF FF`; `sta.sta_id@12 = 0x0F` (IWLAGN_BROADCAST_ID,
`dvm/commands.h:731`); `sta.modify_mask@13 = 0`; key block @16..71 zero; `station_flags@72 = 0`;
`station_flags_msk@76 = 0`; rest zero. Response payload `u8 status`, must be `0x01`
(ADD_STA_SUCCESS_MSK, `dvm/sta.c:99-119`, `dvm/commands.h:878-887`).

Is the broadcast station needed for a scan? The firmware header says the station table is needed
before transmitting "except active scans or active measurements; those commands carry their own
txpower/rate setup data" (`dvm/commands.h:826-830`), yet Linux's scan TX command names
`sta_id = 15` (`dvm/scan.c:736`). Adding it costs two commands, so do what Linux does.

**REPLY_TX_LINK_QUALITY_CMD (0x4E) payload, 88 bytes** (`dvm/sta.c:549-597`), 2.4 GHz:
`sta_id@0 = 0x0F`; `reserved1@1 = 0`; `control@2 = 0`;
`general_params@4 = {flags 0, mimo_delimiter 0, single_stream_ant_msk = first(valid_tx_ant), dual_stream_ant_msk = valid_tx_ant & ~first, or 0x03 (ANT_AB) if that is 0, start_rate_index[4] = 0}`;
`agg_params@12 = {le16 agg_time_limit = 4000 (0x0FA0), u8 agg_dis_start_th = 3, u8 agg_frame_cnt_limit = 0, le32 0}`;
`rs_table[16]@20 = 0x0000020A | (first(valid_tx_ant) << 14)` each (1 Mbps PLCP 10, CCK 0x200);
`reserved2@84 = 0`. `first()` returns the lowest antenna **mask** bit (ANT_A=1, `dvm/rs.h:368-375`).

### 9.6 POWER_TABLE_CMD (0x77)

The DVM module parameter `force_cam` defaults to `true` (`dvm/power.c:25-27`), and
`iwl_power_build_cmd` then always builds the CAM (continuously-aware, no power save) command
(`dvm/power.c:277-280`): all zeros plus `flags |= 0x0008` (PCI_PM, because `pm_support` is true
after ASPM L0s is disabled, `dvm/power.c:243-252, 371`, `trans.c:217`). So Linux sends:
`flags = 0x0008`, all other 34 bytes 0. Do the same. Do not set bit0 (DRIVER_ALLOW_SLEEP): with
power save the NIC may sleep, and every write-pointer kick would first have to check
`CSR_UCODE_DRV_GP1` MAC_SLEEP and request wake-up (`tx.c:94-112`, `dvm/power.c:337-343`).

### 9.7 Adding the station interface (mac80211 `add_interface` → `iwl_set_mode`, `dvm/mac80211.c:1197-1204, 1206-1250`)

19. Rebuild RXON with the interface: `dev_type = 3`, `filter_flags = 0x04`, `node_addr = MAC` (`dvm/rxon.c:31-34, 71-72`), recompute rx_chain, `commit_rxon` → **0x10** RXON, **0x18** ADD_STA(15), **0x4E** LQ(15) as in §9.5.
20. TX power from mac80211 config (`dvm/rxon.c:1237-1239`, `382-438`): send **0x95** `{global_lmt = min(2 × power_level_dBm, max_tx_pwr_half_dbm), 0x40, 0x7F, 0}`. With no regulatory input use `global_lmt = max_tx_pwr_half_dbm` from §4.3. Linux refuses TX power while a scan is running (`dvm/lib.c:31-33`), so send it before scanning. Whether a passive scan needs it at all is **UNVERIFIED**; active-scan probes carry their own rate/gain (§10).

The device is now in the state from which Linux issues `REPLY_SCAN_CMD`.

---

## 10. REPLY_SCAN_CMD (0x80) and scan results

### 10.1 Command layout (`struct iwl_scan_cmd`, `dvm/commands.h:2298-2338`)

| Off | Field | Value Linux uses (2.4 GHz, unassociated) | Source |
|---|---|---|---|
| 0 | `le16 len` | total payload length = 764 + probe_len + 12 × n_channels | `dvm/scan.c:888-892` |
| 2 | `u8 scan_flags` | 0 | `dvm/commands.h:2292-2296` |
| 3 | `u8 channel_count` | n (≤ 50, `MAX_SCAN_CHANNEL`) | `dvm/scan.c:31, 875-879` |
| 4 | `le16 quiet_time` | 10 (ms) | `dvm/scan.c:663`, `dvm/agn.h:211` |
| 6 | `le16 quiet_plcp_th` | 1 | `dvm/scan.c:662`, `dvm/agn.h:212` |
| 8 | `le16 good_CRC_th` | NEWSCAN firmware (this one): active → 1, passive → 0 | `dvm/scan.c:790-795`, `dvm/commands.h:2234-2236` |
| 10 | `le16 rx_chain` | `0x0001 \| (rx_ant << 4) \| (rx_ant << 7) \| (valid_rx_ant << 1)`, `rx_ant = valid_rx_ant` (CAM, no BT full concurrency) | `dvm/scan.c:812-842` |
| 12 | `le32 max_out_time` | 0 | unassociated: not set (`dvm/scan.c:665-692`) |
| 16 | `le32 suspend_time` | 0 | same |
| 20 | `le32 flags` | `0x00000005` (BAND_24G \| AUTO_DETECT) | `dvm/scan.c:741` |
| 24 | `le32 filter_flags` | `0x00000044` (ACCEPT_GRP \| BCON_AWARE) | `dvm/scan.c:866-867` |
| 28 | `struct iwl_tx_cmd_hdr tx_cmd` (56 B) | see below | `dvm/commands.h:1182-1255` |
| 84 | `struct iwl_ssid_ie direct_scan[20]` (20 × 34 B) | `{u8 id = 0; u8 len; u8 ssid[32]}`; extra SSIDs 2..n in reverse order; zero for a wildcard scan | `dvm/scan.c:705-729`, `dvm/commands.h:2226-2232` |
| 764 | `u8 data[]` | probe request frame (`tx_cmd.len` bytes), then `channel_count` × 12-byte channel entries | `dvm/commands.h:2322-2337`, `dvm/scan.c:845-879` |

**`tx_cmd` (offsets within the 56-byte header; `dvm/commands.h:1195-1253`, values `dvm/scan.c:735-810, 864`):**

| Off | Field | Value |
|---|---|---|
| 0 | `le16 len` | probe request length |
| 2 | `le16 next_frame_len` | 0 |
| 4 | `le32 tx_flags` | `0x00003000` = SEQ_CTL (0x2000) \| IGNORE_BT (0x1000, advanced BT on 2.4 GHz) |
| 8 | `scratch` (4) | 0 |
| 12 | `le32 rate_n_flags` | `0x0000000A` (1 Mbps PLCP) \| `0x200` (CCK) \| `BIT(i) << 14`, where `i` = index of the lowest set bit of `valid_tx_ant` (advanced BT restricts 2.4 GHz probes to `first_antenna`, and `iwl_toggle_tx_ant` over one valid antenna returns its index). With antenna A: `0x0000420A` (`dvm/scan.c:799-810`, `dvm/lib.c:836-851`, `dvm/agn.h:320-333`). Use 6 Mbps OFDM (PLCP 13, no CCK bit) on 5 GHz. |
| 16 | `u8 sta_id` | 15 (`ctx->bcast_sta_id`) |
| 17 | `u8 sec_ctl` | 0 |
| 18 | `u8 initial_rate_index` | 0 |
| 19 | `u8 reserved` | 0 |
| 20 | `u8 key[16]` | 0 |
| 36 | `le16 next_frame_flags` | 0 |
| 38 | `le16 reserved2` | 0 |
| 40 | `le32 stop_time.life_time` | `0xFFFFFFFF` (TX_CMD_LIFE_TIME_INFINITE) |
| 44 | `le32 dram_lsb_ptr` | 0 |
| 48 | `u8 dram_msb_ptr` | 0 |
| 49 | `u8 rts_retry_limit` | 0 |
| 50 | `u8 data_retry_limit` | 0 |
| 51 | `u8 tid_tspec` | 0 |
| 52 | `le16 timeout` | 0 |
| 54 | `le16 driver_txop` | 0 |

The source comments say `/*byte 50*/` and `/*byte 51*/` for `rts_retry_limit` and
`data_retry_limit` (`dvm/commands.h:1241-1242`). Counting the packed fields gives 49 and 50, so
those comments are stale. Total header 56 bytes, consistent with `scan_cmd.data` at 764
(28 + 56 + 680).

**Probe request template** (`iwl_fill_probe_req`, `dvm/scan.c:561-607`), at `data[0]`:

```
le16 frame_control = 0x0040 (PROBE_REQ)   le16 duration = 0
u8 da[6]    = FF FF FF FF FF FF           u8 sa[6] = your MAC
u8 bssid[6] = FF FF FF FF FF FF           le16 seq_ctrl = 0
IE 0 (SSID): 00 <len> <ssid>              # wildcard: 00 00
then mac80211-supplied IEs (scan_request->ie), e.g. Supported Rates
```

DVM itself only adds the SSID IE. The rest comes from mac80211 (`dvm/scan.c:848-851`), so which
IEs a minimal port must add is outside the DVM trace (**UNVERIFIED**; a Supported Rates IE
`01 04 02 04 0B 16` plus Extended Supported Rates is the usual minimum by 802.11, not from this
source). For a passive scan the template is still built but not transmitted. Max length
`max_probe_length = 300` (§7.1).

**Channel entry** (`struct iwl_scan_channel`, 12 bytes, `dvm/commands.h:2199-2216`, values `dvm/scan.c:346-556`):

| Off | Field | Value (2.4 GHz) |
|---|---|---|
| 0 | `le32 type` | bit0: 1 = active, 0 = passive (passive if not requested active or the channel is NO_IR, §4.3); bits[20:1]: direct-SSID bitmap `IWL_SCAN_PROBE_MASK(n_probes)` when n_probes > 0 |
| 4 | `le16 channel` | channel number |
| 6 | `u8 tx_gain` | `0x28` = (1<<5) \| (5<<3). 5 GHz: `0x3B` |
| 7 | `u8 dsp_atten` | 110 (0x6E) |
| 8 | `le16 active_dwell` | 30 + 3 × (n_probes + 1) = **33** TU for one SSID. 5 GHz: 20 + 2 × (n+1) |
| 10 | `le16 passive_dwell` | 100 + 20 = **120** TU (5 GHz: 110); unassociated → not clamped; forced to `active + 1` if not larger |

`n_probes` counts only the SSIDs after the first. The first SSID goes into the probe template,
and an active wildcard scan has exactly one empty SSID, so `n_probes = 0` (`dvm/scan.c:705-729`).

Send it as a normal host command (0x80, payload `len` bytes). Linux marks it NOCOPY, so TB0 =
header + first 16 payload bytes, TB1 = the rest (§6.7). Only one band per command: wait for
SCAN_COMPLETE before the next one (`dvm/commands.h:2286, 2332-2335`).

### 10.2 Notifications and frames during a scan

| Id | Packet | Payload | Source |
|---|---|---|---|
| 0x80 | response to REPLY_SCAN_CMD | `le32 status` (1 = OK, 2 = cannot fulfil) | `dvm/commands.h:2345-2350`, `dvm/scan.c:232-241` |
| 0x82 | SCAN_START_NOTIFICATION | `le32 tsf_low; le32 tsf_high; le32 beacon_timer; u8 channel; u8 band; u8 rsvd[2]; le32 status` (20 B) | `dvm/commands.h:2355-2363` |
| 0x83 | SCAN_RESULTS_NOTIFICATION (per channel) | `u8 channel; u8 band; u8 probe_status; u8 num_probe_not_sent; le32 tsf_low; le32 tsf_high; le32 statistics[1]` (16 B) | `dvm/commands.h:2368-2386` |
| 0x84 | SCAN_COMPLETE_NOTIFICATION | `u8 scanned_channels; u8 status; u8 bt_status; u8 last_channel; le32 tsf_low; le32 tsf_high` (12 B) | `dvm/commands.h:2391-2398`, `dvm/scan.c:285-333` |
| 0xC0 | REPLY_RX_PHY_CMD | `struct iwl_rx_phy_res` (60 B), cached for the next MPDU | `dvm/rx.c:540-549` |
| 0xC1 | REPLY_RX_MPDU_CMD | the received 802.11 frame (beacon / probe response) | `dvm/rx.c:775-896` |

Beacons and probe responses are **not** wrapped in a scan notification. They arrive as ordinary
received frames: one 0xC0 PHY packet, then one 0xC1 MPDU packet (`dvm/rx.c:978-979`). The
filter flags in the scan command (ACCEPT_GRP | BCON_AWARE) are what let them through.

**`struct iwl_rx_phy_res`** (0xC0 payload, `dvm/commands.h:1049-1062`):

| Off | Field |
|---|---|
| 0 | `u8 non_cfg_phy_cnt` |
| 1 | `u8 cfg_phy_cnt` (Linux drops the MPDU if > 20) |
| 2 | `u8 stat_id` |
| 3 | `u8 reserved1` |
| 4 | `le64 timestamp` (TSF at on-air rise) |
| 12 | `le32 beacon_time_stamp` |
| 16 | `le16 phy_flags` — bit0 2.4 GHz, bit1 CCK, bit2 short preamble, bits[6:4] antennas, bit7 AGG (`dvm/commands.h:994-1000`) |
| 18 | `le16 channel` |
| 20 | `u8 non_cfg_phy_buf[32]` = `le32 non_cfg_phy[8]` |
| 52 | `le32 rate_n_flags` |
| 56 | `le16 byte_count` |
| 58 | `le16 frame_time` |

RSSI (`dvm/rx.c:734-772`, masks `dvm/commands.h:1024-1038`): `v1 = non_cfg_phy[1]` (payload
offset 24), `agc = (v1 & 0xFE00) >> 9`; `v2 = non_cfg_phy[2]` (offset 28), `rssi_a = v2 & 0xFF`,
`rssi_b = (v2 >> 16) & 0xFF`; `v3 = non_cfg_phy[3]` (offset 32), `rssi_c = v3 & 0xFF`;
`dBm = max(rssi_a, rssi_b, rssi_c) - agc - 44` (`IWLAGN_RSSI_OFFSET`, `iwl-agn-hw.h:33`).

**0xC1 MPDU packet payload** (`dvm/rx.c:798-823`, `dvm/commands.h:1064-1067`):

```
@0  le16 byte_count          # length L of the 802.11 frame
@2  le16 reserved
@4  802.11 frame, L bytes    # starts with frame_control
@4+L le32 rx_pkt_status      # require bit0 (NO_CRC32_ERROR) and bit1 (NO_RXE_OVERFLOW)
```

Reject if `L + 4 + 4 > payload_len` (`dvm/rx.c:803-806`), if no cached PHY packet came first
(`dvm/rx.c:788-791`), or if `rx_pkt_status & 0x3 != 0x3` (`dvm/rx.c:818-823`).

**Listing SSIDs** (802.11 frame format, general knowledge, not iwlwifi): `frame_control`
subtype 8 (beacon, `0x0080`) or 5 (probe response, `0x0050`). BSSID = addr3 at frame offset 16.
The 24-byte MAC header is followed by 12 fixed bytes (timestamp 8, beacon interval 2,
capability 2), then IEs from frame offset 36. IE id 0 is the SSID (len 0..32). Channel: the DS
Parameter IE (id 3), or `phy_res.channel`.

---

## 11. Condensed order of operations

1. PCI: memory + bus master; cfg 0x41 := 0; ASPM L0s/L1/CLKPM off (§1).
2. `start_hw`: prepare_card_hw → SW reset → prepare_card_hw → APM init → RF-kill mask (§2.1, §3.1-3.2).
3. Read OTP/EEPROM, parse MAC / antennas / SKU (IPAN) / xtal / temperature / channels / max TX power (§4.2-4.3). Pick cmd queue 4 or 9 (§6.6).
4. Stop device (§3.4). `start_hw` again.
5. `start_fw(INIT)`: prepare → GP1 clears → `nic_init` (APM, VMAIN, nic_config, RX ring, TX rings) → FH_TX-only mask → load DATA then INST via service channel 9, polling CSR_INT bit27 → mask 0xBA00008F, CSR_RESET = 0 (§7.2-7.4).
6. Poll RX for ALIVE; check `is_valid` (§8). Sleep 5 ms.
7. alive_notify: SCD setup + cmd queue (FIFO 7) + data queues; 0x5A; 0xB0 XTAL (§6.4-6.6, §9.1).
8. 0xCD open; 0x65 INIT calib cfg; collect 0x66 until 0x67 (≤ 2 s) (§9.1).
9. Stop device; `start_hw`; `start_fw(REGULAR)`; ALIVE; sleep 5 ms; alive_notify again, now sending XTAL + TEMP_OFFSET + all INIT results as 0xB0 (§9.2).
10. 0x9B, 0xCC, 0xCD open, 0xCD close, 0x65 RT (start DC), 0x98, RXON (no MAC) + ADD_STA(15) + LQ(15), GP1_CLR 0x8 + 0xA4, 0x77 CAM (§9.3-9.6).
11. RXON with MAC / ESS / ACCEPT_GRP + ADD_STA(15) + LQ(15); 0x95 TX power (§9.7).
12. 0x80 scan; parse 0x82 / 0xC0+0xC1 / 0x83 / 0x84 (§10).

---

## 12. UNVERIFIED items, collected

1. Whether every CSR_INT cause latches while its CSR_INT_MASK bit is 0 (§5.4, §7.3). Linux evidence: CSR_INT shows masked causes (`rx.c:1920-1923`); RX itself is driven by `closed_rb_num` in DRAM.
2. Required alignment of the firmware-chunk DMA source buffer (§7.3). Linux uses `dma_alloc_coherent`; page-aligned is safe.
3. Whether one TB per host command works instead of Linux's 20-byte TB0 + remainder (§6.7).
4. Whether firmware accepts cmd queue 4 when the EEPROM enables IPAN (§6.6).
5. `ver_subtype` value of the INIT ALIVE (§8).
6. (Resolved: Linux sends CAM by default, `force_cam = true`, §9.6.)
7. Whether a passive scan needs the 0x95 TX power command or the broadcast station first (§9.5, §9.7). Linux has both in place before any scan.
8. Which probe-request IEs beyond SSID a minimal active scan needs (mac80211 supplies them, §10.1).
9. Whether the upstream port's ASPM also needs disabling (§1, step 4).
10. OTP vs EEPROM on your specific unit: read `CSR[0x034]` bit16 (§4.2).
11. The payload of DVM's "simple generic response" to commands like RXON is not parsed by DVM, so I did not trace its layout. Only the `sequence` match matters for completion (§6.7).

