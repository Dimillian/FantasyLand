#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
if [ -x "$PWD/.tools/cargo/bin/cargo" ]; then
  export CARGO_HOME="$PWD/.tools/cargo"
  export RUSTUP_HOME="$PWD/.tools/rustup"
  export PATH="$CARGO_HOME/bin:$PATH"
fi
cargo build --release --lib --target wasm32-unknown-unknown --locked
if [ -x "$PWD/.tools/wasm-bindgen-0.2.128-aarch64-apple-darwin/wasm-bindgen" ]; then
  "$PWD/.tools/wasm-bindgen-0.2.128-aarch64-apple-darwin/wasm-bindgen" --target web --out-dir dist/pkg --out-name fantasy_land target/wasm32-unknown-unknown/release/fantasy_land.wasm
else
  wasm-bindgen --target web --out-dir dist/pkg --out-name fantasy_land target/wasm32-unknown-unknown/release/fantasy_land.wasm
fi
