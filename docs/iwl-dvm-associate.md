# Intel Centrino Wireless-N 130 (8086:0896) — from "scan works" to "WPA2-PSK data"

Continuation of `docs/iwl-dvm-bringup.md` (called **BU** below; "BU §9.5" = that file's section 9.5).
Traced from Linux 6.18.44. Paths without a prefix are relative to
`drivers/net/wireless/intel/iwlwifi/` (as in BU). `mac80211/x.c` means `net/mac80211/x.c`,
`ieee80211.h` means `include/linux/ieee80211.h`.

Conventions (same as BU): every value is the one Linux uses on this device's path, in hex,
little-endian unless stated. Things I could not trace to source are marked **UNVERIFIED**. The
WPA2 cryptography (§5) has no kernel source behind it (the 4-way handshake lives in
wpa_supplicant/hostapd). Every constant there is marked **[GK]** (general knowledge, IEEE
802.11-2016 section given where I can).

Device facts used throughout (from BU and the running driver `src/iwl.rs`): command queue **9**
(IPAN set, PAN queue map BU §6.6), `valid_tx_ant = 0x1` (ANT_A), `valid_rx_ant = 0x3`, broadcast
station 15 already in use for scans. Linux uses the **BSS context** (not PAN) for a station
interface: the first context whose `interface_modes` contains STATION is BSS
(`dvm/mac80211.c:1273-1300`, `dvm/main.c:611-623`). The PAN context stays inactive, so no
`REPLY_WIPAN_*` command is ever sent (`dvm/rxon.c:516-533`).

---

## 0. What `src/iwl.rs` must grow before any of this works

1. **Real TX rings for data queues.** `tx_init` points every non-command queue at one shared
   `IDLE_RING` (`src/iwl.rs:156-160`). Queues 0 (VO) and 2 (BE) at least need their own 256×128-byte
   TFD ring (256-byte aligned), each initialised to the INVALID filler like now. Each also needs
   per-slot 64-byte first-TB bounce buffers and command buffers, plus its own `write_ptr`/`read_ptr`.
   (Linux: every queue gets its own ring and 256 first-TB buffers for data queues, BU §6.1.)
2. **A REPLY_TX (0x1C) handler in `rx_poll`/`event`.** `cmd()` correctly ignores 0x1C as a command
   completion (`src/iwl.rs:256`, BU §5.4). The TX status must be consumed elsewhere and used to reclaim
   the data queue (§1.7).
3. **Byte-count table writes for data queues** (§1.5). The table is already allocated and registered
   (`BC`, `src/iwl.rs:12,19,194`).
4. **Data-frame RX.** `event()` only parses beacons and probe responses out of 0xC1. It needs a
   data path (§7).
5. **More scan state.** `Bss` keeps SSID, BSSID, channel, dBm, privacy. Association also needs the
   beacon's TSF (frame offset 24), beacon interval (offset 32), capability (offset 34), Supported/Extended
   Supported Rates (IE 1/50), RSN IE (48), DTIM period (TIM IE 5, byte 1), ERP IE (42) and the WMM
   parameter IE (221, OUI 00:50:F2 type 2) (§2.1, §4).

---

## 1. Transmitting an 802.11 frame through DVM (REPLY_TX, 0x1C)

### 1.1 Which queue

mac80211 picks an access category (AC). DVM maps it to a hardware queue with the BSS context's
`ac_to_queue = {VO→0, VI→1, BE→2, BK→3}` (`dvm/main.c:581-583`), through
`vif->hw_queue[ac]` (`dvm/mac80211.c:1241-1242`) and `info->hw_queue` (`mac80211/tx.c:1906`).
`iwlagn_tx_skb` uses `info->hw_queue` directly (`dvm/tx.c:416`). With the PAN map, queue→FIFO is
0→3 (VO), 1→2 (VI), 2→1 (BE), 3→0 (BK), exactly as for the non-PAN map (BU §6.6), and those queues
are already enabled at ALIVE.

| Frame | AC chosen by mac80211 | Queue |
|---|---|---|
| Any management frame (auth, assoc req, deauth) | `priority = 7` → AC_VO (`mac80211/wme.c:132-134`) | **0** |
| Data to a non-QoS (non-WMM) AP, including EAPOL | AC_BE (`mac80211/wme.c:167-170`) | **2** |
| EAPOL to a WMM AP | `priority = 7` → AC_VO (`mac80211/wme.c:172-174`) | **0** |
| IP data to a WMM AP | `cfg80211_classify8021d` (DSCP), usually 0 → AC_BE (`mac80211/wme.c:180-184`) | **2** (usually) |

`ieee802_1d_to_ac[]` is `{BE, BK, BK, BE, VI, VI, VO, VO}` for priorities 0..7 (`mac80211/wme.c:23-31`).

### 1.2 The command buffer Linux builds

The REPLY_TX "command" is one contiguous buffer: `struct iwl_cmd_header` (4 bytes), then
`struct iwl_tx_cmd` (56 bytes), then the 802.11 MAC header copied in (`dvm/tx.c:349-350, 414`;
struct `dvm/commands.h:1182-1261`). The buffer comes from `kmem_cache_zalloc`, so every field
Linux does not set is **zero** (`pcie/gen1_2/internal.h:798-803`). The transport uses the layout
`struct iwl_tx_cmd_v6_params` (`fw/api/tx.h:219-243`, `pcie/gen1_2/tx.c:2116`). It is byte-for-byte
the same 56 bytes, so both definitions give the offsets below.

| Buf off | tx_cmd off | Field | Notes / source |
|---|---|---|---|
| 0 | — | `u8 cmd` = **0x1C** | `dvm/tx.c:349`, `dvm/commands.h:39` |
| 1 | — | `u8 group_id` = 0 | zeroed |
| 2 | — | `le16 sequence` = `(queue << 8) \| write_ptr` | `pcie/gen1_2/tx.c:2183-2185` (QUEUE_TO_SEQ/INDEX_TO_SEQ as in BU §6.7) |
| 4 | 0 | `le16 len` = MPDU length = 802.11 header + body as handed to the device (includes the 8-byte CCMP header if any; excludes pad, MIC, FCS) | `dvm/tx.c:353-354` (`skb->len`) |
| 6 | 2 | `le16 next_frame_len` = 0 | `dvm/tx.c:102` |
| 8 | 4 | `le32 tx_flags` | §1.4 |
| 12 | 8 | `scratch` (4 B) = 0, written by uCode | `dvm/commands.h:1206-1208` |
| 16 | 12 | `le32 rate_n_flags` | mgmt only; **0** for data (`dvm/tx.c:140-143`) |
| 20 | 16 | `u8 sta_id` | 15 for mgmt, 0 (AP) for data (§1.4) |
| 21 | 17 | `u8 sec_ctl` | 0, or **0x02** (`TX_CMD_SEC_CCM`) for CCMP (`dvm/tx.c:195-196`, `dvm/commands.h:1162`) |
| 22 | 18 | `u8 initial_rate_index` = 0 | `dvm/tx.c:141` |
| 23 | 19 | `u8 reserved` = 0 | |
| 24 | 20 | `u8 key[16]` | CCMP: the 16-byte TK (`dvm/tx.c:197`) |
| 40 | 36 | `le16 next_frame_flags` = 0 | |
| 42 | 38 | `le16 reserved2` = 0 | |
| 44 | 40 | `le32 life_time` = **0xFFFFFFFF** | `TX_CMD_LIFE_TIME_INFINITE`, `dvm/tx.c:52`, `dvm/commands.h:2233` |
| 48 | 44 | `le32 dram_lsb_ptr` = phys(first-TB buffer of this slot) + **12** | `pcie/gen1_2/tx.c:2187-2192`: `tb0_phys + sizeof(iwl_cmd_header) + offsetof(scratch)` = +4 +8 |
| 52 | 48 | `u8 dram_msb_ptr` = bits 35:32 of that address = 0 | same |
| 53 | 49 | `u8 rts_retry_limit` = **0x3C** (60) | `IWLAGN_RTS_DFAULT_RETRY_LIMIT`, `dvm/tx.c:122`, `iwl-agn-hw.h:37` |
| 54 | 50 | `u8 data_retry_limit` = **0x0F** (15) | `IWLAGN_DEFAULT_TX_RETRY`, `dvm/tx.c:132`, `iwl-agn-hw.h:35` (probe responses: 3) |
| 55 | 51 | `u8 tid_tspec` | QoS data: TID; everything else **0x08** (`IWL_TID_NON_QOS` = `IWL_MAX_TID_COUNT` = 8) (`dvm/tx.c:76-81`, `dvm/commands.h:734`, `iwl-trans.h:271`) |
| 56 | 52 | `le16 pm_frame_timeout` | assoc/reassoc req: **3**; other mgmt: **2**; data: 0 (`dvm/tx.c:91-98`) |
| 58 | 54 | `le16 driver_txop` = 0 | `dvm/tx.c:100` |
| 60 | 56 | 802.11 MAC header, `hdr_len` bytes (24, or 26 for QoS data) | `dvm/tx.c:414` |
| 60+hdr_len | — | 2 zero pad bytes if `hdr_len` is 26 (part of TB1, §1.3) | `pcie/gen1_2/tx.c:2204-2214` |

The frame body (everything after the MAC header, starting with the CCMP header for protected
frames) is **not** copied into the command. It stays in its own buffer.

### 1.3 TB layout (`iwl_trans_pcie_tx`, `pcie/gen1_2/tx.c:2111-2310`)

For a data queue `q`, slot `wp = write_ptr` (0..255, the TFD ring index):

| TB | Address | Length | Source |
|---|---|---|---|
| TB0 | the slot's 64-byte first-TB bounce buffer | **20** (`IWL_FIRST_TB_SIZE`) | `tx.c:2223-2224`, `iwl-trans.h:712` |
| TB1 | command buffer + 20 | `tb1_len = ALIGN(56 + 4 + hdr_len - 20, 4)`: **64** for a 24-byte header, **68** for 26 | `tx.c:2204-2217, 2233-2237` |
| TB2 | frame body = `frame + hdr_len` | `frame_len - hdr_len` (omitted if 0) | `iwl_fill_data_tbs`, `tx.c:1693-1715, 2259-2261` |

`num_tbs = 3` (byte 3 of the TFD, BU §6.2). TB encoding as in BU §6.2.

Order matters, because TB0 is a *copy*:

1. Write `sequence`, `dram_lsb_ptr`/`dram_msb_ptr`, and (if `tb1_len != 40 + hdr_len`) set
   `TX_CMD_FLG_MH_PAD` (`0x00100000`) in `tx_flags` (`tx.c:2183-2214`).
2. Build TB0/TB1/TB2 in the TFD (`memset` the TFD first, `tx.c:2223-2224` with `reset = true`).
3. Only then copy buffer bytes [0, 20) into the bounce buffer (`tx.c:2270-2271`). The uCode writes
   `scratch` back into that bounce buffer (`dvm/commands.h:1206-1207`; `tx.c:2220-2221` calls it
   "bi-directional"). Keep both buffers and the frame body alive until the slot is reclaimed (§1.7).

Note the TB0 content for TX: `cmd`, `group_id`, `sequence`, `len`, `next_frame_len`, `tx_flags`,
`scratch`, `rate_n_flags`. `sta_id` onward is in TB1.

Linux never sends a REPLY_TX as a single TB. Whether one works is **UNVERIFIED**; keep the 3-TB form.

### 1.4 `tx_flags`, rate, station, security per frame type

Flag values (`dvm/commands.h:1101-1155`): PROT_REQUIRE 0x1, **ACK 0x8**, **STA_RATE 0x10**,
IMM_BA_RSP 0x40, **IGNORE_BT 0x1000**, **SEQ_CTL 0x2000**, MORE_FRAG 0x4000, TSF 0x10000,
**MH_PAD 0x100000**, AGG_CCMP 0x400000.

Rules from `iwlagn_tx_cmd_build_basic` / `_build_rate` (`dvm/tx.c:43-185`):

- **ACK** unless mac80211 says NO_ACK (`:54-57`). In station mode every frame goes to the AP
  (addr1 = BSSID), so always ACK.
- **IGNORE_BT** on 2.4 GHz with advanced BT coex (this device, BU §0) for auth, assoc req, reassoc req, and
  any frame mac80211 flags `IEEE80211_TX_CTRL_PORT_CTRL_PROTO`, i.e. EAPOL (`:63-69`, `mac80211/tx.c:552-559`).
- **SEQ_CTL** (firmware writes the sequence number) for every non-QoS frame, because mac80211 sets
  `IEEE80211_TX_CTL_ASSIGN_SEQ` for all non-QoS-data frames (`dvm/tx.c:80-86`, `mac80211/tx.c:863-866`).
  For QoS data the bit is **cleared** and the driver writes `seq_ctrl` itself from a per-(station, TID)
  counter that starts at 0 and advances by 0x10 per MSDU (`dvm/tx.c:76-79, 373-406, 438-439`).
- **sta_id**: every non-data frame uses the context's **broadcast station 15**, "to not break
  aggregation" (`dvm/tx.c:310-312`). Data frames use the destination station, i.e. the AP at
  `sta_id 0` (`IWL_AP_ID`, `dvm/tx.c:313-321`, `dvm/main.c:617`, `dvm/commands.h:727`).
- **Rate**: data frames set STA_RATE and take their rate from the station's link-quality table
  (`dvm/tx.c:138-143`), so **LQ for station 0 must exist before the first data frame** (EAPOL).
  Management frames carry a fixed rate. mac80211 picks the lowest rate the AP supports for any
  non-data frame (`mac80211/rate.c:328-336, 368-400, 428-430`). On 2.4 GHz that is normally 1 Mbps,
  so `rate_n_flags = plcp 0x0A | RATE_MCS_CCK 0x200 | ANT_A 0x4000` = **0x0000420A** (`dvm/tx.c:153-184`,
  PLCP `dvm/rs.h:103`, `dvm/commands.h:236,259`; antenna toggling over `valid_tx_ant = 0x1` always
  gives A). If the AP does not list 1 Mbps, use the lowest rate it lists (PLCP table in §2.3).
- **pm_frame_timeout**, **tid_tspec**, retry limits as in §1.2.

Resulting values:

| Frame | Queue | sta_id | tx_flags | rate_n_flags | sec_ctl | tid_tspec | pm_to |
|---|---|---|---|---|---|---|---|
| Auth (open, seq 1) | 0 | 15 | **0x00003008** (ACK\|IGNORE_BT\|SEQ_CTL) | 0x420A | 0 | 8 | 2 |
| Assoc request | 0 | 15 | **0x00003008** | 0x420A | 0 | 8 | **3** |
| Deauth / disassoc (ours) | 0 | 15 | **0x00002008** (no IGNORE_BT) | 0x420A | 0 (CCMP only with MFP, not used) | 8 | 2 |
| EAPOL, non-QoS data | 2 | 0 | **0x00003018** (ACK\|STA_RATE\|IGNORE_BT\|SEQ_CTL) | 0 | 0 | 8 | 0 |
| EAPOL, QoS data (TID 7) | 0 | 0 | **0x00101018** (ACK\|STA_RATE\|IGNORE_BT\|MH_PAD) | 0 | 0 | 7 | 0 |
| IP data, non-QoS, CCMP | 2 | 0 | **0x00002018** (ACK\|STA_RATE\|SEQ_CTL) | 0 | **0x02** + TK in `key` | 8 | 0 |
| IP data, QoS TID 0, CCMP | 2 | 0 | **0x00100018** (ACK\|STA_RATE\|MH_PAD) | 0 | **0x02** + TK | 0 | 0 |

An EAPOL frame sent after the PTK is installed (rekeying) is encrypted like data, so it gets
`sec_ctl 0x02` too (`dvm/tx.c:356-357`). For CCMP the device appends the 8-byte MIC itself. The host
supplies the 8-byte CCMP header inside the frame body (§8.3), because DVM tells mac80211
`IEEE80211_KEY_FLAG_GENERATE_IV` (`dvm/mac80211.c:610-612`) and mac80211 then inserts the header but no
MIC (`mac80211/wpa.c:455-487`).

### 1.5 Byte-count (BC) table entry (`iwl_txq_gen1_update_byte_cnt_tbl`, `pcie/gen1_2/tx.c:2058-2109`)

Written **before** the write-pointer kick (`tx.c:2275-2276` precedes `2296-2298`):

```
len = tx_cmd.len + 4 (CRC) + 4 (delimiter)          # tx.c:2058-2059, 2073
if sec_ctl & 3 == 2 (CCM): len += 8 (CCMP MIC)       # tx.c:2083-2086, ieee80211.h:2219
# NO divide-by-4: that is only for 7000..22000 families (tx.c:2095-2097); this is 6030
entry = len | (sta_id << 12)                          # tx.c:2102; len must be <= 0xFFF
BC[q*320 + wp] = entry                                 # le16, TFD_QUEUE_BC_SIZE = 256+64 = 320 (iwl-fh.h:590-594)
if wp < 64: BC[q*320 + 256 + wp] = entry               # the duplicate, tx.c:2106-2108
```

Byte address in the table = `2 * index`, so queue `q` starts at byte `q * 640` (matching BU §6.1).
Example: auth frame (24 + 6 = 30 bytes) to station 15 → `0x0026 | 0xF000` = **0xF026**.

### 1.6 Kick

`write_ptr = (wp + 1) & 0xFF`, then `CSR[0x460]` (HBUS_TARG_WRPTR) = `write_ptr | (q << 8)`
(`tx.c:2295-2298, 119-121`). It is the same plain write as for the command queue. The
wake-up branch (`tx.c:94-112`) only runs in power save (`STATUS_TPOWER_PMI`), and this driver
runs CAM (BU §9.6). The kick is skipped only for a non-last fragment (`tx.c:2278`), and this recipe
never fragments.

### 1.7 TX status and reclaim (`iwlagn_rx_reply_tx`, `dvm/tx.c:1111-1255`)

One REPLY_TX packet per frame (non-aggregated). RX packet: `cmd = 0x1C`, `sequence` bit 15
**clear**, `sequence = (queue << 8) | index` of the TFD it answers (`dvm/tx.c:1114-1116`). Payload
`struct iwlagn_tx_resp` (`dvm/commands.h:1389-1429`), offsets computed from the packed struct:

| Off | Field |
|---|---|
| 0 | `u8 frame_count` (1 = not aggregated) |
| 1 | `u8 bt_kill_count` |
| 2 | `u8 failure_rts` |
| 3 | `u8 failure_frame` (retries without ACK) |
| 4 | `le32 rate_n_flags` (rate that finally went out) |
| 8 | `le16 wireless_media_time` (µs) |
| 10 | `u8 pa_status`, then `pa_integ_res_a/b/C[3]` @11/14/17 |
| 20 | `le32 tfd_info` |
| 24 | `le16 seq_ctl` (sequence control of the frame) |
| 26 | `le16 byte_cnt` |
| 28 | `u8 tlc_info` |
| 29 | `u8 ra_tid` = TID in bits 3:0, sta_id in bits 7:4 (`dvm/commands.h:1384-1387`) |
| 30 | `le16 frame_ctrl` |
| 32 | `le16 status` (low byte = `TX_STATUS_*`, bits 7:0 mask `0xFF`, `dvm/commands.h:1310`) |
| 34 | `le16 sequence` |
| 36 | `le32 scd_ssn` (only with `frame_count = 1` at this offset: `(__le32 *)&status + frame_count`), use `& 0xFFF` (`dvm/tx.c:914-918`) |

Success is `status & 0xFF` equal to **0x01** (SUCCESS) or **0x02** (DIRECT_DONE)
(`dvm/agn.h:180-185`). Failure codes in §9.

Reclaim (`iwl_pcie_reclaim`, `pcie/gen1_2/tx.c:2338-2430`): `tfd_num = scd_ssn & 0xFF`. Free every slot
from the queue's `read_ptr` up to but **not including** `tfd_num`. For each freed slot `i`, write the
"invalid" BC entry `1 | (sta_id << 12)` at `q*320 + i` (and at `q*320 + 256 + i` if `i < 64`)
(`tx.c:2312-2336, 2419-2421`). Then `read_ptr = tfd_num`. Linux also sanity-checks that
`tfd_num - 1` lies in `[read_ptr, write_ptr)` (`tx.c:2384-2397`). With one frame in flight this is
just `read_ptr = index + 1`.

---

## 2. Before authentication: tune to the BSS and add the AP station

### 2.1 What Linux does, in order

mac80211 `ieee80211_prep_connection` (`mac80211/mlme.c:8929-9115`):

1. `ieee80211_prep_channel` → channel change → DVM `iwlagn_mac_config`
   (`dvm/rxon.c:1152-1252`): staging channel and band flags, then `commit_rxon` → **RXON (not
   associated)** on the AP's channel. BSSID still 0. Each un-associated RXON wipes the firmware station table,
   so DVM re-adds broadcast station 15 and its LQ (`dvm/rxon.c:340-380`, `dvm/sta.c:642-718`).
2. `BSS_CHANGED_BSSID | BASIC_RATES | BEACON_INT` (`mac80211/mlme.c:9087-9095`) → DVM
   `iwlagn_bss_info_changed` (`dvm/rxon.c:1383-1494`): `bssid_addr` = AP, `filter_flags |= BCON_AWARE`
   while not associated (`:1485-1491`), `force` because BEACON_INT changed (`:1415-1416`) →
   **RXON (not associated)** again, now with the BSSID → broadcast station 15 + LQ(15) restored again.
3. `sta_info_insert` → `sta_state NOTEXIST→NONE` → **REPLY_ADD_STA for the AP, sta_id 0, no LQ**
   (`mac80211/mlme.c:9100`, `dvm/mac80211.c:840-874`, `dvm/sta.c:237-379`). The comment at
   `dvm/rxon.c:1114-1118` says the AP station must be added **after** the BSSID is in the RXON
   "to correctly set up filters in the device".
4. Linux sends **no** RXON_TIMING, QoS or PAN command on this path. Timing goes out only before the
   *associated* RXON (`dvm/rxon.c:446-453`), and `REPLY_WIPAN_PARAMS` is suppressed while the PAN
   context is inactive (`dvm/rxon.c:509-533`, called from `:1124`).
5. TX power: Linux does not resend `REPLY_TX_POWER_DBM_CMD` (0x95) after these un-associated RXONs
   (no call in `dvm/rxon.c:340-380, 1114-1131`). It does after the associated RXON
   (`dvm/rxon.c:493-504`). The firmware header says the power command "must be executed after every
   RXON command, before Tx can occur" (`dvm/commands.h:1090-1091`). Re-sending 0x95 (BU §9.7) after
   each RXON costs one command. Whether it is needed before auth is **UNVERIFIED**; send it.

You may collapse steps 1-2 into one RXON (channel + BSSID). Keep the RXON before the AP ADD_STA.

**Preconditions:** no scan running (DVM ignores config changes while scanning, `dvm/rxon.c:1164-1167`;
mac80211 cancels scans, `mac80211/mlme.c:9111-9113`). Wait for SCAN_COMPLETE (0x84, BU §10.2) first.

### 2.2 Pre-auth RXON (0x10), 50 bytes (layout BU §9.5)

| Off | Field | Value |
|---|---|---|
| 0 | `node_addr` | our MAC |
| 8 | `bssid_addr` | AP BSSID (`dvm/rxon.c:1464`) |
| 16 | `wlap_bssid_addr` | 0 |
| 24 | `dev_type` | **0x03** (ESS) |
| 26 | `rx_chain` | **0x2406** for `valid_rx_ant 0x3` (non-HT: SMPS static → idle 1, active 2, BU §9.5; `dvm/lib.c:773-834`) |
| 28 | `ofdm_basic_rates` | from the AP's basic rates, §2.3; **0x15** for a normal b/g AP |
| 29 | `cck_basic_rates` | §2.3; **0x0F** for a normal b/g AP |
| 30 | `assoc_id` | 0 |
| 32 | `flags` | **0x00008005** (BAND_24G\|AUTO_DETECT\|TSF2HOST). The BG-only variant adds SELF_CTS 0x40000000 (BU §9.5). SHORT_SLOT (0x10) stays clear: `bss_conf.use_short_slot` is only learned at association (`dvm/rxon.c:716-737, 1041-1058`) |
| 36 | `filter_flags` | **0x00000044** (ACCEPT_GRP 0x4 \| BCON_AWARE 0x40). DIS_DECRYPT (0x8) stays clear (HW crypto, `dvm/rxon.c:739-749, 1103`) |
| 40 | `channel` | AP channel |
| 42..44 | HT basic rates ×3 | 0xFF 0xFF 0xFF |
| 45..49 | rest | 0 |

Then, as in BU §9.5: **ADD_STA(15)** (broadcast, mode 0) → **LQ(15)**.

`iwl_check_rxon_cmd` (`dvm/rxon.c:752-822`) refuses: multicast node/BSSID, neither 1M nor 6M basic,
`assoc_id > 2007`, CCK+short slot, CCK+auto-detect, TGG_PROTECT without AUTO_DETECT, channel 0.

### 2.3 Basic ("ACK") rate bitmaps (`iwl_calc_basic_rates`, `dvm/rxon.c:914-1003`)

Driver rate index (`dvm/rs.h:34-47`) and PLCP (`dvm/rs.h:94-106`):

| Rate (Mbps) | 1 | 2 | 5.5 | 11 | 6 | 9 | 12 | 18 | 24 | 36 | 48 | 54 |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| index | 0 | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 10 | 11 |
| PLCP | 0x0A | 0x14 | 0x37 | 0x6E | 0x0D | 0x0F | 0x05 | 0x07 | 0x09 | 0x0B | 0x01 | 0x03 |
| IE byte (500 kb/s) | 0x02 | 0x04 | 0x0B | 0x16 | 0x0C | 0x12 | 0x18 | 0x24 | 0x30 | 0x48 | 0x60 | 0x6C |

`cck` = bit `idx` for each basic CCK rate (index 0-3); `ofdm` = bit `idx-4` for each basic OFDM rate.
Basic rates are the AP's Supported/Extended Supported Rates bytes with bit 7 (0x80) set. Then:
- if no basic OFDM rate is below 24M (index 8): `ofdm |= 0x10` (24M); if none below 12M: `ofdm |= 0x04` (12M); always `ofdm |= 0x01` (6M);
- if no basic CCK rate is below 11M: `cck |= 0x08`; below 5.5M: `|= 0x04`; below 2M: `|= 0x02`; always `cck |= 0x01`.

A typical AP with basic {1, 2, 5.5, 11} gives cck **0x0F**, ofdm **0x15**.

### 2.4 REPLY_ADD_STA (0x18) for the AP, 92 bytes

`struct iwl_addsta_cmd` (`dvm/commands.h:841-875`), offsets from the packed struct:

| Off | Field | AP add (pre-auth) |
|---|---|---|
| 0 | `u8 mode` | **0** (add) (`dvm/sta.c:296`) |
| 1-3 | reserved | 0 |
| 4 | `sta.addr[6]` | AP BSSID |
| 10 | `sta.reserved1` | 0 |
| 12 | `sta.sta_id` | **0x00** (`ctx->ap_sta_id`, `dvm/sta.c:244-245`) |
| 13 | `sta.modify_mask` | 0 |
| 14 | `sta.reserved2` | 0 |
| 16 | `key.key_flags` (le16) | 0 |
| 18 | `key.tkip_rx_tsc_byte2` | 0 |
| 19 | `key.reserved1` | 0 |
| 20 | `key.tkip_rx_ttak[5]` (le16 ×5) | 0 |
| 30 | `key.key_offset` | 0 |
| 31 | `key.reserved2` | 0 |
| 32 | `key.key[16]` | 0 |
| 48 | `key.tx_secur_seq_cnt` (le64) | 0 |
| 56 | `key.hw_tkip_mic_rx_key` (le64) | 0 |
| 64 | `key.hw_tkip_mic_tx_key` (le64) | 0 |
| 72 | `station_flags` (le32) | `ctx->station_flags` = **0** for BSS (`dvm/sta.c:298`) + HT flags (§4.2), so 0 pre-assoc |
| 76 | `station_flags_msk` (le32) | 0 |
| 80 | `tid_disable_tx` (le16) | 0 |
| 82 | `legacy_reserved` | 0 |
| 84 | `add_immediate_ba_tid` | 0 |
| 85 | `remove_immediate_ba_tid` | 0 |
| 86 | `add_immediate_ba_ssn` (le16) | 0 |
| 88 | `sleep_tx_count` (le16) | 0 |
| 90 | `reserved2` | 0 |

Response payload `u8 status`, must be **0x01**. Other values: 0x02 no room, 0x04 no BA resource,
0x08 modify of a non-existent station (`dvm/commands.h:878-887`, `dvm/sta.c:43-73, 113-118`).

Linux sends **no LQ for station 0 yet**. It is sent at association (`dvm/mac80211.c:864-873`,
§4.2). Management frames don't need it (they use station 15 and a fixed rate).

### 2.5 Wait for a beacon before the first unicast frame

If the channel is passive (NO_IR in the NVM, BU §4.3), the firmware refuses to transmit until it has
heard a beacon after the RXON. TX status is then `TX_STATUS_FAIL_PASSIVE_NO_RX` = **0x90**, Linux
stops all queues until a beacon from `active.bssid_addr` arrives (`dvm/tx.c:1193-1200`,
`dvm/rx.c:653-667`). BCON_AWARE in the pre-assoc filter exists for this: "it will lose beacons that are
needed before sending frames out on passive channels" (`dvm/rxon.c:1477-1491`). After the RXON, poll RX until a beacon
(fc `0x0080`) with addr3 = BSSID arrives, then send the auth frame. (mac80211 also waits for a
beacon before associating because DVM sets `NEED_DTIM_BEFORE_ASSOC`, `dvm/mac80211.c:90`.)

---

## 3. Authentication and association frames

All management frames: `frame_control` (le16) at 0, `duration` at 2, `da` at 4,
`sa` at 10, `bssid` at 16, `seq_ctrl` at 22 (0; firmware assigns because of SEQ_CTL), body at 24
(`struct ieee80211_mgmt`, `ieee80211.h:1162-1192`).

**Duration field.** Linux never sends 0 there: mac80211 writes a computed duration into every
header (`ieee80211_tx_h_calculate_duration`, `mac80211/tx.c:1080-1103, 1862`). The examples below
write 0 and rely on the uCode overwriting it. The only hint that it does is the comment on
`TX_CMD_FLG_DUR_MSK` ("HCCA-AP - disable duration overwriting", `dvm/commands.h:1154-1155`).
**UNVERIFIED.** If frames are ignored, fill it in: an ACKed unicast frame needs SIFS + ACK time at
the control-response rate **[GK, 802.11-2016 §10.27]** (314 µs is safe at 1 Mbps long preamble).

### 3.1 Open-system authentication request (`ieee80211_send_auth`, `mac80211/util.c:1076-1122`)

30 bytes:

```
00: B0 00                  frame_control = FTYPE_MGMT 0x0000 | STYPE_AUTH 0x00B0 (ieee80211.h:59,74)
02: 00 00                  duration
04: <BSSID>                da
0A: <our MAC>              sa
10: <BSSID>                bssid
16: 00 00                  seq_ctrl
18: 00 00                  auth_alg = WLAN_AUTH_OPEN 0 (ieee80211.h:1663)
1A: 01 00                  auth_transaction = 1
1C: 00 00                  status_code = 0
```

Expected response: fc `0x00B0`, addr1 = our MAC, `auth_alg 0`, `auth_transaction 2` @26,
`status_code` @28 = 0 (`WLAN_STATUS_SUCCESS`, `ieee80211.h:1751`). mac80211 waits 200 ms (`HZ/5`)
per try, 3 tries (`mac80211/mlme.c:36-40`).

### 3.2 Association request (`ieee80211_send_assoc`, `mac80211/mlme.c:2150-2340`)

```
00: 00 00                  frame_control = STYPE_ASSOC_REQ 0x0000 (ieee80211.h:65)
02: 00 00                  duration
04: <BSSID> <our MAC> <BSSID>  (mlme.c:2245-2247)
16: 00 00                  seq_ctrl
18: capab_info (le16)
1A: listen_interval (le16) = 0x000A
1C: IEs
```

- **capab_info** = ESS 0x0001 (`mlme.c:2285-2287`) | PRIVACY 0x0010 if the BSS has it
  (`mlme.c:2202-2203`) | SHORT_SLOT_TIME 0x0400 | SHORT_PREAMBLE 0x0020 on 2.4 GHz
  (`mlme.c:1783-1786`) = **0x0431** for a WPA2 AP (`ieee80211.h:1674-1695`). DVM advertises
  SPECTRUM_MGMT (`dvm/mac80211.c:91`), so mac80211 would add 0x0100 plus Power Capability and Supported
  Channels IEs if the AP sets that capability bit (`mlme.c:1788-1831`). That is rare on 2.4 GHz.
  A minimal client may leave it out (**UNVERIFIED** whether any 2.4 GHz AP requires it).
- **listen_interval** = `hw.max_listen_interval` = `IWL_CONN_MAX_LISTEN_INTERVAL` = **10**
  (`mac80211/main.c:1514`, `dvm/mac80211.c:182`, `dvm/commands.h:597`).
- **IE order** (`mlme.c:1586-1599, 2270-2324`):
  1. SSID: `00 <len> <ssid>` (`mlme.c:2271-2275`).
  2. Supported Rates: `01 <n≤8> ...` and Extended Supported Rates: `32 <n> ...`. Linux sends the
     intersection of its rates and the AP's, **without** the 0x80 basic bit (it passes `basic_rates = 0`)
     (`mlme.c:1544-1576`, `mac80211/util.c:3263-3316`). Device rate order is 1, 2, 5.5, 11, 6, 9, 12, 18 |
     24, 36, 48, 54. For an AP with all 12: `01 08 02 04 0B 16 0C 12 18 24` and `32 04 30 48 60 6C`.
  3. RSN IE (from wpa_supplicant's `assoc_data->ie`; goes before HT, `mlme.c:1592`). For WPA2-PSK/CCMP **[GK, 802.11-2016 §9.4.2.25]**:
     ```
     30 14  01 00  00 0F AC 04  01 00 00 0F AC 04  01 00 00 0F AC 02  00 00
     ```
     version 1; group cipher 00-0F-AC:4 (CCMP, `ieee80211.h:2515`); 1 pairwise: CCMP; 1 AKM:
     00-0F-AC:2 (PSK, `ieee80211.h:2529`); RSN Capabilities 0x0000. **The group cipher must equal the
     AP's** (copy it from the AP's RSN IE). Keep these exact bytes: they are repeated in EAPOL message 2 (§5.4).
  4. HT Capabilities (`mlme.c:1838-1845`), only if the connection mode is HT.
  5. WMM information element (`mlme.c:2308-2318`, `mac80211/util.c:4290-4303`), only if the AP
     advertises WMM: `DD 07 00 50 F2 02 00 01 00` (QoS info 0 = no U-APSD).

**Minimal-client recommendation (my choice, not Linux's default):** leave out HT Capabilities
**and** the WMM IE. The AP then associates you as a legacy, non-QoS 11b/g station: it may not send
you QoS data, A-MSDU or aggregates, and you send only non-QoS data with firmware-assigned sequence
numbers. HT is optional for WPA2-CCMP (CCMP itself is 802.11i). APs configured "n-only" or "WMM
required" will reject this with status 12/18/… (**UNVERIFIED** per AP). Still parse 26-byte QoS headers on RX (§7.2).

### 3.3 Association response

fc `0x0010` (`ieee80211.h:66`). Body @24: `capab_info` (le16), `status_code` (le16) @26, `aid` (le16) @28,
then IEs (Supported Rates, Ext Rates, ERP 42, WMM parameter 221/00:50:F2/2, HT…)
(`ieee80211.h:1186-1192`). `aid = le16 @28 & 0x07FF` (the top 5 bits are reserved,
"802.11-2016 9.4.1.8", `mac80211/mlme.c:6554-6566`). Useful status codes: 0 success, 1 unspecified,
10 capabilities unsupported, 12 denied, 13 auth alg unsupported, 17 AP full, 18 basic rates
unsupported, 40 invalid IE, 41/42/43 invalid group/pairwise cipher / AKMP, 45 invalid RSN caps
(`ieee80211.h:1751-1782`).

From the response and the beacon, keep: AID, `use_short_slot` (capab 0x0400), `use_short_preamble`
(capab 0x0020, also ERP Barker bit), `use_cts_prot` (ERP IE bit 1 "Use_Protection"
**[GK, §9.4.2.12]**), WMM parameters if present, beacon interval, DTIM period, beacon TSF.

Deauthentication (fc `0x00C0`) / disassociation (`0x00A0`) from the AP: `reason_code` le16 @24
(`ieee80211.h:73,75,1177-1179`). Reasons in §9.

---

## 4. After association: what Linux sends, in order

### 4.1 Sequence

mac80211 `ieee80211_assoc_success` (`mac80211/mlme.c:6271-6451`): QoS parameters to the driver
(`conf_tx` ×4), then `sta_info_move_state(ASSOC)` (`:6437`), then `ieee80211_set_associated`
(`:6451`) → `BSS_CHANGED_ASSOC` (plus QOS, etc.). DVM turns that into:

1. `sta_state AUTH→ASSOC` → `iwl_sta_update_ht` → **ADD_STA modify** for station 0 (§4.2) →
   `iwl_rs_rate_init` → **LQ(0)** (§4.3) (`dvm/mac80211.c:889-898`).
2. `iwlagn_bss_info_changed` (`dvm/rxon.c:1383-1494`): `assoc_id = AID`, short-preamble flag, if
   ASSOC: `filter |= ASSOC` and `priv->timestamp = bss_conf->sync_tsf` (`:1423-1438`); TGG_PROTECT and
   SELF_CTS from `use_cts_prot` (`:1454-1462`); BCON_AWARE cleared once associated (`:1485-1491`); if
   BSS_CHANGED_QOS, send **QOS_PARAM** now (`:1418-1421`). Then `commit_rxon` (`:1493-1494`). The
   ASSOC bit toggled, so a full RXON is required (`dvm/rxon.c:877-879`), and that means:
   1. **RXON with ASSOC cleared** first, but with all the new settings (`iwlagn_disable_bss`,
      `dvm/rxon.c:79-97, 1114-1120`). This wipes the station table again (`dvm/commands.h:826-831`).
   2. Restore **ADD_STA(15) + LQ(15)** and **ADD_STA(0) + LQ(0)** from the driver's copies
      (`dvm/rxon.c:364-371`, `dvm/sta.c:642-718`; station 0 has an LQ by now).
   3. **REPLY_RXON_TIMING** 0x14 (§4.4), "must be before associated RXON" (`dvm/rxon.c:446-453`).
   4. **REPLY_QOS_PARAM** 0x13 (§4.5) (`dvm/rxon.c:454-455`).
   5. **RXON with ASSOC set** (§4.6) (`dvm/rxon.c:479-485`). An associated RXON does not clear stations (`:476-478`).
   6. `iwl_init_sensitivity` → **SENSITIVITY_CMD 0xA8** (`dvm/rxon.c:491`, `dvm/calib.c:538-593`).
      It sets detector thresholds from per-device ranges. I did not trace its payload. Whether
      association works without it is **UNVERIFIED** (likely: it only tunes the receiver).
   7. **REPLY_TX_POWER_DBM_CMD 0x95**, forced (`dvm/rxon.c:493-504`), same payload as BU §9.7.
3. Chain-noise calibration (REPLY_PHY_CALIBRATION_CMD) after association (`dvm/rxon.c:1496-1508, 1348-1381`).
   It is a tuning step. **UNVERIFIED** whether it is needed. Skip it in a first port.

A port may skip step 1 and send the final ADD_STA(0) with the right flags directly in step 2.2.
**Keep the order unassoc RXON → stations → timing → QoS → assoc RXON**, and make sure LQ(0) is in
place before the first EAPOL frame. The AP may send EAPOL message 1 while you are still doing this.
Messages are retransmitted, but don't process EAPOL until the assoc RXON is done.

### 4.2 Station-0 flags (`iwl_sta_calc_ht_flags`, `dvm/sta.c:145-217`)

`station_flags_msk` = RTS_MIMO_PROT 0x00020000 | MIMO_DIS 0x00400000 | HT40_EN 0x00200000 |
MAX_AGG_SIZE 0x00180000 | AGG_MPDU_DENSITY 0x03800000 = **0x03FA0000** (`dvm/commands.h:736-746`).
Legacy (non-HT) AP or no HT in our assoc request: `station_flags = 0`. HT AP: MIMO_DIS if the AP
is in static SMPS, RTS_MIMO_PROT if dynamic, `ampdu_factor << 19`, `ampdu_density << 23`, HT40_EN if
40 MHz. The modify command: `mode = 0x01`, `sta_id = 0`, `station_flags`, `station_flags_msk`,
everything else 0, **`modify_mask = 0`** (`dvm/sta.c:210-216`). The restored add (§4.1 step 2.2)
re-sends the stored station entry: `mode 0`, addr, sta_id 0, the updated `station_flags`,
`station_flags_msk 0` (`dvm/sta.c:206-207, 294-298, 667, 675-689`).

### 4.3 LQ for station 0 (0x4E, 88 bytes)

Linux's first table (`iwl_rs_rate_init` → `rs_initialize_lq` → `rs_fill_link_cmd(NULL, …)`,
`dvm/rs.c:2740-2836, 2607-2657, 2838-2984`) starts at the lowest rate the AP supports
(`rate_lowest_index`) and steps down with `rs_get_lower_rate`, which stays on the current rate
when there is no lower one (`dvm/rs.c:801-806`). Called with `priv = NULL`, it never
toggles antennas. For an AP whose lowest rate is 1 Mbps that is **sixteen entries of 0x0000420A**,
i.e. the same table as LQ(15) (BU §9.5), except:
`sta_id@0 = 0x00`; `agg_params.agg_frame_cnt_limit@15 = 0x3F` (`LINK_QUAL_AGG_FRAME_LIMIT_DEF` 63,
`dvm/rs.c:2969-2970`, `dvm/commands.h:1511`); `agg_dis_start_th@14 = 3`; `agg_time_limit@12 = 4000`.
`general_params`: `single_stream_ant_msk@6 = 0x01`, `dual_stream_ant_msk@7 = 0x03`
(`dvm/rs.c:2810-2820`), `mimo_delimiter@5 = 0`. Afterwards Linux's rate-scaling algorithm rewrites
the table from TX statuses. A port with no rate scaling can send a fixed descending legacy table instead,
e.g. 54/48/36/24/18/12/6 OFDM (PLCP 0x03, 0x01, 0x0B, 0x09, 0x07, 0x05, 0x0D, no CCK bit), then
11/5.5/2/1 CCK (0x6E, 0x37, 0x14, 0x0A | 0x200), padded with 1 Mbps, each `| 0x4000`. Linux never
sends that table, so it is **UNVERIFIED**. Only list rates the AP supports. Without HT, no entry may have
`RATE_MCS_HT 0x100` (`dvm/sta.c:782-803`).

### 4.4 REPLY_RXON_TIMING (0x14), 20 bytes (`dvm/commands.h:603-611`, built in `dvm/rxon.c:272-338`)

| Off | Field | Value |
|---|---|---|
| 0 | `le64 timestamp` | `bss_conf->sync_tsf` = the 8-byte timestamp of the AP's last beacon/probe response (frame offset 24) (`mac80211/mlme.c:9040-9058`, `dvm/rxon.c:287, 1431`) |
| 8 | `le16 beacon_interval` | AP beacon interval in TU, e.g. 0x0064. Unchanged if ≤ 4096 (`iwl_adjust_beacon_interval`, `dvm/rxon.c:237-270, 316-318`; max `4 * TIME_UNIT`, `dvm/commands.h:598`, `dvm/agn.h:33`). 0 → 200 (`dvm/dev.h:66`) |
| 10 | `le16 atim_window` | 0 |
| 12 | `le32 beacon_init_val` | `interval_tm - (timestamp mod interval_tm)`, `interval_tm = beacon_interval * 1024` µs (`dvm/rxon.c:323-326`) |
| 16 | `le16 listen_interval` | 10 (`hw->conf.listen_interval`, `dvm/rxon.c:288`) |
| 18 | `u8 dtim_period` | from the TIM IE, or 1 (`dvm/rxon.c:328`) |
| 19 | `u8 delta_cp_bss_tbtts` | 0 |

### 4.5 REPLY_QOS_PARAM (0x13), 36 bytes (`dvm/commands.h:690-715`, `dvm/rxon.c:152-178`)

```
@0  le32 qos_flags = (AP has WMM ? 0x01 UPDATE_EDCA : 0) | (HT enabled ? 0x02 TGN : 0)
@4  ac[0] = background, @12 ac[1] = best effort, @20 ac[2] = video, @28 ac[3] = voice
    each: le16 cw_min, le16 cw_max, u8 aifsn, u8 reserved=0, le16 edca_txop (µs)
```

The array order is reversed relative to mac80211's ACs: `ac[3 - mac80211_ac]`
(`dvm/mac80211.c:1170-1182`). `edca_txop = txop_in_32us_units * 32`. Values:

- AP without WMM: mac80211 `ieee80211_set_wmm_default(…, enable_qos = false)` gives every AC
  `cw_min 15, cw_max 1023, aifs 2, txop 0`, or cw_min 31 for an 11b-only BSS (`mac80211/util.c:964-1005`).
  Flags 0. Payload: `00 00 00 00` + 4 × `0F 00 FF 03 02 00 00 00`.
- AP with WMM: use its WMM Parameter element, `cw = 2^ECW - 1`, AIFSN, TXOP limit.
  **[GK, WMM spec / 802.11-2016 §9.4.2.29]** (mac80211's parser is `ieee80211_sta_wmm_params` in `mlme.c`, not traced field by field).

### 4.6 Associated RXON (0x10)

Same as §2.2 except:

| Off | Field | Value |
|---|---|---|
| 30 | `assoc_id` | AID (`dvm/rxon.c:1423`) |
| 32 | `flags` | 0x00008005 \| SHORT_SLOT 0x10 if `use_short_slot` \| SHORT_PREAMBLE 0x20 if `use_short_preamble` \| TGG_PROTECT 0x08 and SELF_CTS 0x40000000 if `use_cts_prot` (`dvm/rxon.c:1054-1058, 1424-1427, 1454-1462`, bits `dvm/commands.h:473-519`) |
| 36 | `filter_flags` | **0x00000024** (ACCEPT_GRP 0x4 \| ASSOC 0x20) |

The un-associated RXON of §4.1 step 2.1 is this structure with `filter_flags = 0x04`
(`iwlagn_disable_bss` clears only ASSOC, `dvm/rxon.c:86`).

After this, mac80211 moves the station to AUTHORIZED when the 4-way handshake finishes. DVM does
nothing for that transition (`dvm/mac80211.c:840-857`).

---

## 5. WPA2-PSK 4-way handshake (host side) — all **[GK]**

### 5.1 PSK / PMK

`PMK = PSK = PBKDF2-HMAC-SHA1(passphrase, SSID, 4096 iterations, 32 bytes)` **[GK, 802.11-2016 Annex J.4.1]**.
The passphrase is 8..63 ASCII characters. The salt is the raw SSID bytes. PBKDF2 needs two 20-byte blocks
(`T1 || T2[0..12)`), each 4096 HMAC iterations. **Precompute it before associating.** Done between
messages 1 and 2 it can blow the AP's retransmit window (hostapd's is on the order of a second, **UNVERIFIED** for your AP).

### 5.2 PTK

`PRF-384(PMK, "Pairwise key expansion", Min(AA,SPA) || Max(AA,SPA) || Min(ANonce,SNonce) || Max(ANonce,SNonce))`
**[GK, §12.7.1.3]**, where AA = AP MAC (BSSID), SPA = our MAC, and Min/Max compare byte strings
lexicographically. `PRF-n(K, A, B)` = concatenation of `HMAC-SHA1(K, A || 0x00 || B || i)` for
`i = 0, 1, 2, …` (one byte), truncated to n bits **[GK, §12.7.1.2]**. CCMP needs 384 bits = 48 bytes
(three HMACs). PRF-512 is for TKIP.

`KCK = PTK[0..16)`, `KEK = PTK[16..32)`, `TK = PTK[32..48)` **[GK, §12.7.1.3]**.

SNonce: 32 random bytes. You need an entropy source (e.g. RDRAND where present, or a
DRBG seeded from TSC jitter/RF noise). A predictable SNonce weakens only this session's keys, but do it properly.

### 5.3 EAPOL-Key frame format **[GK, §12.7.2 and IEEE 802.1X-2010]**

Carried in an 802.11 data frame after LLC/SNAP `AA AA 03 00 00 00 88 8E` (`ETH_P_PAE 0x888E`,
`include/uapi/linux/if_ether.h:88`). Offsets from the start of the EAPOL PDU, **multi-byte fields big-endian**:

| Off | Len | Field |
|---|---|---|
| 0 | 1 | Protocol Version (AP usually 0x01 or 0x02. Mirror the AP's, or send 0x01) |
| 1 | 1 | Packet Type = **0x03** (EAPOL-Key) |
| 2 | 2 | Packet Body Length = 95 + Key Data Length |
| 4 | 1 | Descriptor Type = **0x02** (RSN) |
| 5 | 2 | Key Information |
| 7 | 2 | Key Length (16 in messages 1 and 3 for CCMP; 0 in our messages 2 and 4) |
| 9 | 8 | Key Replay Counter |
| 17 | 32 | Key Nonce |
| 49 | 16 | EAPOL-Key IV (0) |
| 65 | 8 | Key RSC (in message 3: the GTK's current PN, for group replay detection) |
| 73 | 8 | Reserved (0) |
| 81 | 16 | Key MIC |
| 97 | 2 | Key Data Length |
| 99 | n | Key Data |

Key Information bits: 2:0 descriptor version (**2** = HMAC-SHA1-128 MIC + AES key wrap, used with
AKM 00-0F-AC:2 and CCMP), 3 Key Type (1 = pairwise), 6 Install, 7 Key Ack, 8 Key MIC, 9 Secure,
10 Error, 11 Request, 12 Encrypted Key Data.

| Message | Key Information | Notes |
|---|---|---|
| 1 (AP→us) | **0x008A** | ANonce, replay counter R. Key Data may hold a PMKID KDE (ignore) |
| 2 (us→AP) | **0x010A** | replay counter R, Nonce = SNonce, MIC, Key Data = our RSN IE |
| 3 (AP→us) | **0x13CA** | install, ack, MIC, secure, encrypted; replay counter R+1 or more; ANonce again |
| 4 (us→AP) | **0x030A** | MIC, secure; replay counter = message 3's; Nonce 0; no Key Data |
| Group msg 1 (AP rekey) | 0x1382 | GTK KDE encrypted; reply 0x0302 with MIC, replay counter copied |

**MIC** = first 16 bytes of `HMAC-SHA1(KCK, entire EAPOL PDU from offset 0 to the end of Key Data,
with the 16 MIC bytes set to 0)`.

### 5.4 Handshake steps

1. **Message 1**: check Descriptor Type 2, version bits 2, Key Ack set, MIC clear. Save ANonce and
   replay counter. Generate SNonce, derive the PTK (§5.2).
2. **Message 2**: Key Data = the RSN IE *byte-identical* to the one in your association request
   (22 bytes for §3.2's IE), so Key Data Length 22, body length 117, PDU 121 bytes. Compute the MIC, send
   as data to the AP **unencrypted** (§1.4, EAPOL row). Wrong MIC (wrong passphrase or PTK math)
   → the AP silently drops it and later resends message 1, then deauths with reason 15.
3. **Message 3**: verify ANonce unchanged, replay counter greater than before, MIC with KCK.
   Unwrap Key Data with **AES-128 Key Unwrap (RFC 3394) under KEK**, default IV `A6A6A6A6A6A6A6A6`.
   Wrapped length = Key Data Length. Plaintext = wrapped − 8. The plaintext holds:
   - the AP's RSN IE (must match the beacon's; a mismatch should abort);
   - the **GTK KDE**: `DD <len> 00 0F AC 01 <b0> <b1> <GTK…>`, where `b0` bits 1:0 = GTK key ID
     (usually 1 or 2), bit 2 = Tx, `b1` reserved, then 16 bytes of GTK for CCMP **[GK, §12.7.2, Table 12-6 / Figure 12-37]**;
   - optional padding `DD 00 00 …` to a multiple of 8.
4. **Message 4**: Key Information 0x030A, replay counter from message 3, Nonce 0, Key Data Length 0
   (PDU 99 bytes), MIC. Send it **before** installing the PTK, so it goes out unencrypted. Wait for its
   REPLY_TX success. (wpa_supplicant's order: send 4/4, then install. **[GK]**)
5. Install PTK and GTK (§6). From now on unicast data is CCMP-protected in both directions.
   Group-addressed frames from the AP use the GTK with the key ID from the KDE. The initial receive PN for the
   GTK is message 3's Key RSC (little-endian 6 bytes in its first 6 octets **[GK]**).

---

## 6. Installing keys in the firmware (REPLY_ADD_STA 0x18, key modify)

### 6.1 What Linux sends (`iwl_set_dynamic_key` → `iwlagn_send_sta_key`, `dvm/sta.c:1201-1257, 1077-1131`)

- **Which station.** The PTK goes on the AP station, **sta_id 0**. In station mode **the GTK also goes on
  station 0**, not on broadcast station 15: "The device expects GTKs for station interfaces to be
  installed as GTKs for the AP station" (`dvm/sta.c:1057-1075`).
- **Key slot.** `key_offset` = the first free bit of the driver's key-table bitmap, in `0 .. sta_key_max_num-1`
  (`dvm/sta.c:720-729, 1217-1219`). With the firmware PAN flag and IPAN, `sta_key_max_num = 16`
  (`STA_KEY_MAX_NUM_PAN`, `dvm/main.c:1352-1353`, `dvm/commands.h:767`). With no WEP keys, the first
  key (PTK) gets **0** and the second (GTK) **1**. A GTK rekey takes the next free slot, then the old one is
  freed (`dvm/sta.c:1182-1184`).
- **Command**: a copy of the station's stored add command (§2.4 / §4.2) (`dvm/sta.c:1087-1089`), then:

| Off | Field | PTK | GTK (key ID k) |
|---|---|---|---|
| 0 | `mode` | **0x01** (`STA_CONTROL_MODIFY_MSK`) | 0x01 |
| 4 | `sta.addr` | BSSID (from the copy) | BSSID |
| 12 | `sta.sta_id` | 0x00 | 0x00 |
| 13 | `sta.modify_mask` | **0x01** (`STA_MODIFY_KEY_MASK`) | 0x01 |
| 16 | `key.key_flags` (le16) | `(0 << 8) \| 0x0008 MAP_KEY \| 0x0002 CCMP` = **0x000A** | `(k << 8) \| 0x0008 \| 0x0002 \| 0x4000 MULTICAST`, e.g. k=1 → **0x410A** |
| 30 | `key.key_offset` | 0x00 | 0x01 |
| 32 | `key.key[16]` | TK | GTK |
| 72 | `station_flags` | from the copy | from the copy |
| 76 | `station_flags_msk` | 0 (from the copy) | 0 |

Constants: `dvm/commands.h:752-777`; `key_flags` build `dvm/sta.c:1091-1097, 1120-1128`.
`tx_secur_seq_cnt` is left 0 (Linux never sets it for CCMP). TX PNs come from the CCMP header the host
writes (§8.3). Response status must be 0x01. Removing a key: same command with `key_flags` =
`(idx << 8) | 0x0008 | 0x0800 INVALID` (+ 0x4000 for a group key) and the same offset (`dvm/sta.c:1186-1198`).

mac80211 installs the pairwise key first, then the group key (wpa_supplicant's order **[GK]**).
Linux does not store the key in its station copy (`iwlagn_send_sta_key` works on a local copy), so an
un-associated RXON loses the keys. In this recipe no RXON is sent after the keys.

### 6.2 TX side of the key

The TX command does not refer to the firmware key slot. It carries the TK itself: `sec_ctl = 0x02`,
`key[16] = TK` (`dvm/tx.c:192-200`). The station table key is used for **RX decryption**. A station
never transmits with the GTK; multicast from a STA goes to the AP as unicast (addr1 = BSSID) under the PTK.

---

## 7. Receiving data

### 7.1 Packet and status

Data frames arrive exactly like beacons (BU §10.2): a 0xC0 `REPLY_RX_PHY_CMD`, then a 0xC1
`REPLY_RX_MPDU_CMD` with `le16 byte_count L` @0, the 802.11 frame @4 (L bytes), `le32 rx_pkt_status`
@4+L (`dvm/rx.c:775-823`). Reject on `status & 3 != 3` as now. There is **no FCS** in the frame
(`RX_INCLUDES_FCS` is only set for a monitor interface, `dvm/mac80211.c:1310-1322`) and **no pad**
after a 26-byte QoS header (DVM hands `hdr`/`len` straight to mac80211, `dvm/rx.c:604-671`).

Hardware decryption, raw `rx_pkt_status` bits as tested by `iwlagn_translate_rx_status`
(`dvm/rx.c:674-731`, masks `dvm/commands.h:1002-1021`):

| Bits | Meaning |
|---|---|
| 10:8 (`& 0x700`) | security type: 0 none, 1 WEP, **2 CCMP** (0x200), 3 TKIP, 7 error/unknown |
| 11 (`0x800`) | decryption done (`RX_MPDU_RES_STATUS_DEC_DONE_MSK`) |
| 6 (`0x40`) | CCMP MIC OK (`RX_MPDU_RES_STATUS_MIC_OK`) |
| 5 (`0x20`) | ICV OK (WEP/TKIP) |
| 7 (`0x80`) | TTAK OK (TKIP) |

A CCMP frame was decrypted and verified iff `(st & 0x700) == 0x200 && (st & 0x800) && (st & 0x40)`.
Linux then sets only `RX_FLAG_DECRYPTED` (`dvm/rx.c:569-596`). Bad MIC → Linux passes it on without the
flag and mac80211 decrypts in software, which fails (`mac80211/wpa.c:565-578`). For a port: **drop it**.
DEC_DONE clear → not decrypted (key missing or wrong key ID). Drop it.

The decrypted frame **still contains the 8-byte CCMP header after the MAC header and the 8-byte MIC at
the end**: DVM never sets `RX_FLAG_IV_STRIPPED` or `RX_FLAG_MIC_STRIPPED`, and mac80211 strips both
itself (`mac80211/wpa.c:533-537, 546, 585-589`). L includes them.

**Replay check (mac80211 does it, since DVM does not set `RX_FLAG_PN_VALIDATED`, `mac80211/wpa.c:550-563`):**
PN = 48-bit `hdr[7] hdr[6] hdr[5] hdr[4] hdr[1] hdr[0]` (most→least significant, `ccmp_hdr2pn`,
`mac80211/wpa.c:415-423`). Key ID = `hdr[3] >> 6`. Drop if PN ≤ the last accepted PN for that key and
security index. mac80211 keeps a counter per TID for QoS data (`rx->security_idx`) **[GK, §12.5.3.4.4: per-TC replay counters]**.
Duplicate detection by sequence number + Retry bit (fc 0x0800, `ieee80211.h:49`) is also mac80211's job. Do it per TID.

### 7.2 802.11 → Ethernet

Frame control (le16): type = bits 3:2 (2 = data, `0x0008`); subtype bits 7:4. QoS if `fc & 0x0080`
(`ieee80211.h:61, 99`). Null-data subtypes (bit 6 set, e.g. 0x0048 / 0x00C8) carry no payload, so drop
them. From the AP: FromDS `0x0200` set, ToDS `0x0100` clear (`ieee80211.h:46-47`).

```
addr1 @4  = DA (our MAC, or group address)
addr2 @10 = BSSID (transmitter)
addr3 @16 = SA
seq   @22
qos   @24 (2 bytes) if QoS: TID = byte24 & 0x0F (ieee80211.h:256); bit 7 = A-MSDU present
hdr_len = 24, or 26 for QoS. (+4 HT Control if QoS and fc & 0x8000 Order: not seen without HT.)
body  = frame[hdr_len ..]
if fc & 0x4000 (Protected): require §7.1 status, replay-check, body = body[8 .. len-8]
LLC/SNAP: body[0..6] == AA AA 03 00 00 00 → ethertype = BE16 body[6..8], payload = body[8..]
Ethernet frame = addr1 (6) | addr3 (6) | ethertype (2) | payload
```

Drop frames whose SA (addr3) is our own MAC: our multicasts reflected by the AP. EtherType 0x888E
goes to the handshake code, which accepts it unprotected before the PTK is installed. A-MSDU (QoS bit 7)
is only sent to peers that advertise it, via HT capabilities, so a legacy association should never see it.
If one appears, drop it. (**[GK]** for the A-MSDU claim.)

---

## 8. Transmitting data

### 8.1 Header

Non-QoS (the minimal client from §3.2), to-DS:

```
00: 08 01 / 08 41      frame_control: FTYPE_DATA 0x0008 | TODS 0x0100 (| PROTECTED 0x4000 once keyed)
02: 00 00              duration (see §3 note: Linux fills it; 0 relies on the uCode, UNVERIFIED)
04: <BSSID>            addr1 = RA = AP
0A: <our MAC>          addr2 = TA = SA
10: <DA>               addr3 = final destination (any unicast/multicast/broadcast address)
16: 00 00              seq_ctrl (firmware writes it: SEQ_CTL flag)
18: [CCMP header 8 B]  only if protected
    AA AA 03 00 00 00 <ethertype BE16> <payload>
```

QoS variant (if you associated with WMM): fc `88 01` / `88 41`, QoS control at 24 = `TID, 0x00`
(normal ack policy, no A-MSDU), `hdr_len = 26` → MH_PAD (§1.3). Then **you** write `seq_ctrl` =
`(per-TID counter) << 4`, fragment 0, and advance the counter by 1 per frame (`dvm/tx.c:402-406, 438-439`).

`tx_cmd.len` = header + (8 if protected) + 8 (LLC/SNAP) + payload. The MIC is not counted
(the device appends it, `mac80211/wpa.c:455-458`). The BC entry adds +8 for it (§1.5).

### 8.2 TX command fields

Queue 2 (BE) for non-QoS; for QoS the AC from §1.1. `sta_id 0`, `tx_flags` per the §1.4 table,
`rate_n_flags 0`, `tid_tspec` = 8 (non-QoS) or the TID, `sec_ctl 0x02` and `key = TK` when protected.

### 8.3 CCMP header (`ccmp_pn2hdr`, `mac80211/wpa.c:402-412, 474-483`)

Keep a 48-bit TX PN per PTK, starting at 0. Pre-increment it for every protected MPDU
(`atomic64_inc_return`, so the first frame uses PN 1). Never reuse a PN: a retransmission at the 802.11
layer is done by the firmware and keeps the PN. Each new frame gets a new one. For `pn = PN`:

```
hdr[0] = pn        & 0xFF      (PN0)
hdr[1] = (pn >> 8) & 0xFF      (PN1)
hdr[2] = 0x00                  (reserved)
hdr[3] = 0x20 | (keyid << 6)   (Ext IV = 1; pairwise key ID 0 → 0x20)
hdr[4] = (pn >> 16) & 0xFF     (PN2)
hdr[5] = (pn >> 24) & 0xFF
hdr[6] = (pn >> 32) & 0xFF
hdr[7] = (pn >> 40) & 0xFF     (PN5)
```

Set PROTECTED (0x4000) in the frame control. The firmware runs AES-CCM with `key[16]` using the PN from
this header and the MAC header as AAD. That last part is what Linux relies on (GENERATE_IV with
hardware encryption), but the firmware-internal behaviour is **UNVERIFIED**. EAPOL frames sent after the
PTK install (rekey) are encrypted the same way.

---

## 9. Likely failure points and how to see them

| Symptom | Where to look | Likely cause |
|---|---|---|
| ADD_STA response status ≠ 0x01 | 0x18 response byte 0 | 0x08: modifying station 0 after an un-associated RXON wiped it (re-add first); 0x02: table full |
| `REPLY_ERROR` (0x02) packet | payload `le32 error_type; u8 cmd_id; u8 rsvd; le16 bad_cmd_seq_num; le32 error_info; le64 timestamp` (`dvm/commands.h:426-433`) | wrong command length/layout for `cmd_id` |
| CSR_INT bit 25 (SW_ERR) | firmware error log at ALIVE's `error_event_table_ptr` (BU §7.5) | firmware assert, often a bad TFD/BC entry or a bad RXON |
| No REPLY_TX at all | TFD contents, BC entry, write-pointer register value | data queue still pointing at the shared idle ring; BC entry missing or zero; kick wrote the wrong queue number |
| REPLY_TX status **0x90** PASSIVE_NO_RX | §1.7 @32 | passive channel, no beacon seen since the RXON (§2.5) |
| **0x83** LONG_LIMIT / **0x82** SHORT_LIMIT | `failure_frame` @3 | no ACK: wrong channel, AP out of range, rate the AP doesn't support, wrong BSSID in addr1 |
| **0x8B** STA_INVALID | `ra_tid` @29 | `sta_id` not in the firmware table (an un-associated RXON wiped it, or the AP was never added) |
| **0x86** RFKILL_FLUSH | | RF-kill. Linux flushes the queues (`dvm/tx.c:1101-1109`) |
| **0x84** FIFO_UNDERRUN | | BC length ≠ actual TB data, or TB2 missing |
| **0x87** LIFE_EXPIRE / **0x88** DEST_PS | | queued too long / AP flagged power save (not expected) |
| 0x40..0x44 POSTPONE_* | | not sent (BT, quiet period…). Treat as failure and retry (`dvm/commands.h:1265-1280`) |
| Auth response never arrives | RX 0xC1 with fc 0x00B0 | RXON BSSID/node_addr wrong; auth sent before the beacon on a passive channel |
| Auth status 13 | body @28 | AP wants shared-key or SAE (WPA3-only AP) |
| Assoc status 12/18/40-45 | §3.3 | rates intersection empty (18), RSN IE mismatch or unsupported cipher/AKM (40-43), AP requires HT/WMM (12, **UNVERIFIED**) |
| Deauth reason **15** (4-way timeout) | fc 0x00C0, reason @24 | message-2 MIC wrong: PSK salt/passphrase, PRF label (`"Pairwise key expansion"`, no NUL inside A, `0x00` separator), Min/Max ordering, or replay counter not copied |
| Deauth reason **17** IE_DIFFERENT | | message-2 RSN IE not byte-identical to the assoc-request RSN IE |
| Deauth reason **2 / 6 / 7** | (`ieee80211.h:1823-1847`) | AP lost our state: data sent before association, or a frame after a deauth |
| Deauth reason 14 / 23 / 24 | | MIC failure / 802.1X failure / cipher rejected |
| Handshake completes but no data passes RX | `rx_pkt_status` | SEC_TYPE CCMP without DEC_DONE → key not installed on sta 0 or wrong `key_offset`/key ID; DEC_DONE without MIC_OK → wrong TK |
| AP ignores our encrypted frames | (AP side counters) | PN reused/decreasing, PROTECTED bit missing, `tx_cmd.len` not counting the CCMP header, key ID ≠ 0 for the pairwise key |
| Link drops after some time | `MISSED_BEACONS_NOTIFICATION` 0xA2, payload `le32 consecutive, total, expected, received` (`dvm/commands.h:98, 2750-2755`) | AP gone, wrong timing command, or GTK rekey not handled (group msg 1 unanswered → deauth 16) |

---

## 10. Condensed order of operations (after BU §11 step 12)

1. Wait for SCAN_COMPLETE. Pick the BSS. Keep TSF, beacon interval, capability, rates, RSN IE,
   DTIM, ERP, WMM. Precompute PSK = PBKDF2(passphrase, SSID).
2. RXON unassoc: channel, BSSID, MAC, ESS, filter 0x44, flags 0x8005, basic rates (§2.2-2.3) →
   ADD_STA(15) + LQ(15) → ADD_STA(0, BSSID, mode 0) → 0x95 TX power (§2.4, §2.1 item 5).
3. Wait for a beacon from the BSSID (§2.5).
4. Auth (seq 1) on queue 0 via station 15, tx_flags 0x3008, 1 Mbps. Wait for auth seq 2, status 0 (§3.1).
5. Assoc request (capab 0x0431, listen 10, SSID, rates, RSN IE, optionally no HT/WMM) on queue 0,
   pm_frame_timeout 3. Wait for assoc response status 0. Read the AID (§3.2-3.3).
6. RXON with ASSOC clear + AID + flags (filter 0x04) → ADD_STA(15) + LQ(15) → ADD_STA(0, station flags) +
   LQ(0) → RXON_TIMING → QOS_PARAM → RXON with ASSOC (filter 0x24) → (SENSITIVITY) → 0x95 (§4).
7. EAPOL message 1 → derive the PTK → message 2 (data, sta 0, STA_RATE, IGNORE_BT, unencrypted) →
   message 3 → verify MIC, unwrap GTK → message 4 (unencrypted), wait for TX success (§5).
8. ADD_STA key modify: PTK on sta 0 offset 0 (key_flags 0x000A), GTK on sta 0 offset 1 (0x410A for key
   ID 1) (§6).
9. Data: TX with PROTECTED + CCMP header + `sec_ctl 2` + TK (§8). RX: check decrypt bits, replay-check,
   strip header + CCMP header + MIC + LLC/SNAP → Ethernet (§7).

---

## 11. UNVERIFIED items, collected

1. Whether a TX power command (0x95) is required after the un-associated RXON before the first auth (§2.1 item 5). Linux doesn't send one; the firmware header says it must be sent.
2. Single-TB REPLY_TX instead of Linux's 20 + rest + body split (§1.3).
3. Whether association/data work without SENSITIVITY_CMD and chain-noise calibration (§4.1 steps 2.6, 3).
4. A fixed descending legacy LQ table for station 0 (§4.3). Linux starts with all-1 Mbps and rate-scales.
5. Whether a given AP accepts a non-HT, non-WMM client (§3.2), and whether any 2.4 GHz AP requires the spectrum-management elements.
6. Firmware internals of CCMP TX: that it uses the host-written PN and appends the MIC (§8.3). This is the only way Linux's GENERATE_IV path can work, but it is not stated in the source.
7. AP retransmit timing for EAPOL message 1 (§5.1).
8. Whether the uCode overwrites a zero `duration` field in our headers (§3 note). Linux always sends mac80211's computed value.
9. Everything marked **[GK]** in §5, §3.3 (ERP bits), §4.5 (WMM element) and §7 (A-MSDU, per-TID replay counters): from the 802.11-2016 standard and wpa_supplicant/hostapd behaviour, not from the kernel tree. EAPOL Key Length 0 in messages 2/4 and the 802.1X version byte are implementation conventions; APs are not known to check them (**UNVERIFIED**).
