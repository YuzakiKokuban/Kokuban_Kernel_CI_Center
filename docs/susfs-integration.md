# SUSFS integration — 2026-11-02

The center supports two exclusive BakaSU hook modes: manual hooks and SUSFS inline hooks. LKM cannot enable SUSFS. Every configured device now defaults to SUSFS in BakaSU mode, including Pixel 4; `--no-susfs` retains the manual-hook build path.

## Source and patch provenance

- OP15 source: `cctv18/android_gki_kernel_common`, branch `android16-6.12-2026-03`, commit `58ee67741556c83c523f48518284c4a6b1ef31d6` (Linux 6.12.69).
- Imported snapshot: `YuzakiKokuban/android_kernel_oneplus_sm8850`. GitHub refused a second fork in the existing Xiaomi fork network; the user explicitly authorized an independent repository. The import commit records upstream provenance rather than copying the whole Linux history.
- 5.15: `ShirkNeko/susfs4ksu`, `gki-android13-5.15-dev`, commit `6a701d403cf8930d9b8142cfb06509d0a5c3ddab`.
- 6.1: `cctv18/susfs4oki`, `oki-android14-6.1`, commit `fad7721e8d2e724dbe8c1f0fdcc22881fd9d5077`.
- 6.6: same repository, `oki-android15-6.6`, commit `3f6aae536724af0df3dfa15f932034d14a5dbc5c`.
- 6.12: same repository, `oki-android16-6.12`, commit `5f236a7b573c5a21ad0e5871ee96b63b89486596`.
- Pixel 4 / 4.14: `JackA1ltman/NonGKI_Kernel_Build_2nd`, commit `f59ce26a196508276e54453d0fe98c5a36b2b51b`, `Patches/Patch/susfs_patch_to_4.14.patch`, adapted to floral source `9bf1df6e86457bf38266d0b1e42e2f49769b65f0`.

The device patches under `configs/susfs` adapt upstream patch context to the actual source snapshots. They retain vendor code, including GKI VMA padding hooks. Runtime application uses `--fuzz=0`; failed hunks are not ignored. Known manual-hook patches are reversed in a temporary staging tree before SUSFS is applied. The actual source is updated only after staged patch/configuration and call-site checks pass. The regular manual-hook path continues to reject mixed or stale SUSFS trees.

Patch implementation files and headers come from the pinned upstream revision. SUSFS can be disabled using `--no-susfs`, or explicitly enabled on a configured BakaSU project with `--with-susfs`. An offline first build requires the patchset to have been cached previously.

## Validation boundary

Eleven current source snapshots (all configured 4.14, 5.15, 6.1, 6.6 and 6.12 projects) passed strict source patch and hook-call checks in WSL, including BakaSU's upstream inline checker. Exact refs are recorded in `susfs-source-validation.json`. These checks do not establish device compatibility, stock module ABI compatibility, or runtime behavior. Full builds of every device are not implied by source validation.

OP15 additionally completed a full WSL build with BakaSU commit `516d3e87fbc986ccefdde2c2765f1700612441a9`, SUSFS and Hybrid Mount. The generated kernel reports `6.12.69-android16-Kokuban-Claire-g623c5276587d-4k`. The AnyKernel ZIP passed integrity checks; its Image matches the build output, the display name is correct, and the final configuration enables SUSFS while disabling manual/tracepoint hooks. See `op15-build-validation.json`. No device boot test or stock vendor-module ABI compatibility check was performed.

Pixel 4 also completed a full WSL build with the same pinned BakaSU commit, SUSFS 2.3.0 and its built-in Hybrid Mount. The final image-only AK3 package passed WLAN, audio initcall and Incremental FS checks, contains no external modules, and configures flame/coral. See `pixel4-build-validation.json` for the exact release and checksums. The packaging verifier now resolves its script from the central root, including when the build directory is outside the center checkout. No device boot or runtime SUSFS test has been performed.

## Community Linux 4.14 references

The old simonpunk `kernel-4.14` branch reports SUSFS 1.5.5 and is not a suitable drop-in patchset for the current BakaSU inline-hook API. Community 2.3.0 ports do exist:

