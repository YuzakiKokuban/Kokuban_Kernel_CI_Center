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

output="$(KOKUBAN_CORE=/bin/echo "$KOKUBAN" build s25_sm8750 bakasu bakasu --no-bbg --no-hybridmount)"
assert_contains "$output" "local --project s25_sm8750 --branch bakasu --variant bakasu"
assert_contains "$output" "--apply-bbg false"
assert_contains "$output" "--apply-hybridmount false"

output="$(KOKUBAN_CORE=/bin/echo "$KOKUBAN" plan s23_sm8550 main default --no-bbg)"
assert_contains "$output" "--dry-run"
assert_contains "$output" "--apply-bbg false"

"$KOKUBAN" config set apply_bbg false >/dev/null
output="$("$KOKUBAN" plan s23_sm8550)"
assert_contains "$output" "apply_bbg: false"
assert_contains "$output" "apply_hybridmount: true"

"$KOKUBAN" preset set daily s25_sm8750 bakasu bakasu --no-bbg >/dev/null
output="$("$KOKUBAN" run daily --offline --dry-run)"
assert_contains "$output" "apply_bbg: false"
assert_contains "$output" "offline: true"

output="$("$KOKUBAN" plan oneplus15_sm8850 bakasu bakasu)"
assert_contains "$output" "apply_susfs: true"
assert_contains "$output" "bakasu-bakasu-susfs"

output="$("$KOKUBAN" plan oneplus15_sm8850 bakasu bakasu --no-susfs)"
assert_contains "$output" "apply_susfs: false"
if [[ "$output" == *"bakasu-bakasu-susfs"* ]]; then
  echo "SUSFS and manual builds must use separate source workspaces" >&2
  exit 1
fi

output="$("$KOKUBAN" plan oneplus15_sm8850 main default)"
assert_contains "$output" "apply_susfs: false"
if "$KOKUBAN" plan oneplus15_sm8850 main default --with-susfs >/dev/null 2>&1; then
  echo "SUSFS must reject an LKM build" >&2
  exit 1
fi

if "$KOKUBAN" plan oneplus15_sm8850 bakasu bakasu --with-susfs --no-susfs >/dev/null 2>&1; then
  echo "Conflicting SUSFS switches must be rejected" >&2
  exit 1
fi

echo "OK: kokuban CLI wrapper tests passed."
