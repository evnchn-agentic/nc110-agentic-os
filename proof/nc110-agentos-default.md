# NC110 agent OS as the default boot (2026-10-18 22:43-23:00, evnchn's go 22:43, ask A179-1018-13)

- Entry `nc110-bm-agentos-t2-d4275899` (tools/nc110-install-agentos.sh): GRUB arms the TCO, then multiboot
  k-agentos-d4275899.elf with cmdline `bmctl` + module nc110-agentos.key (a dedicated key, ~/.nc110/agentos.key on the Mac,
  not in the repo; the test key is rejected). Any GRUB-side failure sets next_entry=0, saves nc110_fallback=<sha8> and reboots.
- grubenv: saved_entry=nc110-bm-agentos-t2-d4275899 (backup /boot/grub/grubenv.bak-agentos-*). Linux = manual fallback:
  hold Shift through POST, GRUB menu (30 s), "Alpine NC110, with Linux lts". Nothing on Slot A rewrites saved_entry
  (GRUB_SAVEDEFAULT=false; slotlab only reads grubenv).

| Test on the real netbook | Result |
|---|---|
| q35 GRUB 2.14 rehearsal of the exact entry bytes | boots, answers with the agent-OS key, test key rejected; missing kernel -> fallback branch |
| one-shot agent-OS boot from Linux | bmctl up, inherited TCO TMR=100 (GRUB-armed), key module verified |
| broken entry (kernel missing), default already agent OS | Linux for one boot, grubenv `nc110_fallback=fallbck0` (fallback proven) |
| plain reboot from Linux | **agent OS by default** in 20 s |
| network-loaded kernel that hangs (wdt=probe) | watchdog reset -> back in the **agent OS** in 138 s |
| bmctl `reboot` | back in the agent OS in 15 s |
| soak | tools/nc110-soak.sh -> proof/nc110-soak.log (started 23:00) |

## Round 2 (23:03-23:25): Codex 3/10 -> fixes, all tested on the real netbook
- **Remote route to Linux (finding 3):** the loader now boots an i386 Linux bzImage (32-bit boot protocol). The custom kernel
  6.18.44-agentnet is cross-built on the Mac. Agent OS -> `tools/nc110-to-linux.sh` -> **Slot A Linux with SSH, no hands**
  (9.3 MB staged in 127 s, SSH 10 s after the jump). q35 proof: `proof/linux-from-agentos-q35-serial.log`.
- **Crash-loop escape (finding 1) and same-boot fallback (finding 4):** template t3. The 4th agent-OS boot in one RTC hour
  goes to stock Linux. Proven: agent-OS boots 1-3, then **the 4th came up as 6.18.44-0-lts** with
  `nc110_fallback=loop-2026-10-18-23`. Every GRUB failure path ends in `nc110_linux_now`, with no grubenv write needed.
- Default now `nc110-bm-agentos-t3-aa8e0f26`, the kernel with the Linux route. Reboot from Linux goes back to the agent OS (verified).
- Open (documented): finding 2, a loaded kernel petting forever without bmctl, needs outside enforcement (a power switch);
  finding 6, a hang before GRUB arms.

## Round 3 (23:27-23:47): Codex 6/10 -> fixes, tested on the real netbook
- **Linux-visit lease (new MUST-FIX):** the pinger feeds the TCO only while `/dev/shm/nc110-lease` is younger than 10 min. Test: lease
  left unrenewed -> Linux answered SSH until t=630 s -> watchdog reset -> **t4 agent OS answered at t=651 s, no hands**.
- **Template t4:** a per-RTC-day boot budget (the hourly one missed slow loops) and a fail-closed counter (`^x{1,5}$`, else escape).
  Test: agent-OS boots 1-5, then **the 6th came up as stock 6.18.44-0-lts**, `nc110_fallback=loop-2026-10-18`. Reboot -> agent OS.
- **Loader:** decompression interval for relocatable/fixed kernels, header length, initrd/setup_data cleared, cmdline_size,
  strict E820 records. q35 Linux route and both suites re-run: PASS.
- Default: `nc110-bm-agentos-t4-f19a9a17`.
- **Still open (needs evnchn):** Codex finding 2. A loaded kernel that feeds the TCO but never serves bmctl can only be stopped from
  outside (a smart plug, and the battery out). Also open: a hang before GRUB arms the watchdog, and stock recovery Linux has no
  finite watchdog-open limit (a deliberate choice: it is the stable parking place).

## Round 4 (23:49-00:08): Codex 7/10 (8/10 excl. finding 2) -> recovery Linux gets a deadline, tested on hardware
- Template t5 (`nc110-bm-agentos-t5-49b98b1e`, now default): every Linux fallback goes through `nc110_linux_now2`. GRUB arms the
  TCO, and the cmdline carries `nc110.lease=600 watchdog.open_timeout=300`.
- Slot A `/etc/local.d/nc110-lease.start` (tools/slot-a/): active only for `nc110.lease` boots, so manual Shift boots are
  untouched. It holds /dev/watchdog and feeds it while /dev/shm/nc110-lease keeps changing (aged on /proc/uptime). On expiry:
  sync + clean reboot.
- **Test:** day budget escaped to recovery Linux (stock 6.18.44-0-lts, cmdline with nc110.lease) -> no renewals -> recovery
  Linux answered SSH until t=584 s -> **agent OS answered at t=629 s** (the lease-triggered reboot, not the 60 s TCO path).
- Loader SHOULD-FIX items also done (relocatable interval, E820 header length, header bound); q35 Linux route + suites PASS.

## Round 5 (00:09-00:30): Codex MUST-FIX "iTCO stops on reboot" -> expiry is terminal via the TCO, tested on hardware
- `tools/slot-a/nc110-lease.start` v2: on expiry it runs sync, remounts / read-only and **never feeds again**, so the TCO resets
  the board and no userspace shutdown is involved. stat/open/write failures exit, which leaves an unclean close, so the TCO also
  resets. Installed with `tools/nc110-install-agentos.sh --lease-service` (verified hash, 755, OpenRC local enabled).
- **Fault injection:** SIGKILL of the lease loop -> Linux last answered t=59 s -> **agent OS at t=79 s**.
- **Terminal expiry:** unrenewed lease -> Linux last answered t=643 s -> **agent OS at t=663 s**.
- `to-linux` checks real /dev/watchdog ownership; the wall-clock fallback pinger is gone. Loader: checked alignment arithmetic.

## Where it stands
Every path below was tested on the real netbook:
- plain boot -> agent OS
- network kernel hang -> TCO -> agent OS
- network reboot -> agent OS
- broken entry -> Linux
- 6th boot of a day -> recovery Linux -> unrenewed lease -> agent OS
- agent OS -> Linux over the network -> unrenewed or killed lease -> agent OS

Needs hardware or evnchn (not software-fixable): a network-loaded kernel that feeds the TCO but never serves bmctl (Codex
finding 2), and a BIOS/pre-GRUB hang. Both need an external power switch with the battery out. Remaining software items
(Codex): the date-based boot budget resets at midnight, GRUB writes happen before arming, and channel health counts inbound
authenticated traffic without proof that replies reach the client.

## Soak (clean, from 2026-10-19 00:30)
- 2026-10-19 07:03 ~6.5 h: one boot (7eb89d5f) the whole time, no unplanned reboot or lost contact, 0 bad/replay/retx. Live log: proof/nc110-soak.log