1. [JackA1ltman/NonGKI_Kernel_Build_2nd — standalone 4.14 patch](https://github.com/JackA1ltman/NonGKI_Kernel_Build_2nd/blob/mainline/Patches/Patch/susfs_patch_to_4.14.patch). The patch embeds SUSFS 2.3.0 implementation/header changes. Its companion [inline-hook script](https://github.com/JackA1ltman/NonGKI_Kernel_Build_2nd/blob/mainline/Patches/susfs_inline_hook_patches.sh) supplies additional BakaSU call sites. The project's support list includes Pixel 4a/sunfish, not Pixel 4/floral.
2. [mzggr0914/android_kernel_a325x_bakasu](https://github.com/mzggr0914/android_kernel_a325x_bakasu): Linux 4.14.357, SUSFS 2.3.0, upstream BakaSU main retained without driver-side modifications. [Integration notes](https://github.com/mzggr0914/android_kernel_a325x_bakasu/blob/common/docs/susfs-integration.md) explain parent-kernel adapters and fixes to mount allocation, KSTAT, SELinux and command dispatch. This is a useful compatibility reference for the center's policy of leaving BakaSU unchanged.
3. [souta-lab/agate-bakasu-susfs](https://github.com/souta-lab/agate-bakasu-susfs), branch `feature/susfs-2.3.4.14-port`: maintainer reports Xiaomi 11T boot/root/SUSFS functional verification on Linux 4.14.336. Its documented driver/checker changes mean it is a secondary reference, not a direct replacement for this center's current BakaSU integration.

Pixel 4 now has a dedicated strict patch and hook-mode configuration. Existing manual hooks are reversed before applying SUSFS; the opt-out path still uses the validated manual baseline. The port preserves built-in Hybrid Mount, WLAN/audio regression and initcall gates, and image-only AK3 packaging. Neither MediaTek example nor Pixel 4a support is used as proof of floral device compatibility.

A read-only `patch --dry-run --fuzz=0` of the JackA1ltman standalone 4.14 patch at commit `f59ce26a196508276e54453d0fe98c5a36b2b51b` against the actual Pixel 4 source at `9bf1df6e86457bf38266d0b1e42e2f49769b65f0` failed only two hunks: the initial include sections of `fs/proc/task_mmu.c` and `mm/memory.c`. The rest matched, often with line-number offsets. This is encouraging evidence for a port, not a complete integration: the existing manual BakaSU hooks still need replacement with the appropriate SUSFS hooks and the result needs compilation and device checks. See `susfs-4.14-community-probe.json`.

That probe records the initial investigation. The completed source adaptation adds the current BakaSU filename-based stat/access hooks, balanced exec filename references, three-argument read hook, fstat/initrc bridge, setresuid, input and reboot hooks. Independent review also found and corrected three inherited issues: borrowed mount IDs are marked at allocation time, fresh clones clear inherited borrowed-ID ownership, and private SUSFS KSTAT selectors remain local instead of leaking into the public statx result mask. `scripts/test-pixel4-susfs.py` compiles the actual C functions/snippets from the reviewed patch and exercises allocation cleanup and 32 KSTAT combinations. It also runs against an optional real source checkout. This is host behavior evidence, not device boot evidence.

## Upstream watcher rollout

Detection is read-only and compares per-project success records. The source update and build use the same detected full BakaSU SHA and verify the installed driver commit. A failed wave is not acknowledged, and new devices are backfilled even when an older global upstream record already knows the SHA. The serialized update/build workflow acknowledges the wave only after both matrices succeed.

`verify_only=true` checks the live pending matrix, source repository write permissions and reachable BakaSU branches, without source updates, builds, releases or tracker acknowledgement. This permits a real GitHub credential and published-core check after deployment.

The 2026-11-02 failed wave reached the Razr Fold ABI checks and passed the golden baseline (16,516 symbols, zero CRC mismatch/missing) before failing on nonexistent `Kokuban/abi/stock_module_imports.tsv`. The user confirmed that no such stock manifest was ever available and explicitly authorized skipping that optional consumer check. Only its project configuration reference was removed; the golden baseline, zero-mismatch/missing thresholds and fixed symbol CRC gates remain enabled.

Deployed on 2026-11-03 through [PR #59](https://github.com/YuzakiKokuban/Kokuban_Kernel_CI_Center/pull/59). Main core and local-mode workflows passed. The [live read-only watcher](https://github.com/YuzakiKokuban/Kokuban_Kernel_CI_Center/actions/runs/41587281308) verified the matching published core stamp, all 11 pending BakaSU projects, and each source repository's write permission/branch. Update/build/acknowledgement jobs were skipped and the tracker remained unchanged. Exact evidence: `upstream-watcher-rollout-validation.json`. This validates detection and access rather than execution of the next complete all-device build wave.

The [Pixel 4 test prerelease](https://github.com/YuzakiKokuban/android_kernel_google_msm-4.14/releases/tag/pixel4-4.14-susfs-test-20261103) includes the package, actual source overlay, final config, symbol exports, successful build log, report and SHA256SUMS. All eight uploaded asset digests were checked against local files.
