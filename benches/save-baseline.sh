#!/usr/bin/env bash
# Runs the keymap benchmarks, saves the criterion baseline, and copies
# the estimates into benches/baselines/ for version control.
#
# Usage:
#   ./benches/save-baseline.sh [baseline-name]
#
# Example:
#   ./benches/save-baseline.sh master

set -euo pipefail

BASELINE="${1:-master}"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$SCRIPT_DIR/.."

cargo bench -p helix-term --bench keymap -- --save-baseline "$BASELINE"

DST="$SCRIPT_DIR/baselines/$BASELINE"
mkdir -p "$DST"

for bench_dir in "$REPO_ROOT/target/criterion"/*/; do
  name=$(basename "$bench_dir")
  src="$bench_dir/$BASELINE/estimates.json"
  if [[ -f "$src" ]]; then
    mkdir -p "$DST/$name"
    cp "$src" "$DST/$name/estimates.json"
    echo "saved: $name"
  fi
done

echo "done — commit benches/baselines/$BASELINE/ to persist the baseline"
