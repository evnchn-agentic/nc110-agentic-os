#!/usr/bin/env bash
# Install a kernel as an agent-OS version on the NC110 (run while Slot A Linux is up). Adds ONE new entry
# "nc110-bm-agentos-<sha8>" (never edits an existing one), copies /baremetal/k-agentos-<sha8>.elf and the agent-OS
# key module (~/.nc110/agentos.key -> /baremetal/nc110-agentos.key). Does NOT change the default: that is
#   tools/nc110-install-agentos.sh --set-default <sha8>     (backs up grubenv first)
# Entry behaviour (template t4): a per-RTC-day boot budget in grubenv (the 31st agent-OS boot of a day, or a malformed
# counter, boots Slot A Linux instead), then GRUB arms the TCO (nc110_tco_consts, lock_mode=accept), then multiboot +
# module + boot. Every GRUB-side failure (grubenv not writable, arming refused, kernel/module missing) calls
# nc110_linux_now2 (grub-test/nc110-linux-now2.cfg): stock Slot A, with a 10-min agent lease,
# Linux in the SAME boot, leaving the breadcrumb nc110_fallback=<sha8|loop-date> in grubenv when it can (read from Linux). Manual fallback any time: hold Shift through POST -> GRUB menu -> "Alpine NC110 ...".
set -u
cd "$(dirname "$0")/.."
S() { ssh -F /dev/null -o BatchMode=yes -o ConnectTimeout=5 -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null \
  -o LogLevel=ERROR -i "$HOME/.ssh/id_ed25519_homelab" root@192.168.50.118 "$@"; }
SC() { scp -q -F /dev/null -o BatchMode=yes -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -o LogLevel=ERROR \
  -i "$HOME/.ssh/id_ed25519_homelab" "$@"; }
