#!/bin/bash
# Fetch everything the benchmark needs that is not tracked in git (see .gitignore):
#
#   deps/downloads/*      release archives (checked against deps/downloads/SHA256SUMS)
#   deps/toolchains/*     extracted LLVM, wabt, wasm-tools, wasmtime
#   deps/src/*            spec repos (wg-2.0, wg-3.0), LLVM docs checkout, spec HTML
#   deps/bin/w2l-ref      wasmtime-based reference runner (built with cargo)
#   verification/reference/third_party/{wabt-src,simde}   used by the wasm2c baseline
#
# Idempotent: existing pieces are skipped. The deps/bin symlinks are tracked in git.
# Needs network, git, curl, tar, zstd, xz, sha256sum, and rustup/cargo.
#
#   ./setup.sh [--skip-ref-build]
set -euo pipefail
W2L="$(cd "$(dirname "$(readlink -f "$0")")" && pwd)"
cd "$W2L"

BUILD_REF=1
[ "${1:-}" = "--skip-ref-build" ] && BUILD_REF=0

LLVM_VER=23.1.4
WABT_VER=1.0.42
WASM_TOOLS_VER=1.262.0
WASMTIME_VER=50.0.0
WABT_COMMIT=ff0ef7e0009402740c805a9744c09b05be063e48
SIMDE_COMMIT=71fd833d9666141edcd1d3c109a80e228303d8d7

DL=deps/downloads
TC=deps/toolchains
SRC=deps/src
TP=verification/reference/third_party

log() { echo "==> $*"; }
need() { command -v "$1" >/dev/null || { echo "missing required tool: $1" >&2; exit 1; }; }
for t in git curl tar zstd xz sha256sum python3 bwrap socat; do need "$t"; done
[ "$BUILD_REF" = 1 ] && for t in rustup cargo; do need "$t"; done

mkdir -p "$DL" "$TC" "$SRC" "$TP"

# --- release archives ---------------------------------------------------------
fetch() { # file url
  if [ ! -f "$DL/$1" ]; then
    log "download $1"
    curl -fL --retry 3 -o "$DL/$1.part" "$2"
    mv "$DL/$1.part" "$DL/$1"
  fi
  (cd "$DL" && awk -v f="$1" '$2 == f' SHA256SUMS | sha256sum -c --quiet -) \
    || { echo "checksum mismatch for $1" >&2; exit 1; }
}

LLVM_A=LLVM-$LLVM_VER-Linux-X64.tar.zst
WABT_A=wabt-$WABT_VER-linux-x64.tar.gz
WT_A=wasm-tools-$WASM_TOOLS_VER-x86_64-linux.tar.gz
WM_A=wasmtime-v$WASMTIME_VER-x86_64-linux.tar.xz

fetch "$LLVM_A" "https://github.com/llvm/llvm-project/releases/download/llvmorg-$LLVM_VER/$LLVM_A"
fetch "$WABT_A" "https://github.com/WebAssembly/wabt/releases/download/$WABT_VER/$WABT_A"
fetch "$WT_A" "https://github.com/bytecodealliance/wasm-tools/releases/download/v$WASM_TOOLS_VER/$WT_A"
fetch "$WM_A" "https://github.com/bytecodealliance/wasmtime/releases/download/v$WASMTIME_VER/$WM_A"

# --- toolchains ---------------------------------------------------------------
if [ ! -x "$TC/LLVM-$LLVM_VER-Linux-X64/bin/clang" ]; then
  log "extract LLVM (large; the archive needs a 2 GiB zstd window)"
  zstd -dc --long=31 "$DL/$LLVM_A" | tar -x -C "$TC"
fi
ln -sfn "LLVM-$LLVM_VER-Linux-X64" "$TC/llvm"
[ -x "$TC/wabt-$WABT_VER/bin/wasm2c" ] || { log "extract wabt"; tar -xzf "$DL/$WABT_A" -C "$TC"; }
[ -x "$TC/wasm-tools-$WASM_TOOLS_VER-x86_64-linux/wasm-tools" ] || { log "extract wasm-tools"; tar -xzf "$DL/$WT_A" -C "$TC"; }
[ -x "$TC/wasmtime-v$WASMTIME_VER-x86_64-linux/wasmtime" ] || { log "extract wasmtime"; tar -xJf "$DL/$WM_A" -C "$TC"; }

# --- source checkouts ---------------------------------------------------------
clone_tag() { # dir url tag
  [ -d "$1/.git" ] && return 0
  log "clone $2 @ $3"
  git clone -q --depth 1 --branch "$3" "$2" "$1"
}
clone_commit() { # dir url commit
  [ -d "$1/.git" ] && return 0
  log "clone $2 @ ${3:0:12}"
  git init -q "$1"
  git -C "$1" remote add origin "$2"
  git -C "$1" fetch -q --depth 1 origin "$3"
  git -C "$1" checkout -q FETCH_HEAD
}

clone_tag "$SRC/spec-wg-2.0" https://github.com/WebAssembly/spec.git wg-2.0
clone_tag "$SRC/spec-wg-3.0" https://github.com/WebAssembly/spec.git wg-3.0

if [ ! -d "$SRC/llvm-project-$LLVM_VER/.git" ]; then
  log "clone llvm-project docs @ llvmorg-$LLVM_VER"
  git clone -q --depth 1 --filter=blob:none --sparse --branch "llvmorg-$LLVM_VER" \
    https://github.com/llvm/llvm-project.git "$SRC/llvm-project-$LLVM_VER"
  git -C "$SRC/llvm-project-$LLVM_VER" sparse-checkout set llvm/docs
fi

if [ ! -s "$SRC/wasm-core-2.html" ]; then
  log "download WebAssembly core 2.0 spec HTML (best effort)"
  curl -fL --retry 3 -o "$SRC/wasm-core-2.html" https://www.w3.org/TR/wasm-core-2/ \
    || { rm -f "$SRC/wasm-core-2.html"; echo "warning: could not fetch wasm-core-2.html" >&2; }
fi

# wasm2c runtime sources + simde for the verification/reference baseline
# (submodules of wabt are not needed; simde is cloned separately)
clone_commit "$TP/wabt-src" https://github.com/WebAssembly/wabt.git "$WABT_COMMIT"
clone_commit "$TP/simde" https://github.com/simd-everywhere/simde.git "$SIMDE_COMMIT"

# --- Rust ---------------------------------------------------------------------
if [ "$BUILD_REF" = 1 ]; then
  log "rustup targets"
  rustup target add wasm32v1-none wasm32-unknown-unknown x86_64-unknown-linux-gnu
  log "build w2l-ref"
  (cd agent/tools/w2l-ref && CARGO_TARGET_DIR="$W2L/deps/build/w2l-ref" cargo build --release)
  cp "deps/build/w2l-ref/release/w2l-ref" deps/bin/w2l-ref
fi

# --- sanity -------------------------------------------------------------------
log "versions"
export PATH="$W2L/deps/bin:$PATH"
clang --version | head -1
wasm-tools --version
wat2wasm --version
wasmtime --version
[ -x deps/bin/w2l-ref ] && echo "w2l-ref: ok"
echo "setup complete"
