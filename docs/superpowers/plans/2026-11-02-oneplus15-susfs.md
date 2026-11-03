# OnePlus 15 and SUSFS implementation plan

> Execute inline in WSL, using test-driven development for runtime changes.

**Goal:** Add `oneplus15_sm8850` at Linux 6.12.69 and restore optional SUSFS builds across the center.

**Architecture:** Import cctv18's Android 16 GKI branch into the user-authorized independent `YuzakiKokuban/android_kernel_oneplus_sm8850` repository. Reuse the GKI 6.12 pipeline. Select exactly one BakaSU hook mode, manual or SUSFS, with project-specific upstream patch metadata and fail-fast patch/config checks.

**Constraints:** Version suffix `-android16-Kokuban-Claire`; ZIP prefix `OP15`; AnyKernel key `oneplus15`; kernel string `OP15-Kokuban-Kernel`. Run tools and validation in local Ubuntu WSL. Preserve other device source repositories. Do not publish central changes or trigger release workflows without instruction.

## Tasks

- [x] Import the upstream 6.12.69 source snapshot, record its upstream commit, and prepare main/bakasu branches in the new remote repository.
- [x] Add tests for SUSFS hook-mode selection, exact source compatibility, patch failures, unsupported projects, CLI options, and isolation of cached build variants.
- [x] Implement a SUSFS integration helper and project metadata; preserve existing manual-hook behavior when disabled. Reject incompatible patchsets rather than suppressing failures.
- [x] Restore CLI/settings/workflow propagation, local plan/build metadata, and artifact feature suffixes; add OP15 configuration and documentation.
- [x] Run Rust, Python, shell, workflow/config checks in WSL; validate integration against real source and complete the OP15 build. Report device and full-build limits accurately.

## Review focus

- SUSFS is a BakaSU hook choice, not an extra toggle on top of manual hooks.
- LKM builds cannot enable SUSFS; default resolution must account for build mode.
- Existing source patches need reversible handling before changing hook mode.
- Upstream patchsets differ across 4.14, 5.15, 6.1, 6.6 and 6.12.
- Cache keys and source resets must prevent feature state leaking between builds.

## Progress

- Confirmed WSL Ubuntu-26.10, authenticated GitHub CLI, and clean central worktree.
- GitHub refused a second fork in the Xiaomi network; user approved a new independent OnePlus repository. Created remote repository; upstream clone in progress.
- Working central branch: `codex/oneplus15-susfs`.
- Remote main: `623c5276587d4ee595ad22cbd30e72d88cdaa66a`; remote bakasu: `eca7ae52d73834def05ca82332fa3d0db406c7e8`.
- Ten real source snapshots passed strict SUSFS integration and the upstream BakaSU inline checker; source refs recorded in `docs/susfs-source-validation.json`.
- WSL checks: 59 Rust tests, fmt, clippy, 6 SUSFS Python tests, 5 manual-hook tests, 7 Pixel4 tests, CLI wrapper/shell/download tests, and YAML parsing passed. Independent review found no blocking issue.
- Pixel4 SUSFS remains disabled. User requested community 4.14 research; discovered a public 2.3.0 patch and two current kernel references. A read-only probe against floral failed only two include hunks; results recorded separately. Hook-mode adaptation and compilation remain necessary.
- Full OP15 BakaSU + SUSFS + Hybrid Mount WSL build completed, including AnyKernel packaging. ZIP integrity, packaged Image hash, AnyKernel display name and final hook-mode configuration were verified. Evidence: `docs/op15-build-validation.json`; package/log/config/symvers under `build_artifacts/oneplus15_sm8850/`.
- No device boot test or stock vendor-module ABI manifest was available. Central changes remain local on the feature branch; no central push, workflow release or notification was performed.