gen_entry() {  # $1 sha8 -> the exact menuentry text (also printed by --print-entry for the q35 rehearsal)
  local H=$1 E="nc110-bm-agentos-t11-$1" KN="k-agentos-$1.elf"   # t11: boot budget 30/day (demos reboot to show the boot time); t10: t9 plus the optional config module (hosts, WireGuard); t9: t7 for kernels whose header asks for video (gfxpayload=text keeps VGA text: this VBIOS has no 1024x600, t8 got 800x600); t7: TSC stamps; t6: quiet; t5: recovery Linux
  ENTRY="menuentry \"$E\" {
  set color_normal=black/black
  insmod rdmsr
  rdmsr 0x10 -v nc110_tsc0
  insmod ext2
  search --no-floppy --fs-uuid --set=root c0954bc7-86a0-440c-8401-2c3e95549743
  insmod datehook
  insmod regexp
  set nc110_now=\"\${YEAR}-\${MONTH}-\${DAY}\"
  if [ \"\$nc110_hour\" != \"\$nc110_now\" ]; then
    set nc110_hour=\"\$nc110_now\"
    set nc110_boots=\"\"
  fi
  set nc110_boots=\"\${nc110_boots}x\"
  if regexp '^x{1,30}\$' \"\$nc110_boots\"; then
    true
  else
    set color_normal=light-gray/black
    echo \"nc110-agentos $H: 31st agent-OS boot today (or a malformed counter), escaping to Slot A Linux\"
    set nc110_boots=\"\"
    set nc110_fallback=\"loop-\$nc110_now\"
    save_env nc110_hour nc110_boots nc110_fallback
    nc110_linux_now2
    set color_normal=light-gray/black
    echo \"nc110-agentos $H: Slot A Linux failed to load, trying the agent OS anyway\"
  fi
  if save_env nc110_hour nc110_boots; then
    true
  else
    set color_normal=light-gray/black
    echo \"nc110-agentos $H: grubenv not writable, Slot A Linux now\"
    nc110_linux_now2
  fi
  terminal_output console
  set gfxpayload=text
  insmod multiboot
  nc110_tco_consts
  tco_arm
  if [ \"\$wdt_ok\" = \"yes\" ]; then
    rdmsr 0x10 -v nc110_tsc1
    if multiboot /baremetal/$KN bmctl grubtsc=\${nc110_tsc0},\${nc110_tsc1}; then
      if module /baremetal/nc110-agentos.key; then
        if [ -f /baremetal/nc110-agentos.cfg ]; then
          module /baremetal/nc110-agentos.cfg
        fi
        boot
      fi
    fi
    set color_normal=light-gray/black
    echo \"nc110-agentos $H: kernel or key module failed to load\"
  else
    set color_normal=light-gray/black
    echo \"nc110-agentos $H: GRUB did NOT arm the TCO\"
  fi
  set nc110_fallback=$H
  save_env nc110_fallback
  nc110_linux_now2
}"
}
if [ "${1:-}" = "--print-entry" ]; then
  gen_entry "$(shasum -a 256 "$2" | cut -c1-8)"; printf '%s\n' "$ENTRY"; exit 0
fi
# Slot A = root on /dev/sda2; under the agent-OS route's custom kernel (no initrd) /proc/mounts says /dev/root
st=$(S 'hostname; r=$(awk "\$2==\"/\"{print \$1}" /proc/mounts); [ "$r" = /dev/root ] && grep -q "root=/dev/sda2" /proc/cmdline && r=/dev/sda2; echo $r') \
  || { echo "Slot A not reachable"; exit 2; }
grep -qx nc110 <<<"$st" && grep -qx /dev/sda2 <<<"$st" || { echo "not on Slot A: STOP"; echo "$st"; exit 2; }
T=$(date -u +%Y%m%dT%H%M%SZ)
if [ "${1:-}" = "--lease-service" ]; then  # (re)install Slot A's lease service, verified, local service enabled
  SC tools/slot-a/nc110-lease.start root@192.168.50.118:/etc/local.d/nc110-lease.start || exit 2
  v=$(S "rc-update add local default >/dev/null 2>&1; chmod 755 /etc/local.d/nc110-lease.start && sha256sum /etc/local.d/nc110-lease.start | cut -c1-64; rc-update show default | grep -cw local")
  [ "$(head -1 <<<"$v")" = "$(shasum -a 256 tools/slot-a/nc110-lease.start | cut -c1-64)" ] && [ "$(tail -1 <<<"$v")" = 1 ] \
    || { echo "lease service install NOT verified: $v"; exit 2; }
  echo "lease service installed and verified (local service enabled)"; exit 0
fi
if [ "${1:-}" = "--set-default" ]; then
  E=$(S "grep -o '^menuentry \"nc110-bm-agentos-t[0-9][0-9]*-$2\"' /boot/grub/grub.cfg | tail -1 | cut -d'\"' -f2")
  [ -n "$E" ] || { echo "no agent-OS entry for $2"; exit 2; }
  S "cp -p /boot/grub/grubenv /boot/grub/grubenv.bak-agentos-$T && grub-editenv /boot/grub/grubenv set saved_entry='$E' && sync && grub-editenv /boot/grub/grubenv list"
  exit $?
fi
K=${1:?kernel path}; KEY=$HOME/.nc110/agentos.key
[ "$(wc -c <"$KEY" | tr -d " ")" = 72 ] || { echo "agent-OS key missing/bad"; exit 2; }
H=$(shasum -a 256 "$K" | cut -c1-8); E="nc110-bm-agentos-t11-$H"; KN=k-agentos-$H.elf
S "grep -q '^menuentry \"$E\"' /boot/grub/grub.cfg" && { echo "$E already installed"; exit 0; }
S "grep -q '^function nc110_tco_consts' /boot/grub/grub.cfg" || { echo "GRUB arming function missing"; exit 2; }
S "grep -q '^function nc110_linux_now2' /boot/grub/grub.cfg" || { echo "nc110_linux_now2 missing: append grub-test/nc110-linux-now2.cfg first"; exit 2; }
gen_entry "$H"
BK=/boot/grub/grub.cfg.bak-nc110bm-$T
S "cp -p /boot/grub/grub.cfg $BK" || exit 2
S "[ -e /boot/baremetal/$KN ]" || SC "$K" "root@192.168.50.118:/boot/baremetal/$KN" || exit 2
SC "$KEY" root@192.168.50.118:/boot/baremetal/nc110-agentos.key && S "chmod 600 /boot/baremetal/nc110-agentos.key" || exit 2
CFG=$HOME/.nc110/agentos.cfg  # optional: hosts + WireGuard (holds a private key; never in the repo)
if [ -f "$CFG" ]; then
  [ "$(head -c 10 "$CFG")" = "NC110CFG1" ] && [ "$(wc -c <"$CFG" | tr -d " ")" -le 4096 ] || { echo "agent-OS config bad (header/size)"; exit 2; }
  SC "$CFG" root@192.168.50.118:/boot/baremetal/nc110-agentos.cfg && S "chmod 600 /boot/baremetal/nc110-agentos.cfg" || exit 2
fi
printf '%s\n' "$ENTRY" | S "cat >> /boot/grub/grub.cfg" || { S "cp -p $BK /boot/grub/grub.cfg"; exit 2; }
ESHA=$(printf '%s\n' "$ENTRY" | shasum -a 256 | cut -c1-64); ELEN=$(printf '%s\n' "$ENTRY" | wc -c | tr -d ' ')
v=$(S "sync; sha256sum /boot/baremetal/$KN | cut -c1-64; sha256sum /boot/baremetal/nc110-agentos.key | cut -c1-64; head -c \$(stat -c %s $BK) /boot/grub/grub.cfg | sha256sum | cut -c1-64; sha256sum $BK | cut -c1-64; tail -c $ELEN /boot/grub/grub.cfg | sha256sum | cut -c1-64; grub-script-check /boot/grub/grub.cfg && echo check-ok")
mapfile -t vv <<<"$v"
[ "${vv[0]}" = "$(shasum -a 256 "$K" | cut -c1-64)" ] && [ "${vv[1]}" = "$(shasum -a 256 "$KEY" | cut -c1-64)" ] && [ "${vv[2]}" = "${vv[3]}" ] \
  && [ "${vv[4]}" = "$ESHA" ] && [ "${vv[5]:-}" = check-ok ] || { echo "verify FAILED, restoring grub.cfg"; S "cp -p $BK /boot/grub/grub.cfg; sync"; exit 2; }
echo "installed $E (default unchanged). Test once: next_entry=$E; make default: $0 --set-default $H"
