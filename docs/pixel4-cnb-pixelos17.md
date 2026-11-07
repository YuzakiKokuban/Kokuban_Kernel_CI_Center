# Pixel 4 PixelOS 17 CNB kernel

The bakasu source now matches the PixelOS flame 20261107-1111 kernel baseline: CNB 4.14.357, BakaSU 603931b9 and SUSFS 2.3.0, with LTO/CFI retained and WireShark naming.

Build Kernel: project floral_sm8150, branch bakasu, build mode default/bakasu, other options at defaults. For this source, the workflow delegates to the kernel-owned ci/build.py before the legacy setup code runs. BakaSU setup overrides, tuning fragments, localversion/LTO overrides, BBG and disabling SUSFS are rejected. The legacy Hybrid Mount input does not inject code into this reviewed baseline. Trees without ci/pixelos17.json keep the legacy pipeline.

The builder downloads the same pinned AOSP clang-r596125 used by the ROM. Android DTC and mkdtimg are pinned and SHA256 verified. Kernel artifacts include Image.lz4, concatenated base DTBs, dtbo.img, config, symbols, provenance and matching external modules. The ZIP is NOT a recovery-flashable AnyKernel installer: modules must be packaged into the matching ROM vendor layout.

BakaSU is vendored because its isolated UAPI header fix exists in a local commit. Source provenance and its original version are retained. The upstream updater skips this explicitly pinned snapshot; changes to its driver/hooks must be reviewed together before replacing it.

Run python3 scripts/test-pixel4-cnb.py for delegation tests. The actual kernel builder runs ci/test_build.py as part of validation.
