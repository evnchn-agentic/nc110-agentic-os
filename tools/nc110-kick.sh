#!/usr/bin/env bash
# Start a bare-metal session on the NC110 with the ONE remaining Linux touch: one-shot next_entry to the stable
# "nc110-bm-netloader" GRUB entry (installed once, 2026-10-18), then reboot. No kernel copy, no grub.cfg edit.
# From then on everything is over the network:
#   tools/.venv/bin/python tools/bmctl.py load <kernel.elf> bmctl [args]   # new build, in RAM, nothing on /boot
#   tools/.venv/bin/python tools/bmctl.py ping|logall|peek ...|reboot      # reboot = back to Slot A Linux
# Safety: saved_entry stays 0 (checked before and after); any hang -> TCO reset -> GRUB default -> Slot A.
# Exit 0 = netloader answered an authenticated ping. Exit 3 = nothing answered within 3 min: card evnchn.
set -u
cd "$(dirname "$0")/.."
E=nc110-bm-netloader
S() { ssh -F /dev/null -o BatchMode=yes -o ConnectTimeout=5 -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null \
  -o LogLevel=ERROR -i "$HOME/.ssh/id_ed25519_homelab" root@192.168.50.118 "$@"; }
st=$(S "hostname; awk '\$2==\"/\"{print \$1}' /proc/mounts; grub-editenv /boot/grub/grubenv list; grep -c '^menuentry \"$E\"' /boot/grub/grub.cfg") \
  || { echo "Slot A not reachable: STOP"; exit 2; }
grep -qx nc110 <<<"$st" && grep -qx /dev/sda2 <<<"$st" && grep -qx saved_entry=0 <<<"$st" && ! grep -q 'next_entry=.' <<<"$st" \
  && [ "$(tail -1 <<<"$st")" = 1 ] || { echo "not in the expected Slot A state (or no $E entry): STOP"; echo "$st"; exit 2; }
S "grub-editenv /boot/grub/grubenv set next_entry='$E' && grub-editenv /boot/grub/grubenv list | grep -qx 'next_entry=$E' && sync && (sleep 1; reboot) >/dev/null 2>&1 &" \
  || { echo "could not arm next_entry: STOP"; exit 2; }
echo "kicked at $(date +%T): one-shot $E"
for _ in $(seq 60); do
  sleep 3
  r=$(tools/.venv/bin/python tools/bmctl.py ping 2>/dev/null) && { echo "netloader up: $r"; exit 0; }
done
echo "netloader did not answer within 3 min: if Slot A is back, nothing is armed; otherwise card evnchn (power cycle)"
exit 3
