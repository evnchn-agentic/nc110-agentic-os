#!/bin/sh
# Host unit tests for src/term.rs (the kernel's VGA terminal), then the mutant check (each feature stubbed -> red).
set -e
D=$(cd "$(dirname "$0")" && pwd)
mkdir -p "$D/../target/host"
rustc --edition 2021 --test "$D/term_test.rs" -o "$D/../target/host/term_test"
"$D/../target/host/term_test" --test-threads 1
python3 "$D/term-mutants.py"
