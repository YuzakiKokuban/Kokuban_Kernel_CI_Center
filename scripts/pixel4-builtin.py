#!/usr/bin/env python3
"""Gate the Pixel 4 image-only AK3 package on its built-in drivers."""
import argparse
from pathlib import Path

REQUIRED = {"wlan", "msm_11ad_proxy", "incrementalfs"}
# Techpack's private config is outside Kconfig's tristate.conf, so its built-ins
# are not listed in modules.builtin. Verify their linked initcalls instead.
AUDIO_INITCALLS = set("""wcd_pinctrl_driver_init swrm_init audio_native_init
audio_q6_init adsp_loader_init audio_pdr_late_init audio_notifier_init
apr_driver_init wcd9xxx_core_init wcd9xxx_soc_init wsa881x_codec_init
wcd_spi_driver_init cs35l36_i2c_driver_init audio_platform_init
sm8150_asoc_machine_driver_init""".split())


def verify(out, anykernel):
    out, anykernel = Path(out), Path(anykernel)
    config = (out / ".config").read_text().splitlines()
    for name in ("CONFIG_QCA_CLD_WLAN", "CONFIG_INCREMENTAL_FS", "CONFIG_MSM_11AD"):
        if name + "=y" not in config:
            raise ValueError(f"{name} must be built in")
    if (out / "modules.order").read_text().strip():
        raise ValueError("Pixel 4 build still produces external modules")
    actual = {Path(line).stem for line in (out / "modules.builtin").read_text().splitlines()}
    missing = REQUIRED - actual
    if missing:
        raise ValueError("missing built-in drivers: " + ", ".join(sorted(missing)))
    symbols = {line.split()[-1] for line in (out / "System.map").read_text().splitlines() if line.split()}
    missing_init = {name for name in AUDIO_INITCALLS if f"__initcall_{name}6" not in symbols}
    if missing_init:
        raise ValueError("missing built-in audio initcalls: " + ", ".join(sorted(missing_init)))
    if list(anykernel.rglob("*.ko")) or (anykernel / "kokuban-modules").exists():
        raise ValueError("external module payload in image-only AK3")
    if "do.modules=0" not in (anykernel / "anykernel.sh").read_text().splitlines():
        raise ValueError("image-only AK3 requires do.modules=0")
    image = anykernel / "Image.lz4"
    if not image.is_file() or not image.stat().st_size:
        raise ValueError("missing Image.lz4")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--anykernel", type=Path, required=True)
    args = parser.parse_args()
    verify(args.out, args.anykernel)
    print("Pixel 4 image-only AK3 verified: WLAN, audio and Incremental FS built in; no external modules")


if __name__ == "__main__":
    main()
