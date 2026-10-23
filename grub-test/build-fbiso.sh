#!/bin/sh
# q35 rehearsal ISO for the framebuffer console: GRUB (Debian 12, 2.06) sets gfxpayload=1024x600x32,auto and multiboots
# the attended kernel. Run from the repo root after building it: sh grub-test/build-fbiso.sh -> grub-test/fb.iso
set -e
cp target/attended/x86_64-unknown-none/release/nc110-kernel grub-test/fbiso/boot/k.elf
cp test-keys/client.key grub-test/fbiso/boot/key
docker run --rm --platform linux/amd64 -v "$PWD/grub-test:/w" debian:bookworm-slim sh -c '
  set -e
  apt-get update -qq >/dev/null
  apt-get install -y -qq --no-install-recommends grub-pc-bin grub-common xorriso mtools >/dev/null 2>&1
  grub-script-check /w/fbiso/boot/grub/grub.cfg
  grub-mkrescue -o /w/fb.iso /w/fbiso >/tmp/mk.log 2>&1 || { cat /tmp/mk.log; exit 1; }'
ls -la grub-test/fb.iso
