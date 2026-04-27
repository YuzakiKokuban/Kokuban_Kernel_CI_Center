#!/usr/bin/env bash
set -euo pipefail

ROOT="$(CDPATH='' cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"

bash -n "$ROOT/kokuban"
bash -n "$ROOT/scripts/test-kokuban-cli.sh"
bash -n "$ROOT/scripts/check-shell.sh"

if command -v shellcheck >/dev/null 2>&1; then
  shellcheck "$ROOT/kokuban" "$ROOT/scripts/test-kokuban-cli.sh" "$ROOT/scripts/check-shell.sh"
else
  echo "shellcheck not found; bash -n checks passed."
fi
