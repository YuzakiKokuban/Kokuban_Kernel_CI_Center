# Pixel 4 SUSFS and upstream wave restoration

**Goal:** Complete Pixel 4's SUSFS 2.3.0 port, restore SUSFS defaults for every BakaSU project, and deploy a retry-safe upstream update/build wave before the next scheduled run.

**Architecture:** Keep device repositories on their validated manual-hook baseline. At build time, apply reviewed SUSFS patches as the exclusive hook mode, preserving Pixel 4's built-in Hybrid Mount, WLAN/audio and image-only packaging checks. Pin BakaSU setup to the detected full commit. Acknowledge per-project upstream progress only after successful updates/builds, so failed waves retry and new devices are included.

**Constraints:** Use local Ubuntu WSL; do not weaken upstream BakaSU checks or modify the BakaSU driver. Preserve existing naming and existing non-SUSFS builds. No device boot claims. Publish tested changes and update the operational central main/core so the next watcher actually uses them.

## Tasks

- [x] Port the public community 4.14 patch and SUSFS hooks to actual floral source; retain strict patch/config checks.
- [x] Add meaningful regression tests for Pixel 4 hook-mode configuration and all-project default resolution, then implement build/config changes.
- [x] Complete a full Pixel 4 WSL build and image-only AK3 verification with SUSFS and built-in Hybrid Mount/WLAN/audio checks.
- [x] Repair detection, immutable upstream pinning, success acknowledgement and retry behavior; test new-project and failed-wave cases.
- [x] Validate current source integration for every project and run center Rust/Python/shell/workflow checks.
- [ ] Obtain independent review, publish/merge tested central changes and verify the published core used by scheduled work.
- [ ] Exercise the watcher safely, preserving pending work until successful builds and avoiding an unrequested mass release wave.
