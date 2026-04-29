#!/usr/bin/env bash
set -euo pipefail

ROOT="$(CDPATH='' cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
KOKUBAN="$ROOT/kokuban"

assert_contains() {
  local haystack="$1"
  local needle="$2"
  if [[ "$haystack" != *"$needle"* ]]; then
    echo "Expected output to contain: $needle" >&2
    echo "Actual output:" >&2
    echo "$haystack" >&2
    exit 1
  fi
}

tmpdir="$(mktemp -d)"
trap 'rm -rf "$tmpdir"' EXIT

export KOKUBAN_CONFIG="$tmpdir/config"

output="$(KOKUBAN_CORE=/bin/echo "$KOKUBAN" build s25_sm8750 resukisu resukisu --no-bbg)"
assert_contains "$output" "local --project s25_sm8750 --branch resukisu --variant resukisu"
assert_contains "$output" "--apply-bbg false"

output="$(KOKUBAN_CORE=/bin/echo "$KOKUBAN" plan s23_sm8550 main default --no-susfs --no-bbg)"
assert_contains "$output" "--dry-run"
assert_contains "$output" "--apply-susfs false"
assert_contains "$output" "--apply-bbg false"

"$KOKUBAN" config set apply_susfs false >/dev/null
output="$("$KOKUBAN" plan s23_sm8550)"
assert_contains "$output" "apply_susfs: false"

"$KOKUBAN" preset set daily s25_sm8750 resukisu resukisu --no-bbg >/dev/null
output="$("$KOKUBAN" run daily --offline --dry-run)"
assert_contains "$output" "apply_bbg: false"
assert_contains "$output" "offline: true"

echo "OK: kokuban CLI wrapper tests passed."
