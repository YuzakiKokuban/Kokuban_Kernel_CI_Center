# Upstream Wave Reliability Implementation Plan

**Goal:** Retry failed waves, detect newly added projects, and build the exact detected upstream SHA with project SUSFS defaults.

**Architecture:** Detection is read-only. Track successful project/variant pairs after the entire serialized update/build wave succeeds. Legacy global records do not suppress project backfill.

**Files:** `ci_core_rs/src/main.rs`, new `ci_core_rs/src/watcher.rs`, `.github/workflows/upstream-watcher.yml`, `.github/workflows/reusable-update-kernel-source.yml`.

- [x] Add failing watcher unit tests for legacy migration/new projects, failed-wave retry, successful acknowledgement and immutable SHA validation.
- [x] Implement pure matrix planning and acknowledgement, wire CLI watch/ack and pinned setup.
- [x] Update workflow: serialized wave concurrency, final success acknowledgement, full SHA build pin and SUSFS auto.
- [x] Repair legacy reusable updater to use full resolved upstream SHA.
- [x] Run focused and complete Rust tests in WSL; validate workflow YAML and git diff.

No commits or remote operations in this delegated task.
