#!/usr/bin/env bash
# From the agent OS to Slot A Linux with no hands (Codex agent-OS finding 3): chain-load the custom i386 kernel
# (~/nc110-kbuild, SLOTB-BUILD-FORMULA config + PACKET=y, no initrd, no modules) with root=/dev/sda2.
# watchdog.open_timeout=300: the kernel feeds the inherited TCO for at most 5 min unless something opens
# /dev/watchdog, so a Linux that never comes up resets back into the agent OS (the GRUB default) by itself.
# nc110.lease=600 starts Slot A's /etc/local.d/nc110-lease.start: it feeds the TCO only while an agent renews
# /dev/shm/nc110-lease, so a Linux that loses its agent (NIC/SSH dies) resets back into the agent OS within ~11 min.
# Renew from the Mac:  ssh root@192.168.50.118 touch /dev/shm/nc110-lease    `reboot` returns to the agent OS at once.
set -u
cd "$(dirname "$0")/.."
BZ=${BZ:-$HOME/nc110-kbuild/linux-6.18.44/arch/x86/boot/bzImage}
S() { ssh -F /dev/null -o BatchMode=yes -o ConnectTimeout=5 -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null \
  -o LogLevel=ERROR -i "$HOME/.ssh/id_ed25519_homelab" root@192.168.50.118 "$@"; }
tools/.venv/bin/python tools/bmctl.py linux "$BZ" root=/dev/sda2 ro rootfstype=ext4 rootwait console=tty0 watchdog.open_timeout=300 nc110.lease=600 || exit 1
T0=$(date +%s)
for _ in $(seq 60); do
  sleep 5
  if S 'uname -r; hostname' 2>/dev/null; then
    # readiness = the lease service's loop (its script text names /dev/shm/nc110-lease) owns /dev/watchdog
    if S 'for p in $(pgrep -f "shm/nc110-lease"); do ls -l /proc/$p/fd 2>/dev/null | grep -q /dev/watchdog && exit 0; done; exit 1'; then
      echo "Slot A Linux up after $(( $(date +%s) - T0 )) s; the lease service holds /dev/watchdog (renew: touch /dev/shm/nc110-lease every < 10 min)."
    else
      echo "Slot A Linux up, but the lease service does NOT hold /dev/watchdog: open_timeout (300 s) will reset it into the agent OS"
      exit 4
    fi
    exit 0
  fi
done
echo "Linux did not answer SSH within 5 min; the 300 s watchdog open_timeout returns the board to the agent OS"; exit 3
