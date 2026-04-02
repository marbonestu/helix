#!/usr/bin/env bash
# Restores committed criterion baselines into target/criterion/ so that
# `cargo bench -- --baseline <name>` can compare against them.
#
# Usage:
#   ./benches/restore-baseline.sh [baseline-name]
#
# Example:
#   ./benches/restore-baseline.sh master
#   cargo bench -p helix-term --bench keymap -- --baseline master

set -euo pipefail

BASELINE="${1:-master}"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
SRC="$SCRIPT_DIR/baselines/$BASELINE"
DST="$SCRIPT_DIR/../target/criterion"

if [[ ! -d "$SRC" ]]; then
  echo "error: no stored baseline '$BASELINE' at $SRC" >&2
  exit 1
fi

for bench_dir in "$SRC"/*/; do
  name=$(basename "$bench_dir")
  dest="$DST/$name/$BASELINE"
  mkdir -p "$dest"
  cp "$bench_dir/estimates.json" "$dest/estimates.json"
  echo "restored: $name"
done

echo "done — run: cargo bench -p helix-term --bench keymap -- --baseline $BASELINE"
