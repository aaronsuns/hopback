#!/usr/bin/env bash
# Format, lint, typecheck and test everything. CI runs this on every push.
set -euo pipefail
cd "$(dirname "$0")/.."

step() { printf '\n==> %s\n' "$*"; }

step "npm ci"
[ -d node_modules ] || npm ci

step "frontend build (tauri needs dist/ to compile)"
npm run build

step "hopback-hook sidecar (tauri needs it to compile)"
scripts/build-hook.sh >/dev/null

step "cargo fmt"
cargo fmt --all -- --check

step "cargo clippy"
cargo clippy --workspace --all-targets -- -D warnings

step "tsc"
npx tsc --noEmit

step "vitest"
npx vitest run

step "cargo test"
cargo test --workspace

printf '\nAll checks passed.\n'
