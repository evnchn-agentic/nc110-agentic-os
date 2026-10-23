#!/usr/bin/env bash
# Demo for ask A179-1018-5: provisioning without going into Linux. One kick (next_entry only), then two brand-new
# builds loaded over the network, then a network reboot. grub.cfg + /boot/baremetal hashes are taken before and
# after to show the session wrote nothing to /boot. Log: proof/nc110-nolinux-demo.log
set -u
cd "$(dirname "$0")/.."
LOG=proof/nc110-nolinux-demo.log; : > "$LOG"; exec > >(tee -a "$LOG") 2>&1
S() { ssh -F /dev/null -o BatchMode=yes -o ConnectTimeout=5 -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null \
  -o LogLevel=ERROR -i "$HOME/.ssh/id_ed25519_homelab" root@192.168.50.118 "$@"; }
B="tools/.venv/bin/python tools/bmctl.py"
H0=$(S 'sha256sum /boot/grub/grub.cfg; ls /boot/baremetal | sha256sum'); echo "== /boot before: $H0"
echo "== 1. kick (the only Linux touch: one-shot next_entry, no file written)"; tools/nc110-kick.sh || exit 1
for n in 1 2; do
  BID="nolinux-demo-$n-$(date +%H%M%S)"
  NC110_BUILD=$BID cargo build -q --release --features attended --target-dir target/demo$n
  K=target/demo$n/x86_64-unknown-none/release/nc110-kernel
  echo "== $((n+1)). build $BID ($(shasum -a 256 $K | cut -c1-16)), never written to /boot; load over the network"
  $B load $K bmctl demo=$n || exit 1
  $B logall | grep -E 'build:|cmdline:|inherited|keymod'
done
echo "== 4. back to Linux over the network"; $B reboot
for _ in $(seq 40); do sleep 5; H1=$(S 'sha256sum /boot/grub/grub.cfg; ls /boot/baremetal | sha256sum' 2>/dev/null) && break; done
echo "== /boot after:  $H1"; echo "== grubenv: $(S 'grub-editenv /boot/grub/grubenv list | tr "\n" " "')"
[ "$H0" = "$H1" ] && echo "RESULT: PASS, /boot unchanged across the session" || { echo "RESULT: /boot CHANGED"; exit 1; }
