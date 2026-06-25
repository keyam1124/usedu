#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

export MACOSX_DEPLOYMENT_TARGET="${MACOSX_DEPLOYMENT_TARGET:-12.0}"

if ! command -v cargo >/dev/null 2>&1; then
  if [ -x /opt/homebrew/opt/rustup/bin/cargo ]; then
    export PATH="/opt/homebrew/opt/rustup/bin:$PATH"
  else
    echo "cargo not found. Install Rust with rustup before running this script." >&2
    exit 1
  fi
fi

rustup target add aarch64-apple-darwin x86_64-apple-darwin

cargo build --release --target aarch64-apple-darwin
cargo build --release --target x86_64-apple-darwin

mkdir -p dist
rm -f dist/usedu dist/usedu-aarch64-apple-darwin dist/usedu-x86_64-apple-darwin
rm -f dist/usedu-macos-universal.tar.gz dist/SHA256SUMS

cp target/aarch64-apple-darwin/release/usedu dist/usedu-aarch64-apple-darwin
cp target/x86_64-apple-darwin/release/usedu dist/usedu-x86_64-apple-darwin
lipo -create \
  dist/usedu-aarch64-apple-darwin \
  dist/usedu-x86_64-apple-darwin \
  -output dist/usedu

strip -x dist/usedu-aarch64-apple-darwin dist/usedu-x86_64-apple-darwin dist/usedu
codesign --force --sign - dist/usedu >/dev/null 2>&1 || true

tar -czf dist/usedu-macos-universal.tar.gz -C dist usedu
shasum -a 256 \
  dist/usedu \
  dist/usedu-aarch64-apple-darwin \
  dist/usedu-x86_64-apple-darwin \
  dist/usedu-macos-universal.tar.gz > dist/SHA256SUMS

echo "Built:"
file dist/usedu dist/usedu-aarch64-apple-darwin dist/usedu-x86_64-apple-darwin
cat dist/SHA256SUMS
