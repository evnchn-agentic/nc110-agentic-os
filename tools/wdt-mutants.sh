#!/bin/sh
# Mutation check for the GRUB watchdog arming (brief 19:58 rule): each mutant breaks tco-arm.cfg one way, and
# the matching q35 rehearsal must then FAIL. Run from the repo root after sh grub-test/build-iso.sh.
# Logs: proof/wdt-mut-<name>.log. Result table on stdout; exit 1 if any mutant survives.
set -e
W=$(mktemp -d)
trap 'rm -rf "$W"' EXIT
survived=0
mutant() {  # $1 name, $2 ISO mode (locked-accept|locked-refuse), $3 run-wdt expect, $4 python edit on s
  rm -rf "$W/g"; mkdir -p "$W/g"; cp -R grub-test/iso "$W/g/iso"
  python3 -c "
import sys; p='$W/g/iso/boot/grub/tco-arm.cfg'; s=open(p).read(); o=s
$4
assert s != o, 'mutant did not apply'; open(p,'w').write(s)"
  lock="outl 0x630 0x2000 0x2000; outw 0x668 0x1000 0x1000"
  [ "$2" = locked-accept ] && lock="$lock; set lock_mode=accept"
  echo "$lock; echo \"grub: mode $2 mutant $1\"" > "$W/g/iso/boot/grub/mode.cfg"
  docker run --rm --platform linux/amd64 -v "$W/g:/w" debian:bookworm-slim sh -c '
    apt-get update -qq >/dev/null && apt-get install -y -qq --no-install-recommends grub-pc-bin grub-common xorriso mtools >/dev/null 2>&1
    grub-mkrescue -o /w/m.iso /w/iso >/tmp/mk.log 2>&1 || { cat /tmp/mk.log; exit 1; }'
  if ! python3 run-wdt.py "mut-$1-probe" "$W/g/m.iso" - 0 3 none >/dev/null 2>&1 && ! grep -q "mutant $1" "proof/wdt-mut-$1-probe.log"; then
    echo "INFRA    $1: mutant ISO never reached GRUB"; survived=1; return; fi
  rm -f "proof/wdt-mut-$1-probe.log"
  if python3 run-wdt.py "mut-$1" "$W/g/m.iso" - 2 25 "$3" $([ "$2" = locked-accept ] && echo locked) >/dev/null 2>&1; then
    echo "SURVIVED $1 ($2, expect=$3)"; survived=1
  else
    echo "killed   $1 ($2, expect=$3): $(grep -o 'VERDICT.*' "proof/wdt-mut-$1.log")"
  fi
}
# no-arm keeps a fake 'after:' line with the lock bits set, so only the reset assertion can kill it.
mutant no-arm      locked-accept watchdog "s += '\nfunction tco_arm { set wdt_ok=yes; echo \\\"wdt: after: GCS=20 TCO1_CNT=1800 TCO_TMR=0 SMI_EN=2021 fake\\\"; }\n'"
mutant no-unhalt   locked-accept watchdog "s = s.replace('  outw \$tco1_cnt 0 0x0800', '  true')"
mutant no-tmr      locked-accept watchdog "s = s.replace('  outw \$tco_tmr \$tmr_val 0x03ff', '  true')"
mutant no-noreboot locked-accept watchdog "s = s.replace('  write_dword \$gcs_addr 0 0x20', '  true')"
mutant accept-default locked-refuse refuse "s = s.replace('set lock_mode=refuse\n', 'set lock_mode=accept\n', 1)"
exit $survived
