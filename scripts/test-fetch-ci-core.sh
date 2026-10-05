#!/usr/bin/env bash
set -euo pipefail

ROOT="$(CDPATH='' cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
tmpdir="$(mktemp -d)"
trap 'rm -rf "$tmpdir"' EXIT
mkdir -p "$tmpdir/bin" "$tmpdir/output"

export MOCK_BINARY="$tmpdir/source-core"
export MOCK_EXEC_MARKER="$tmpdir/executed"
export MOCK_SLEEP_LOG="$tmpdir/slept"
export MOCK_EXPECTED_STAMP="aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
export MOCK_STAMP="$MOCK_EXPECTED_STAMP"
export MOCK_CHECKSUM_MODE=valid

cat > "$MOCK_BINARY" <<'EOF'
#!/usr/bin/env bash
printf 'executed\n' >> "$MOCK_EXEC_MARKER"
printf '%s\n' "$MOCK_STAMP"
EOF

cat > "$tmpdir/bin/git" <<'EOF'
#!/usr/bin/env bash
printf '%s\n' "$MOCK_EXPECTED_STAMP"
EOF

cat > "$tmpdir/bin/gh" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
pattern=""
output=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    -p) pattern="$2"; shift 2 ;;
    -O) output="$2"; shift 2 ;;
    *) shift ;;
  esac
done
if [ "$pattern" = kokuban_ci_core ]; then
  cp "$MOCK_BINARY" "$output"
else
  case "$MOCK_CHECKSUM_MODE" in
    missing) exit 1 ;;
    corrupt) printf '%064d  kokuban_ci_core\n' 0 > "$output" ;;
    malformed) printf 'not-a-checksum\n' > "$output" ;;
    valid) sha256sum "$MOCK_BINARY" | awk '{print $1 "  kokuban_ci_core"}' > "$output" ;;
    *) exit 1 ;;
  esac
fi
EOF

cat > "$tmpdir/bin/sleep" <<'EOF'
#!/usr/bin/env bash
printf 'slept\n' >> "$MOCK_SLEEP_LOG"
EOF

chmod +x "$tmpdir/bin/"* "$MOCK_BINARY"
export PATH="$tmpdir/bin:$PATH"
export CI_CORE_ATTEMPTS=2 CI_CORE_DELAY=0

run_fetch() {
  bash "$ROOT/scripts/fetch-ci-core.sh" "$tmpdir/output/core" > "$tmpdir/log" 2>&1
}

run_fetch
[ -s "$MOCK_EXEC_MARKER" ]

for mode in corrupt malformed; do
  export MOCK_CHECKSUM_MODE="$mode"
  rm -f "$MOCK_EXEC_MARKER" "$MOCK_SLEEP_LOG"
  if run_fetch; then
    echo "Expected $mode checksum to reject the binary" >&2
    exit 1
  fi
  [ ! -e "$MOCK_EXEC_MARKER" ]
  [ "$(wc -l < "$MOCK_SLEEP_LOG")" -eq "$CI_CORE_ATTEMPTS" ]
done

export MOCK_CHECKSUM_MODE=valid
export MOCK_STAMP="bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
if run_fetch; then
  echo "Expected a stale core to be rejected despite its valid checksum" >&2
  exit 1
fi

export MOCK_CHECKSUM_MODE=missing
export MOCK_STAMP="$MOCK_EXPECTED_STAMP"
run_fetch
grep -q 'checksum' "$tmpdir/log"

echo "OK: CI Core checksum, stamp and retry checks passed."
