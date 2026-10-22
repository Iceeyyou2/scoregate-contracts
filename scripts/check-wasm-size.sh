#!/usr/bin/env bash
set -euo pipefail

DOC="docs/wasm-size-budget.md"

CONTRACTS=(
  "scoregate-score:scoregate_score"
  "scoregate-aggregator:scoregate_aggregator"
  "mock-amm:mock_amm"
  "mock-lending:mock_lending"
)

FAIL=0

for entry in "${CONTRACTS[@]}"; do
  CRATE="${entry%%:*}"
  BIN="${entry##*:}"
  WASM_PATH="target/wasm32-unknown-unknown/release/${BIN}.wasm"

  if [[ ! -f "$WASM_PATH" ]]; then
    echo "WASM binary not found at $WASM_PATH. Building..."
    cargo build --target wasm32-unknown-unknown --release -p "$CRATE" --locked
  fi

  ACTUAL=$(wc -c < "$WASM_PATH" | tr -d '[:space:]')

  # Extract per-contract budget if present, else default 1 MiB
  BUDGET_STR=$(grep -i "$CRATE" "$DOC" 2>/dev/null | grep -oE '[0-9,]+ bytes' | grep -oE '[0-9,]+' | tr -d ',' | head -1 || true)
  if [[ -z "$BUDGET_STR" ]]; then
    BUDGET_STR=1048576
  fi
  BUDGET=$BUDGET_STR
  TOLERANCE=5

  MAX_ALLOWED=$(awk "BEGIN {print int($BUDGET * (1 + $TOLERANCE / 100))}")

  echo "WASM Size Check ($CRATE):"
  echo "  Actual: $ACTUAL bytes"
  echo "  Budget: $BUDGET bytes"
  echo "  Tolerance: $TOLERANCE%"
  echo "  Max Allowed: $MAX_ALLOWED bytes"

  if [[ "$ACTUAL" -gt "$MAX_ALLOWED" ]]; then
    echo "❌ ERROR: $CRATE WASM size budget exceeded!"
    FAIL=1
  else
    echo "✅ $CRATE WASM size is within budget."
  fi
  echo ""
done

if [[ "$FAIL" -ne 0 ]]; then
  exit 1
fi
