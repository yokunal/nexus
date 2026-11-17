#!/usr/bin/env bash
# Verification gate for Nexus 2.0 68-test autonomous battery
set -eo pipefail

MODE="${1:-full}"
PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$PROJECT_ROOT"

echo "==> Stage 1: Correctness (Nexus 2.0 Test Battery)"
cargo test -p nexus-v2 -- --nocapture

if [ "$MODE" = "full" ]; then
  echo "==> Stage 2: Code Quality (Workspace Cargo Check)"
  cargo check --all-targets

  echo "==> Stage 3: Cleanliness & Disk Headroom Check"
  FREE_MB=$(df -m / | awk 'NR==2 {print $4}')
  echo "Available disk headroom: ${FREE_MB} MB"
  if [ "$FREE_MB" -lt 500 ]; then
    echo "WARNING: Disk headroom below 500MB, cleaning debug artifacts..."
    rm -rf target/debug
  fi
fi

echo "VERIFY PASSED ($MODE mode)"
exit 0
