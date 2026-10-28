import importlib.util
import sys
import tempfile
import unittest
from pathlib import Path

sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location("builtin", Path(__file__).with_name("pixel4-builtin.py"))
builtin = importlib.util.module_from_spec(spec)
spec.loader.exec_module(builtin)


class BuiltinGateTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.out = self.root / "out"
        self.ak3 = self.root / "ak3"
        self.out.mkdir()
        self.ak3.mkdir()
        (self.out / ".config").write_text("CONFIG_QCA_CLD_WLAN=y\nCONFIG_INCREMENTAL_FS=y\nCONFIG_MSM_11AD=y\n")
        (self.out / "modules.order").write_text("")
        (self.out / "modules.builtin").write_text("".join(f"kernel/driver/{name}.ko\n" for name in builtin.REQUIRED))
        (self.out / "System.map").write_text("".join(f"00000001 d __initcall_{name}6\n" for name in builtin.AUDIO_INITCALLS))
        (self.ak3 / "Image.lz4").write_bytes(b"image")
        (self.ak3 / "anykernel.sh").write_text("do.modules=0\nflash_boot;\n")

    def test_accepts_complete_builtin_image(self):
        builtin.verify(self.out, self.ak3)

    def test_rejects_missing_builtin_audio(self):
        p = self.out / "System.map"
        p.write_text(p.read_text().replace("00000001 d __initcall_sm8150_asoc_machine_driver_init6\n", ""))
        with self.assertRaisesRegex(ValueError, "sm8150_asoc_machine"):
            builtin.verify(self.out, self.ak3)

    def test_rejects_modular_wlan_configuration(self):
        p = self.out / ".config"
        p.write_text(p.read_text().replace("CONFIG_QCA_CLD_WLAN=y", "CONFIG_QCA_CLD_WLAN=m"))
        with self.assertRaisesRegex(ValueError, "CONFIG_QCA_CLD_WLAN"):
            builtin.verify(self.out, self.ak3)

    def test_rejects_remaining_external_modules(self):
        (self.out / "modules.order").write_text("kernel/drivers/softdog.ko\n")
        with self.assertRaisesRegex(ValueError, "external modules"):
            builtin.verify(self.out, self.ak3)

    def test_rejects_module_payload_in_ak3(self):
        (self.ak3 / "old.ko").write_bytes(b"old")
        with self.assertRaisesRegex(ValueError, "payload"):
            builtin.verify(self.out, self.ak3)

    def test_rejects_module_installer(self):
        (self.ak3 / "anykernel.sh").write_text("do.modules=1\nflash_boot;\n")
        with self.assertRaisesRegex(ValueError, "do.modules"):
            builtin.verify(self.out, self.ak3)

    def test_rejects_missing_image(self):
        (self.ak3 / "Image.lz4").unlink()
        with self.assertRaisesRegex(ValueError, "Image.lz4"):
            builtin.verify(self.out, self.ak3)


if __name__ == "__main__":
    unittest.main()
