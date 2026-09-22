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
  if gh release download ci-core-latest -p "kokuban_ci_core" --clobber -O "$output"; then
    chmod +x "$output" 2>/dev/null || true
    # A binary that cannot even run is not the same finding as a binary with the wrong
    # stamp, and reporting it as a plain mismatch is how the PATH bug above stayed
    # invisible. Surface the reason; never let it read as 'just stale'.
    if ! actual="$("$output" stamp 2>/dev/null)"; then
      why="$("$output" stamp 2>&1 | head -n 1 || true)"
      actual=""
      echo "::warning::released core could not report its stamp (${why:-no output})"
    fi
    actual="${actual//[[:space:]]/}"
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
