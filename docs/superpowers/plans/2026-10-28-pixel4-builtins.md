# Pixel 4 image-only driver repair

**Goal:** Deliver a CI-built AK3 containing Image.lz4 with Wi-Fi, audio and
Incremental FS compiled in, with no external kernel modules or boot loader scripts.

**Architecture:** Use the existing floral static audio configuration and make its
drivers built-in. Register the built-in WLAN driver during kernel initialization;
ICNSS invokes its probe when firmware is ready. Keep the PDR late-subscriber state
replay so an early-ready audio service cannot lose its notification. Preserve the
ROM ramdisk and device trees. BakaSU remains a root feature, not a dependency for
loading Wi-Fi/audio drivers.

## Tasks

- [x] Reproduce and repair PDR late registration; run five extracted-code cases.
- [x] Add a WLAN initialization regression and register its static driver.
- [x] Configure required drivers built-in; disable the unused software watchdog.
- [x] Remove external module installers and add an image-only artifact gate.
- [x] Build and temporarily boot the all-built-in image without the prior loader.
- [ ] Commit/push source and CI changes, run CI and inspect its actual AK3 artifact.

## Verification

Run driver regressions, the existing exec ownership regression, Python artifact
gate tests, Rust format/clippy/tests and a full kernel build. The artifact gate
requires the WLAN/Incremental FS built-in entries and linked audio initcalls,
no entries in modules.order, no .ko payload
and do.modules=0. Verify automatic Wi-Fi and ALSA card registration on the device.
Audible speaker playback requires a later user check; do not imply it was tested.
Do not flash a boot partition during temporary boot validation. Preserve original
boot and userspace backups. Push only verified source/configuration, not private
device logs, APKs, toolchains or temporary outputs.
