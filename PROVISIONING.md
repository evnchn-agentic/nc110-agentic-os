# NC110 agent OS: provisioning without Linux (agent runbook)

**Default boot:** GRUB entry `nc110-bm-agentos-t5-<sha8>` (now t5-49b98b1e), installed by
`tools/nc110-install-agentos.sh` (template t5). GRUB arms the TCO watchdog first, then boots our kernel with `bmctl` and the
dedicated key module (`~/.nc110/agentos.key` on the Mac, not in the repo). `bmctl.py` uses that key by default.

## Day to day (no Linux)
- `tools/.venv/bin/python tools/bmctl.py ping | stats | logall | peek ... | rng N file | reboot`
- `bmctl.py load <kernel.elf> bmctl [args]`: new build into RAM over the network, verified, chain-loaded. /boot is untouched.
- Protocol: kernels from 2026-10-21 (commit after 3855ed3) speak bmctl v2 only (stateless `who2` cookie, see src/bmctl.rs);
  older kernels (the installed f4b8b3db and earlier) speak v1. `bmctl.py` speaks both (sends `who2` and `who`, takes
  the first verified reply). A pre-v2 `bmctl.py` against a v2 kernel exits with "bad who reply ... v2-only".
  Flood rig: `tools/flood-q35-test.py <kernel> <pps> single|spoof` (raw Ethernet via QEMU dgram, spoofed sources).

## Getting to Slot A Linux (persistent changes: a new agent-OS version, grub.cfg, keys)
- Remote, agent OS healthy: `tools/nc110-to-linux.sh` chain-loads the custom i386 kernel (~/nc110-kbuild) with root=/dev/sda2
  and `nc110.lease=600`.
- Remote, agent OS unhealthy or unreachable: the crash-loop escape. GRUB counts agent-OS boots per RTC day, and the 6th goes to
  recovery Linux (stock kernel, `nc110.lease=600`, grubenv `nc110_fallback=loop-<date>`). Five `bmctl reboot`s also get there
  on purpose.
- Any GRUB-side failure (grubenv not writable, arming refused, kernel or key missing) boots recovery Linux in the same boot.
- **Lease:** in an agent-route Linux, Slot A's `/etc/local.d/nc110-lease.start` feeds the watchdog only while an agent keeps
  doing `touch /dev/shm/nc110-lease` (at least every 10 min). Otherwise: sync, / read-only, TCO reset, back into the agent
  OS. `reboot` returns to the agent OS at once. A manual Shift-menu boot has no lease.
- By hand: hold Shift through POST, then pick "Alpine NC110, with Linux lts" in the GRUB menu.

## New agent-OS version (from Linux)
`tools/nc110-install-agentos.sh <kernel>` (new entry, never edits one), test it once with next_entry, then
`--set-default <sha8>` (grubenv backed up). Rollback = `--set-default <old sha8>`.

## Safety
- No disk driver in our kernel; /boot is only written from Linux.
- Any hang leads to a TCO reset, then the agent OS. 10 min without an authenticated command means an idle reset.
- Persistent failure: crash-loop escape to Linux. Soak: `tools/nc110-soak.sh` writes `proof/nc110-soak.log`.
- Known limit: a network-loaded kernel that keeps petting the TCO without serving bmctl cannot be stopped from outside.
  Only kernels from this tree, which always run bmctl with the idle reset, should be loaded. True outside enforcement needs
  an external power switch (and the battery out).

## Tests
`tools/bmctl-q35-test.sh`, `tools/loader-q35-test.sh`, `python3 tools/entropy-est.py --selftest`. Hardware proofs:
`proof/nc110-nolinux-demo.log`, `proof/nc110-agentos-default.md`.

## Parked (evnchn 2026-10-19 10:15, until the .179 quota resets Fri 12:00)

