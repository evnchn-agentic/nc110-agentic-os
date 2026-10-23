#!/bin/sh
# M2.5: GRUB 2.06 ISO that boots our kernel with the client key as a multiboot `module` (the hardware path).
# Run from the repo root after `cargo build --release`: sh grub-test/build-keymod-iso.sh -> grub-test/keymod.iso
# The ISO contains the throwaway TEST key; it is gitignored and must never leave this machine.
set -e
cp target/x86_64-unknown-none/release/nc110-kernel grub-test/keymod-iso/boot/k.elf
cp test-keys/client.key grub-test/keymod-iso/boot/nc110-client.key
docker run --rm --platform linux/amd64 -v "$PWD/grub-test:/w" debian:bookworm-slim sh -c '
  set -e
  apt-get update -qq >/dev/null
  apt-get install -y -qq --no-install-recommends grub-pc-bin grub-common xorriso mtools >/dev/null 2>&1
  grub-script-check /w/keymod-iso/boot/grub/grub.cfg
  grub-mkrescue -o /w/keymod.iso /w/keymod-iso 2>&1 | tail -1'
rm -f grub-test/keymod-iso/boot/nc110-client.key grub-test/keymod-iso/boot/k.elf
