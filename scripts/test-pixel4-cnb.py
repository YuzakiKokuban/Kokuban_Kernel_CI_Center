#!/usr/bin/env python3
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

SCRIPT = Path(__file__).with_name('build-pixel4-cnb.sh').resolve()

class Delegate(unittest.TestCase):
    def run_build(self, **overrides):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            (root/'kernel_source/ci').mkdir(parents=True)
            (root/'kernel_source/ci/pixelos17.json').write_text('{}')
            (root/'kernel_source/ci/build.py').write_text('from pathlib import Path\nPath("invoked").write_text("yes")\n')
            env = dict(os.environ, PROJECT='floral_sm8150', VARIANT='bakasu', DO_RELEASE='false', APPLY_BBG='false', APPLY_SUSFS='auto')
            env.update(overrides)
            p = subprocess.run(['bash',str(SCRIPT)],cwd=root,env=env,capture_output=True,text=True)
            return p.returncode,(root/'invoked').exists(),p.stdout+p.stderr

    def test_current_kernel_delegates_to_its_builder(self):
        status, invoked, log = self.run_build()
        self.assertEqual(status,0,log)
        self.assertTrue(invoked)

    def test_dynamic_driver_replacement_is_rejected(self):
        status, invoked, log = self.run_build(BAKASU_SETUP_ARG='main')
        self.assertNotEqual(status,0)
        self.assertFalse(invoked)
        self.assertIn('pinned',log)

    def test_susfs_disable_is_rejected(self):
        status, invoked, log = self.run_build(APPLY_SUSFS='false')
        self.assertNotEqual(status,0)
        self.assertFalse(invoked)
        self.assertIn('SUSFS',log)

if __name__ == '__main__': unittest.main()
