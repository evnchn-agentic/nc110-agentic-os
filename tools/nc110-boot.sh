#!/usr/bin/env bash
# One attended/overnight iteration on the real NC110 (evnchn's go: 2026-10-17 21:08 "continue over the night").
# Usage: [MODULE=test-keys/client.key] tools/nc110-boot.sh <tag> [kernel cmdline]   (kernel built beforehand)
# MODULE: a THROWAWAY test client key only (never a homelab key); copied to /baremetal/nc110-test-client.key.
# GRUBARM=1: GRUB arms the TCO first (nc110_tco_consts + tco_arm, see grub-test/nc110-grubarm-func.cfg, which must
#   already be in grub.cfg); a refused arm reboots after 10 s. GRUBSLEEP=1 (with GRUBARM): no kernel, GRUB sleeps
#   600 s unpetted then reboots: back in ~150 s = the GRUB-armed watchdog reset it, ~620 s = it did not.
# Formula rules: backup grub.cfg, NEW file /baremetal/k-attended-<sha8>.elf, NEW menuentry appended, original
# prefix verified byte-identical, live grub-script-check, one-shot via next_entry only, saved_entry stays 0.
# Then: reboot from Slot A, wait (<= 6 min) for a NEW boot_id, print what arrived on UDP 5514 since the reboot.
# Exit 3 = not back: STOP, do not retry; the operator must power-cycle.
set -u
TAG=$1; CL=${2:-}
[[ ${WAIT_N:-66} =~ ^[1-9][0-9]{0,2}$ ]] && [ "${WAIT_N:-66}" -ge 12 ] || { echo "WAIT_N must be an integer 12..999"; exit 2; }
TEST_KEY_SHA=2d330230f2f6432d9fa93733edce80448d6793631ebd6348046a285d71862b85   # test-keys/client.key, the throwaway key; nothing else may be copied
[[ $TAG =~ ^[a-z0-9-]{1,40}$ ]] || { echo "tag must be [a-z0-9-]"; exit 2; }
[[ $CL =~ ^[a-z0-9=:.\ -]{0,80}$ ]] || { echo "cmdline must be [a-z0-9=:. -]"; exit 2; }
K=target/attended/x86_64-unknown-none/release/nc110-kernel
LOG=$HOME/party/nc110-m1/udplog.txt
H=$(shasum -a 256 "$K" | cut -c1-8); KN=k-attended-$H.elf; E="nc110-bm-$TAG"
clear_next() {  # unset next_entry and PROVE it is gone; exit 5 if that cannot be shown
  S "grub-editenv /boot/grub/grubenv unset next_entry; sync; grub-editenv /boot/grub/grubenv list" | grep -q 'next_entry=.' \
    && { echo "next_entry STILL SET after unset: the experiment stays scheduled, needs a human"; exit 5; }
  S "grub-editenv /boot/grub/grubenv list" >/dev/null || { echo "cannot read grubenv back: unresolved"; exit 5; }
}
S() { ssh -F /dev/null -o BatchMode=yes -o ConnectTimeout=5 -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null \
  -o LogLevel=ERROR -i "$HOME/.ssh/id_ed25519_homelab" root@192.168.50.118 "$@"; }
pgrep -f udplisten.py >/dev/null || { echo "UDP listener not running: STOP"; exit 2; }
st=$(S 'hostname; awk "\$2==\"/\"{print \$1}" /proc/mounts; grub-editenv /boot/grub/grubenv list; cat /proc/sys/kernel/random/boot_id') \
  || { echo "netbook unreachable: STOP"; exit 2; }
grep -qx nc110 <<<"$st" && grep -qx /dev/sda2 <<<"$st" && grep -qx saved_entry=0 <<<"$st" && ! grep -q 'next_entry=.' <<<"$st" \
  || { echo "not in Slot A default state: STOP"; echo "$st"; exit 2; }
B0=$(tail -1 <<<"$st")
S "grep -q '^menuentry \"$E\"' /boot/grub/grub.cfg" && { echo "entry $E already exists: pick a new tag"; exit 2; }
GRUBARM=${GRUBARM-1}   # default ON since 2026-10-18 01:18 (k18); GRUBARM= turns it off
LOAD="  echo \"nc110-bm: loading /baremetal/$KN $CL\"
  multiboot /baremetal/$KN $CL${MODULE:+
  module /baremetal/nc110-test-client.key}
  boot"
[ -n "${GRUBSLEEP:-}" ] && LOAD='  echo "nc110-bm: GRUBSLEEP: sleeping 600 s unpetted, then reboot"
  sleep 600
  reboot'
[ -n "${GRUBSLEEP:-}" ] && [ -z "${GRUBARM:-}" ] && { echo "GRUBSLEEP needs GRUBARM"; exit 2; }
if [ -n "${GRUBARM:-}" ]; then  # the kernel (or the sleep) is reachable ONLY inside the armed branch
  BODY='  nc110_tco_consts
  tco_arm
  if [ "$wdt_ok" = "yes" ]; then
    echo "nc110-bm: GRUB armed the TCO (~120 s to reset if nobody pets)"
