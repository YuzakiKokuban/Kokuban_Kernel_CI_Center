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

expected="$(git rev-parse HEAD:ci_core_rs 2>/dev/null || echo unknown)"
actual=""

for attempt in $(seq 1 "$attempts"); do
  if gh release download ci-core-latest -p "kokuban_ci_core" --clobber -O "$output"; then
    chmod +x "$output" 2>/dev/null || true
    actual="$("$output" stamp 2>/dev/null || true)"
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
