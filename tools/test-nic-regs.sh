#!/bin/sh
# Host unit test: src/nic_regs.rs vs pinned Linux v6.12 r8169_main.c / 8139cp.c (third_party/linux-v6.12).
set -e
D=$(cd "$(dirname "$0")" && pwd)
(cd "$D/../third_party/linux-v6.12" && shasum -a 256 -c SHA256SUMS)
mkdir -p "$D/../target/host"
NIC_TEST_DIR="$D" rustc --edition 2021 --test "$D/nic_regs_test.rs" -o "$D/../target/host/nic_regs_test"
"$D/../target/host/nic_regs_test" --nocapture --test-threads 1
