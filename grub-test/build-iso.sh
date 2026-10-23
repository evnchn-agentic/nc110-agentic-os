#!/bin/sh
# Build the q35 GRUB rehearsal ISOs in a throwaway amd64 Debian container (GRUB 2.06, as Debian 12 ships).
# Run from the repo root: sh grub-test/build-iso.sh   -> grub-test/wdt.iso (good), grub-test/wdt-badrcba.iso (guard test),
#   grub-test/wdt-locked-{refuse,accept}.iso (TCO_LOCK=1 as on the NC110 BIOS)
set -e
cp grub-test/tco-arm.cfg grub-test/iso/boot/grub/tco-arm.cfg
cp target/grub/x86_64-unknown-none/release/nc110-kernel grub-test/iso/boot/k.elf
docker run --rm --platform linux/amd64 -v "$PWD/grub-test:/w" debian:bookworm-slim sh -c '
  set -e
  apt-get update -qq >/dev/null
  apt-get install -y -qq --no-install-recommends grub-pc-bin grub-common xorriso mtools >/dev/null 2>&1
  for f in /w/tco-arm.cfg /w/iso/boot/grub/grub.cfg /w/m1-nc110-entry.cfg; do grub-script-check "$f"; done
  echo "echo \"grub: mode good\"" > /w/iso/boot/grub/mode.cfg
  grub-mkrescue -o /w/wdt.iso /w/iso >/tmp/mk.log 2>&1 || { cat /tmp/mk.log; exit 1; }
  echo "set rcba_expect=fed1c000; echo \"grub: mode badrcba\"" > /w/iso/boot/grub/mode.cfg
  grub-mkrescue -o /w/wdt-badrcba.iso /w/iso >/tmp/mk.log 2>&1 || { cat /tmp/mk.log; exit 1; }
  # TCO_LOCK rehearsals: emulate the NC110 BIOS (TCO_EN=1 then TCO_LOCK=1), probe whether QEMU enforces the lock,
  # and put TCO_EN back if it did not, so tco_arm sees the NC110 state either way.
  lock="outl 0x630 0x2000 0x2000; outw 0x668 0x1000 0x1000; outl 0x630 0 0x2000; inl -v p 0x630; echo \"q35: TCO_EN clear attempt under TCO_LOCK -> SMI_EN=\$p\"; outl 0x630 0x2000 0x2000"
  echo "$lock; echo \"grub: mode locked-refuse\"" > /w/iso/boot/grub/mode.cfg
  grub-mkrescue -o /w/wdt-locked-refuse.iso /w/iso >/tmp/mk.log 2>&1 || { cat /tmp/mk.log; exit 1; }
  echo "$lock; set lock_mode=accept; echo \"grub: mode locked-accept\"" > /w/iso/boot/grub/mode.cfg
  grub-mkrescue -o /w/wdt-locked-accept.iso /w/iso >/tmp/mk.log 2>&1 || { cat /tmp/mk.log; exit 1; }
  echo "echo \"grub: mode good\"" > /w/iso/boot/grub/mode.cfg'
