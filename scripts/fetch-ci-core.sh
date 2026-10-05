#!/usr/bin/env bash
# Download the published CI Core, refusing to hand back a binary that does not match
# this tree.
#
# `ci-core-latest` is a moving release tag, and every workflow downloads whatever
# happens to be there. When a run starts while a new core is still uploading, it
# executes the *previous* core -- and any check the new core adds is silently absent
# rather than failing. A gate that never runs is indistinguishable from a gate that
# passed, which is how the consumer-side ABI check went missing from a build that was
# supposed to run it.
#
# The binary carries a content stamp of `ci_core_rs` (see ci_core_rs/build.rs), so this
# script waits for the released core to match the tree being driven and fails loudly if
# it never does. A core predating the stamp command cannot prove anything and is treated
# as unmatched.
set -euo pipefail

output="${1:-kokuban_ci_core}"
attempts="${CI_CORE_ATTEMPTS:-30}"
delay="${CI_CORE_DELAY:-20}"

# A bare name would be a PATH lookup, and bash never searches the current directory: the
# invocation below would fail with "command not found" while the download succeeded. That
# is not hypothetical, it is how every attempt of the first run of this guard reported a
# perfectly good binary as 'unavailable'.
case "$output" in
  /* | ./* | ../*) ;;
  *) output="./$output" ;;
esac

expected="$(git rev-parse HEAD:ci_core_rs 2>/dev/null || echo unknown)"
if [ "$expected" = unknown ]; then
  echo "::error::cannot determine the ci_core_rs tree stamp; run this from the repository root"
  exit 1
fi

# Must be initialised: with `set -u`, a failed download is otherwise reported as an
# unbound-variable crash instead of the mismatch it is.
actual=""

for attempt in $(seq 1 "$attempts"); do
  actual=""
  if gh release download ci-core-latest -p "kokuban_ci_core" --clobber -O "$output"; then
    checksum_valid=true
    if gh release download ci-core-latest -p "kokuban_ci_core.sha256" --clobber -O "$output.sha256" 2>/dev/null; then
      expected_hash="$(awk '$2 == "kokuban_ci_core" { print $1; exit }' "$output.sha256")"
      downloaded_hash="$(sha256sum "$output" | cut -d' ' -f1)"
      if [ "$downloaded_hash" != "$expected_hash" ]; then
        checksum_valid=false
        echo "::warning::CI Core checksum mismatch; refusing to execute it and waiting for a matching release"
      fi
    else
      # Older releases did not publish a checksum. They must still pass the stamp gate.
      echo "::warning::CI Core checksum asset unavailable; the source stamp is still required"
    fi
    if "$checksum_valid"; then
      chmod +x "$output" 2>/dev/null || true
      # Report binaries that cannot run separately from binaries with an old stamp.
      if ! actual="$("$output" stamp 2>/dev/null)"; then
        why="$("$output" stamp 2>&1 | head -n 1 || true)"
        actual=""
        echo "::warning::released core could not report its stamp (${why:-no output})"
      fi
      actual="${actual//[[:space:]]/}"
    fi
  fi
  if [ "$actual" = "$expected" ]; then
    echo "CI Core stamp matches the tree being driven ($expected)"
    exit 0
  fi
  echo "Attempt $attempt/$attempts: released core is '${actual:-unavailable}', expected '$expected'"
  sleep "$delay"
done

echo "::error::released CI Core does not match ci_core_rs (binary '${actual:-unavailable}', tree '$expected')"
exit 1
