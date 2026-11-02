# SUSFS integration — 2026-11-02

The center supports two exclusive BakaSU hook modes: the existing manual hooks and optional SUSFS inline hooks. LKM cannot enable SUSFS. OP15 defaults to SUSFS in BakaSU mode; existing projects remain opt-in.

## Source and patch provenance

- OP15 source: `cctv18/android_gki_kernel_common`, branch `android16-6.12-2026-03`, commit `58ee67741556c83c523f48518284c4a6b1ef31d6` (Linux 6.12.69).
- Imported snapshot: `YuzakiKokuban/android_kernel_oneplus_sm8850`. GitHub refused a second fork in the existing Xiaomi fork network; the user explicitly authorized an independent repository. The import commit records upstream provenance rather than copying the whole Linux history.
- 5.15: `ShirkNeko/susfs4ksu`, `gki-android13-5.15-dev`, commit `6a701d403cf8930d9b8142cfb06509d0a5c3ddab`.
- 6.1: `cctv18/susfs4oki`, `oki-android14-6.1`, commit `fad7721e8d2e724dbe8c1f0fdcc22881fd9d5077`.
- 6.6: same repository, `oki-android15-6.6`, commit `3f6aae536724af0df3dfa15f932034d14a5dbc5c`.
- 6.12: same repository, `oki-android16-6.12`, commit `5f236a7b573c5a21ad0e5871ee96b63b89486596`.

The device patches under `configs/susfs` adapt upstream patch context to the actual source snapshots. They retain vendor code, including GKI VMA padding hooks. Runtime application uses `--fuzz=0`; failed hunks are not ignored. Known manual-hook patches are reversed in a temporary staging tree before SUSFS is applied. The actual source is updated only after staged patch/configuration and call-site checks pass. The regular manual-hook path continues to reject mixed or stale SUSFS trees.

Patch implementation files and headers come from the pinned upstream revision. SUSFS can be disabled using `--no-susfs`, or explicitly enabled on a configured BakaSU project with `--with-susfs`. An offline first build requires the patchset to have been cached previously.

## Validation boundary

Ten source snapshots (all configured 5.15, 6.1, 6.6 and 6.12 projects, including OP15) passed strict source patch and hook-call checks in WSL. Exact refs are recorded in `susfs-source-validation.json`. These checks do not establish device compatibility, stock module ABI compatibility, or runtime behavior. Full builds of every device are not implied by source validation.

OP15 additionally completed a full WSL build with BakaSU commit `516d3e87fbc986ccefdde2c2765f1700612441a9`, SUSFS and Hybrid Mount. The generated kernel reports `6.12.69-android16-Kokuban-Claire-g623c5276587d-4k`. The AnyKernel ZIP passed integrity checks; its Image matches the build output, the display name is correct, and the final configuration enables SUSFS while disabling manual/tracepoint hooks. See `op15-build-validation.json`. No device boot test or stock vendor-module ABI compatibility check was performed.

## Community Linux 4.14 references

The old simonpunk `kernel-4.14` branch reports SUSFS 1.5.5 and is not a suitable drop-in patchset for the current BakaSU inline-hook API. Community 2.3.0 ports do exist:

1. [JackA1ltman/NonGKI_Kernel_Build_2nd — standalone 4.14 patch](https://github.com/JackA1ltman/NonGKI_Kernel_Build_2nd/blob/mainline/Patches/Patch/susfs_patch_to_4.14.patch). The patch embeds SUSFS 2.3.0 implementation/header changes. Its companion [inline-hook script](https://github.com/JackA1ltman/NonGKI_Kernel_Build_2nd/blob/mainline/Patches/susfs_inline_hook_patches.sh) supplies additional BakaSU call sites. The project's support list includes Pixel 4a/sunfish, not Pixel 4/floral.
2. [mzggr0914/android_kernel_a325x_bakasu](https://github.com/mzggr0914/android_kernel_a325x_bakasu): Linux 4.14.357, SUSFS 2.3.0, upstream BakaSU main retained without driver-side modifications. [Integration notes](https://github.com/mzggr0914/android_kernel_a325x_bakasu/blob/common/docs/susfs-integration.md) explain parent-kernel adapters and fixes to mount allocation, KSTAT, SELinux and command dispatch. This is a useful compatibility reference for the center's policy of leaving BakaSU unchanged.
3. [souta-lab/agate-bakasu-susfs](https://github.com/souta-lab/agate-bakasu-susfs), branch `feature/susfs-2.3.4.14-port`: maintainer reports Xiaomi 11T boot/root/SUSFS functional verification on Linux 4.14.336. Its documented driver/checker changes mean it is a secondary reference, not a direct replacement for this center's current BakaSU integration.

Pixel 4 support is not enabled by this change. Its existing built-in Hybrid Mount, WLAN/audio gates and manual hooks must be preserved during a dedicated 4.14 port. Neither MediaTek example nor Pixel 4a support proves compatibility with floral/sm8150.

A read-only `patch --dry-run --fuzz=0` of the JackA1ltman standalone 4.14 patch at commit `f59ce26a196508276e54453d0fe98c5a36b2b51b` against the actual Pixel 4 source at `9bf1df6e86457bf38266d0b1e42e2f49769b65f0` failed only two hunks: the initial include sections of `fs/proc/task_mmu.c` and `mm/memory.c`. The rest matched, often with line-number offsets. This is encouraging evidence for a port, not a complete integration: the existing manual BakaSU hooks still need replacement with the appropriate SUSFS hooks and the result needs compilation and device checks. See `susfs-4.14-community-probe.json`.
