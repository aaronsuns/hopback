#!/usr/bin/env bash
# Build hopback-hook and put it where Tauri's `externalBin` expects it:
# src-tauri/binaries/hopback-hook-<target-triple>. Handles universal builds.
set -euo pipefail
cd "$(dirname "$0")/.."
target="${TAURI_ENV_TARGET_TRIPLE:-${1:-$(rustc -vV | sed -n 's/^host: //p')}}"
out=src-tauri/binaries
mkdir -p "$out"
if [ "$target" = "universal-apple-darwin" ]; then
  for t in aarch64-apple-darwin x86_64-apple-darwin; do
    cargo build --release -p hopback-hook --target "$t"
  done
  lipo -create -output "$out/hopback-hook-universal-apple-darwin" \
    target/aarch64-apple-darwin/release/hopback-hook target/x86_64-apple-darwin/release/hopback-hook
else
  cargo build --release -p hopback-hook --target "$target"
  cp "target/$target/release/hopback-hook" "$out/hopback-hook-$target"
fi