Board powered off by evnchn 10:2x; soak pinger stopped, container removed, hwlock released. Was: `bmctl ssh` (faa3129) chain-loaded in RAM, long-lived SSH terminal to the placeholder container
(.95:2223, scratch config) with bmctl live; a power cycle returns to the installed agent OS (t5-49b98b1e).
Next, in order:
1. On-screen chooser in the agent OS: pick what to connect to (placeholder container, other hosts) instead of a
   compiled-in SERVER; SSH becomes a mode of the agent OS itself (default entry), not a separate chain-load.
   Targets + host pins come over bmctl, never compiled in.
2. Brightness control on the NC110 panel (likely the GMA3150 backlight PWM / BLC_PWM_CTL via the IGD MMIO BAR,
   or the EC/ACPI path; probe with read-only `peek` first).
3. Polish the looks (evnchn 2026-10-19: "Friday 12:00 I wish that the agentic OS is more polished on its looks"):
   status bar, clean boot screen instead of raw log, the chooser from item 1 drawn properly. q35 screenshots first.
4. Codex SHOULD from faa3129 review: recovery tests for idle after early SSH failure, flood, injected inner-loop hang.

## Services on .179 for the agent OS (2026-10-20)
- Voice (F12 push-to-talk): LaunchAgent `~/Library/LaunchAgents/com.evnchn.nc110-voice.plist` runs `tools/nc110-voice.py`
  under `~/nds-claude/.venv` (KeepAlive, log `~/.nc110/voice.log`). Revert:
  `launchctl bootout gui/$(id -u)/com.evnchn.nc110-voice; rm ~/Library/LaunchAgents/com.evnchn.nc110-voice.plist`
- SSH terminal target (S key): docker `nc110-sshd` (restart unless-stopped), config and keys in `~/.nc110/sshd/`. Revert: `docker rm -f nc110-sshd`.
- Soak pinger: `nohup bash tools/nc110-soak.sh` (reboot-mortal by design; it also keeps bmctl's 10 min idle bound from firing).

## Agent OS as of 2026-10-20 22:30 (default t9-15caa73f)
- Screen: native 1024x600 (GMA plane B; `nogma` = VBIOS text mode; falls back by itself on a failed check), sizes
  85x25 / 128x37 / 170x50 with Z. Home keys: S SSH terminal, F12 voice, L log, Z size, R restart; touchpad works
  (pointer, chip clicks, xterm mouse reports in SSH).
- bmctl extras: `gma` (display self-check), `mouse` (raw pad bytes), `key <text>` (inject keys), `rec`/`aud`/`type`/`say`.
- Untethered: no cable -> the offline home screen (WiFi from W); a cable plugged in later needs a reboot. Recovery
  Linux also has WiFi (a dedicated 2.4 GHz WPA2 network, 192.168.50.91). Rollback chain: 052086f3 (text) -> 0aab7558 -> be695204 -> 49b98b1e.
- SSH host chooser (S key): hosts in `~/.nc110/hosts.json`; LaunchAgent `com.evnchn.nc110-hosts` (`tools/nc110-hosts.py --watch`,
  log `~/.nc110/hosts.log`) pushes them over bmctl after every boot, pins via ssh-keyscan. Revert:
  `launchctl bootout gui/$(id -u)/com.evnchn.nc110-hosts; rm ~/Library/LaunchAgents/com.evnchn.nc110-hosts.plist`.
  Default agent OS t9-4196a7aa (rollback 394d6891).

## S3 sleep attempt 2026-10-21 (STOPPED)
- Linux (recovery, stock kernel) on this board: S3 + RTC wake OK (60 s), S3 + WoL magic packet OK (23 s); EXP3 = NIC root
  port, _PRW GPE 9. Tools: tools/nc110-wake.py (magic packet).
- Agent OS `bmctl sleep` (sleep.rs: _PTS by hand, FACS vector -> 0xCF9 reset stub, RTC + WoL armed): entered something
  at 01:40 and never came back (6 magic packets, RTC alarm). Needed a physical power-button press. Not in the installed
  default (t9-4196a7aa); do not run `bmctl sleep` again without evnchn's go and hands within reach.
- Next idea (untested): soft-off S5 with RTC/WoL wake, so the firmware powers on through its normal cold boot and no
  waking vector or stub is involved; prove S5 wake under Linux first.
