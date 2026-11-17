#!/usr/bin/env bash
# Verification gate. Layered into:
# Stage 1: Correctness (Unit/Integration tests)
# Stage 2: Code Quality (Types & Linters: tsc, eslint, ruff, mypy)
# Stage 3: Cleanliness & Refactor Check (Formatting, unused code)
#
# Usage:
#   bash scripts/verify.sh --quick   (runs fast Stage 1 checks)
#   bash scripts/verify.sh           (runs full all-stage checks)
set -eo pipefail

MODE="${1:-full}"

echo "==> Stage 1: Correctness (Tests)"
# The agent must replace this stub in Phase 0 (see references/verify-patterns.md).
echo "VERIFY FAILED: NOT CONFIGURED. Edit scripts/verify.sh for this project."
exit 2

if [ "$MODE" = "full" ]; then
  echo "==> Stage 2: Code Quality (Linters & Types)"
  # Add: ruff check ., npm run lint, tsc --noEmit, etc.

  echo "==> Stage 3: Cleanliness & Formatting"
  # Add: prettier --check, black --check, etc.
fi

echo "VERIFY PASSED ($MODE mode)"
exit 0
