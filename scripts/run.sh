#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
mori_rust_env="${CARGO_HOME:-$HOME/.cargo}/env"
if [[ -f "$mori_rust_env" ]]; then
  source "$mori_rust_env"
fi
exec cargo run --release --locked -- "$@"
