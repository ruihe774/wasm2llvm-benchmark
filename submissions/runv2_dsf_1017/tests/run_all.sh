#!/bin/bash
# Build the translator, build the test harness and run the differential suite.
set -u
cd "$(dirname "$0")/.."
export PATH="/home/msk/.cargo/bin:/mnt/workspace/wasm2llvm/deps/bin:$PATH"
export CARGO_HOME="$PWD/.cargo-home"
cargo build --release --offline || exit 1
clang -O2 -rdynamic -o "${TMPDIR:-/tmp}/w2l-harness" tests/harness.c -ldl || exit 1
mkdir -p "${TMPDIR:-/tmp}/w2l-fuzz"
wat2wasm tests/smith/exports.wat -o "${TMPDIR:-/tmp}/exports.wasm"
status=0
for opt in O2 O0; do
  for t in test_payloads test_traps test_numeric test_init test_control test_rust test_misc; do
    echo "### $t $opt"
    timeout 1200 python3 -I "tests/$t.py" "$opt" || status=1
  done
done
echo "### scope"
python3 -I tests/test_scope.py || status=1
echo "### fuzz smith O2"
timeout 1800 python3 -I tests/fuzz_smith.py 12000 60 O2 || status=1
echo "### fuzz smith O0"
timeout 1800 python3 -I tests/fuzz_smith.py 13000 60 O0 || status=1
echo "### fuzz validation"
timeout 1800 python3 -I tests/fuzz_validation.py \
  "${TMPDIR:-/tmp}/w2l-fuzz/control.wasm" "${TMPDIR:-/tmp}/w2l-fuzz/numeric.wasm" \
  payloads/multi-value/multi-value.wasm || status=1
exit $status
