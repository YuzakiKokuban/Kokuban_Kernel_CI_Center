#!/usr/bin/env python3
"""Behavior checks for the optional BakaSU SUSFS integration."""
import importlib.util
from pathlib import Path
import tempfile
import difflib
import json
import unittest

spec = importlib.util.spec_from_file_location('susfs', Path(__file__).with_name('susfs.py'))
susfs = importlib.util.module_from_spec(spec)
spec.loader.exec_module(susfs)


class SusfsTests(unittest.TestCase):
    def test_reviewed_read_hooks_match_bakasu_three_argument_api(self):
        for patch in (Path(__file__).resolve().parent.parent / 'configs/susfs').glob('*.patch'):
            with self.subTest(project=patch.stem):
                content = patch.read_text()
                self.assertNotIn('void ksu_handle_sys_read(unsigned int fd)', content)
                self.assertIn('ksu_handle_sys_read(fd, &buf, &count)', content)

    def test_hook_modes_are_exclusive(self):
        with tempfile.TemporaryDirectory() as tmp:
            cfg = Path(tmp) / 'defconfig'
            cfg.write_text('CONFIG_KSU_MANUAL_HOOK=y\nCONFIG_KSU_TRACEPOINT_HOOK=y\nCONFIG_KEYS=y\n')
            susfs.configure(cfg)
            susfs.check_config(cfg)
            self.assertIn('CONFIG_KEYS=y', cfg.read_text())
            original = cfg.read_text()
            susfs.configure(cfg)
            self.assertEqual(original, cfg.read_text())
            cfg.write_text(original + 'CONFIG_KSU_MANUAL_HOOK=y\n')
            with self.assertRaisesRegex(ValueError, 'KSU_MANUAL_HOOK'):
                susfs.check_config(cfg)

    def test_dropped_susfs_is_rejected(self):
        with tempfile.TemporaryDirectory() as tmp:
            cfg = Path(tmp) / '.config'
            cfg.write_text('CONFIG_KSU=y\n# CONFIG_KSU_SUSFS is not set\n')
            with self.assertRaisesRegex(ValueError, 'KSU_SUSFS'):
                susfs.check_config(cfg)

    def test_failed_patch_does_not_modify_source(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / 'file').write_text('different\n')
            patch = root / 'change.patch'
            patch.write_text('--- a/file\n+++ b/file\n@@ -1 +1 @@\n-before\n+after\n')
            with self.assertRaises(ValueError):
                susfs.apply_checked_patch(root, patch)
            self.assertEqual((root / 'file').read_text(), 'different\n')
            self.assertFalse((root / 'file.rej').exists())

    def test_no_outside_paths_in_patch(self):
        for path in ('../outside', '/etc/passwd', 'fs/../../outside'):
            with self.subTest(path=path), self.assertRaises(ValueError):
                susfs.patch_paths(f'--- a/{path}\n+++ b/{path}\n')

    def test_checks_real_calls_and_rejects_legacy_hooks(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            for path, names in susfs.REQUIRED_HOOKS.items():
                file = root / path
                file.parent.mkdir(parents=True, exist_ok=True)
                file.write_text('\n'.join(f'void f{i}() {{ {name}(); }}' for i, name in enumerate(names)))
            susfs.check_hooks(root)
            file = root / 'fs/exec.c'
            file.write_text('extern int ksu_handle_execveat(void);\n')
            with self.assertRaisesRegex(ValueError, 'ksu_handle_execveat'):
                susfs.check_hooks(root)

    def test_integrates_a_patchset_transactionally_and_repeats(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            source, patchset = root / 'source', root / 'upstream'
            cfg = source / 'arch/arm64/configs/test_defconfig'
            cfg.parent.mkdir(parents=True)
            cfg.write_text('CONFIG_KEYS=y\nCONFIG_KSU_MANUAL_HOOK=y\n')
            (source / 'Makefile').write_text('VERSION = 6\nPATCHLEVEL = 12\n')
            diff = []
            for relative, names in susfs.REQUIRED_HOOKS.items():
                file = source / relative
                file.parent.mkdir(parents=True, exist_ok=True)
                file.write_text('/* original */\n')
                changed = '#ifdef CONFIG_KSU_SUSFS\n' + ''.join(
                    f'void f{i}() {{ {name}(); }}\n' for i, name in enumerate(names)) + '#endif\n'
                diff.extend(difflib.unified_diff(file.read_text().splitlines(True), changed.splitlines(True),
                                               fromfile=f'a/{relative}', tofile=f'b/{relative}'))
            patch = patchset / 'kernel_patches/50_test-6.12.patch'
            patch.parent.mkdir(parents=True)
            patch.write_text(''.join(diff))
            (patchset / 'kernel_patches/fs').mkdir()
            (patchset / 'kernel_patches/fs/susfs.c').write_text('/* payload */\n')
            (root / 'configs').mkdir()
            (root / 'configs/projects.json').write_text(json.dumps({'test': {'susfs': {
                'repo': 'https://example.com/susfs.git', 'revision': 'a' * 40,
                'patch': 'kernel_patches/50_test-6.12.patch'}}}))
            original_root = susfs.ROOT
            susfs.ROOT = root
            try:
                susfs.integrate(source, 'test', 'test_defconfig', patchset=patchset)
                self.assertTrue((source / '.kokuban-susfs.json').exists())
                self.assertEqual((source / 'fs/susfs.c').read_text(), '/* payload */\n')
                susfs.check_hooks(source)
                susfs.check_config(cfg)
                # Repeated integration does not duplicate changes or need a network fetch.
                before = cfg.read_text()
                susfs.integrate(source, 'test', 'test_defconfig', offline=True)
                self.assertEqual(before, cfg.read_text())
            finally:
                susfs.ROOT = original_root


if __name__ == '__main__':
    unittest.main()