'"$LOAD"'
  else
    echo "nc110-bm: GRUB did NOT arm the TCO, rebooting"
    sleep 10
    reboot
  fi'
else
  BODY=$LOAD
fi
ENTRY="menuentry \"$E\" {
  insmod ext2
  search --no-floppy --fs-uuid --set=root c0954bc7-86a0-440c-8401-2c3e95549743
  terminal_output console
  set gfxpayload=text
  insmod multiboot
$BODY
}"
T=$(date -u +%Y%m%dT%H%M%SZ); BK=/boot/grub/grub.cfg.bak-nc110bm-$T
S "cp -p /boot/grub/grub.cfg $BK" || exit 2
S "[ -e /boot/baremetal/$KN ]" || scp -q -F /dev/null -o BatchMode=yes -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null \
  -o LogLevel=ERROR -i "$HOME/.ssh/id_ed25519_homelab" "$K" "root@192.168.50.118:/boot/baremetal/$KN" || exit 2
if [ -n "${MODULE:-}" ]; then
  [ "$(shasum -a 256 "$MODULE" | cut -c1-64)" = "$TEST_KEY_SHA" ] || { echo "MODULE is not the throwaway test key"; exit 2; }
  scp -q -F /dev/null -o BatchMode=yes -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -o LogLevel=ERROR \
    -i "$HOME/.ssh/id_ed25519_homelab" "$MODULE" root@192.168.50.118:/boot/baremetal/nc110-test-client.key || exit 2
fi
printf '%s\n' "$ENTRY" | S "cat >> /boot/grub/grub.cfg" || { echo "append failed, restoring"; S "cp -p $BK /boot/grub/grub.cfg; sync"; exit 2; }
ESHA=$(printf '%s\n' "$ENTRY" | shasum -a 256 | cut -c1-64); ELEN=$(printf '%s\n' "$ENTRY" | wc -c | tr -d ' ')
v=$(S "sync; sha256sum /boot/baremetal/$KN | cut -c1-64; head -c \$(stat -c %s $BK) /boot/grub/grub.cfg | sha256sum | cut -c1-64; sha256sum $BK | cut -c1-64; tail -c $ELEN /boot/grub/grub.cfg | sha256sum | cut -c1-64; echo \$(( \$(stat -c %s /boot/grub/grub.cfg) - \$(stat -c %s $BK) )); grub-script-check /boot/grub/grub.cfg && echo check-ok")
mapfile -t vv <<<"$v"
[ "${vv[0]}" = "$(shasum -a 256 "$K" | cut -c1-64)" ] && [ "${vv[1]}" = "${vv[2]}" ] && [ "${vv[3]}" = "$ESHA" ] \
  && [ "${vv[4]}" = "$ELEN" ] && [ "${vv[5]:-}" = check-ok ] || {
  echo "verify FAILED, restoring backup"; S "cp -p $BK /boot/grub/grub.cfg; sync"; exit 2; }
echo "installed $E -> $KN '$CL'"
mark=$(wc -c <"$LOG")
S "grub-editenv /boot/grub/grubenv set next_entry='$E' && grub-editenv /boot/grub/grubenv list | grep -qx 'next_entry=$E' && sync" \
  || { echo "could not set next_entry: STOP"; clear_next; exit 2; }
S "(sleep 1; reboot) >/dev/null 2>&1 &"
T0=$(date +%s); echo "reboot at $(date +%T)"; sleep 30
for n in $(seq ${WAIT_N:-66}); do
  r=$(S 'cat /proc/sys/kernel/random/boot_id; grub-editenv /boot/grub/grubenv list; cut -d" " -f1 /proc/uptime' 2>/dev/null)
  if [ -n "$r" ] && [ "$(head -1 <<<"$r")" = "$B0" ] && [ "$n" -ge 6 ]; then   # still the old boot 60 s later
    echo "reboot did not happen: clearing next_entry, STOP"; clear_next; exit 2
  fi
  [ -n "$r" ] && [ "$(head -1 <<<"$r")" != "$B0" ] && {
      grep -qx saved_entry=0 <<<"$r" && ! grep -q 'next_entry=.' <<<"$r" || { echo "BACK BUT grubenv not default: STOP"; echo "$r"; exit 4; }
      echo "back after $(( $(date +%s) - T0 )) s, uptime $(tail -1 <<<"$r") s, grubenv default"
      tail -c +$((mark + 1)) "$LOG" | grep -a -v '^--- ' | awk '!seen[$0]++' | sed 's/^nc110 [0-9] [0-9/]* //'; exit 0; }
  sleep 5
done
echo "NOT BACK (next_entry may still be set if the reboot never happened) within $(( ${WAIT_N:-66} * 5 / 60 + 1 )) min of $(date -d @$T0 +%T 2>/dev/null || echo reboot): STOP, do not retry, needs a power cycle"
tail -c +$((mark + 1)) "$LOG" | grep -a -v '^--- ' | awk '!seen[$0]++'; exit 3
