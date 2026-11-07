#!/usr/bin/env bash
# CNB already contains the exact ROM driver/hooks; legacy setup scripts must not replace them.
set -euo pipefail
[[ "${PROJECT:-}" == floral_sm8150 && "${VARIANT:-}" == bakasu ]] || {
    echo 'CNB Pixel 4 requires floral_sm8150 / bakasu' >&2; exit 1;
}
[[ "${APPLY_SUSFS:-auto}" != false ]] || { echo 'CNB requires its integrated SUSFS' >&2; exit 1; }
[[ "${APPLY_BBG:-false}" != true ]] || { echo 'CNB does not support Baseband Guard' >&2; exit 1; }
for name in BAKASU_SETUP_ARG KCONFIG_FRAGMENT LTO_OVERRIDE CUSTOM_LOCALVERSION; do
    [[ -z "${!name:-}" ]] || { echo "CNB uses the pinned ROM configuration; $name cannot override it" >&2; exit 1; }
done
# APPLY_HYBRIDMOUNT belongs to the legacy pipeline. Preserve this ROM baseline's SUSFS hooks.
echo 'Building the pinned CNB ROM configuration; legacy overlays and driver setup are skipped.'
python3 kernel_source/ci/build.py --source kernel_source --out kernel_source/out --artifact-dir .
if [[ "${DO_RELEASE:-false}" == true ]]; then
    shopt -s nullglob
    packages=(Pixel4_Kernel-*-images-modules.zip)
    [[ ${#packages[@]} -eq 1 ]] || { echo 'Expected exactly one current kernel package' >&2; exit 1; }
    release_tag="Pixel4-CNB-$(date -u +%Y%m%d-%H%M%S)"
    gh release create "$release_tag" "${packages[0]}" \
        --repo YuzakiKokuban/android_kernel_google_msm-4.14 \
        --title "Pixel 4 CNB BakaSU / SUSFS" \
        --notes 'PixelOS 17 kernel images and matching modules. This ZIP is a build artifact, not a recovery-flashable AnyKernel installer.'
fi
