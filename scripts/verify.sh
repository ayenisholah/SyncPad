#!/usr/bin/env bash
# Local verification loop: format, lint, and tests for everything present.
set -euo pipefail
cd "$(dirname "$0")/.."

# cargo may live outside PATH when rustup was installed with --no-modify-path
if ! command -v cargo >/dev/null 2>&1 && [ -x "$HOME/.cargo/bin/cargo" ]; then
  export PATH="$HOME/.cargo/bin:$PATH"
fi

# The windows-gnu toolchain needs binutils (as/dlltool) to build import
# libraries for crates using raw-dylib; fall back to an MSYS2 install.
if ! command -v dlltool >/dev/null 2>&1 && [ -d /c/msys64/mingw64/bin ]; then
  export PATH="$PATH:/c/msys64/mingw64/bin"
fi

if [ -f Cargo.toml ]; then
  echo "== server: cargo fmt --check"
  cargo fmt --all --check
  echo "== server: cargo clippy (-D warnings)"
  cargo clippy --workspace --all-targets -- -D warnings
  echo "== server: cargo test"
  cargo test --workspace
else
  echo "== server checks skipped (no Cargo.toml yet)"
fi

if [ -f web/package.json ]; then
  if [ ! -d web/node_modules ]; then
    echo "== web: npm ci"
    (cd web && npm ci)
  fi
  echo "== web: build"
  (cd web && npm run build)
  echo "== web: test"
  (cd web && npm test)
else
  echo "== web checks skipped (no web/package.json yet)"
fi

echo "verify: OK"
