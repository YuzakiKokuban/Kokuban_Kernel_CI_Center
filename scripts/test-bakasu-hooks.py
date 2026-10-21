#!/usr/bin/env python3
"""Regression checks for the pre-build BakaSU hook gate."""
import importlib.util
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location('hooks', Path(__file__).with_name('bakasu-hooks.py'))
hooks = importlib.util.module_from_spec(spec)
spec.loader.exec_module(hooks)

class HookGateTests(unittest.TestCase):
    def test_declarations_and_comments_do_not_satisfy_a_hook(self):
        source = 'extern int ksu_handle_execveat(void *p);\n/* ksu_handle_execveat(&fd); */'
        self.assertFalse(hooks.has_call(source, 'ksu_handle_execveat'))
        self.assertTrue(hooks.has_call(source + '\nvoid f(void) { ksu_handle_execveat(&fd); }', 'ksu_handle_execveat'))

    def test_modern_kernels_require_explicit_setuid_and_read(self):
        self.assertEqual(hooks.auto_lsm((6, 6)), 'y')
        self.assertEqual(hooks.auto_lsm((6, 12)), 'n')
        self.assertEqual(hooks.auto_lsm((4, 14)), 'y')

    def test_config_rejects_the_wrong_hook_method(self):
        with tempfile.TemporaryDirectory() as tmp:
            cfg = Path(tmp) / '.config'
            cfg.write_text('CONFIG_KSU=y\nCONFIG_KSU_TRACEPOINT_HOOK=y\n')
            with self.assertRaisesRegex(ValueError, 'KSU_MANUAL_HOOK'):
                hooks.check_config(cfg, (6, 12))

    def test_retired_config_is_removed_and_rejected_if_reenabled(self):
        with tempfile.TemporaryDirectory() as tmp:
            cfg = Path(tmp) / 'defconfig'
            cfg.write_text('# KernelSU - SUSFS\nCONFIG_KSU_SUSFS=y\nCONFIG_KSU_SUSFS_SUS_PATH=y\n# end of KernelSU - SUSFS\nCONFIG_KEYS=y\n')
            hooks.configure(cfg, (6, 12))
            self.assertNotIn('SUSFS', cfg.read_text())
            self.assertIn('CONFIG_KEYS=y', cfg.read_text())
            hooks.check_config(cfg, (6, 12))
            cfg.write_text(cfg.read_text() + 'CONFIG_KSU_SUSFS=y\n')
            with self.assertRaisesRegex(ValueError, 'removed feature'):
                hooks.check_config(cfg, (6, 12))

    def test_every_required_config_is_enforced(self):
        for version in ((4, 14), (5, 15), (6, 1), (6, 6), (6, 12)):
            with self.subTest(version=version), tempfile.TemporaryDirectory() as tmp:
                cfg = Path(tmp) / '.config'
                cfg.write_text('')
                hooks.configure(cfg, version)
                original = cfg.read_text()
                hooks.configure(cfg, version)
                self.assertEqual(cfg.read_text(), original)
                for key, value in hooks.required_config(version).items():
                    replacement = f'# {key} is not set' if value == 'y' else f'{key}=y'
                    lines = [line for line in original.splitlines() if key not in line]
                    cfg.write_text('\n'.join(lines + [replacement]) + '\n')
                    with self.assertRaises(ValueError):
                        hooks.check_config(cfg, version)


if __name__ == '__main__':
    unittest.main()
