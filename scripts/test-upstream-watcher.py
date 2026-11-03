#!/usr/bin/env python3
"""Exercise watcher detection/acknowledgement with the real core and local upstream."""
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest


class WatcherTests(unittest.TestCase):
    def test_failed_wave_retries_and_success_acknowledges_new_projects(self):
        core = Path(os.environ['CI_CORE_BIN']).resolve()
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / 'configs').mkdir()
            tools = root / 'bin'
            tools.mkdir()
            sha = 'a' * 40
            git = tools / 'git'
            git.write_text(f'#!/bin/sh\nprintf "%s\\trefs/heads/main\\n" {sha}\n')
            git.chmod(0o755)
            project = {'repo': 'owner/kernel', 'defconfig': 'vendor/test',
                       'localversion_base': '-Test', 'supported_ksu': ['bakasu']}
            projects = {'old': project, 'new': project}
            config = root / 'configs/projects.json'
            config.write_text(json.dumps(projects))
            tracker = root / 'configs/upstream_commits.json'
            tracker.write_text(json.dumps({'bakasu': sha}))
            initial = tracker.read_bytes()
            output = root / 'output'
            env = dict(os.environ, CI_CENTRAL_ROOT=str(root), GITHUB_OUTPUT=str(output),
                       PATH=str(tools) + os.pathsep + os.environ['PATH'])

            def detect():
                output.write_text('')
                subprocess.run([str(core), 'watch'], env=env, check=True)
                result = dict(line.split('=', 1) for line in output.read_text().splitlines())
                return result, json.loads(result['matrix'])

            result, pending = detect()
            self.assertEqual(result['found_updates'], 'true')
            self.assertEqual([entry['project'] for entry in pending], ['new', 'old'])
            self.assertTrue(all(entry['commit_id'] == sha for entry in pending))
            self.assertEqual(tracker.read_bytes(), initial, 'Detection must not acknowledge work')
            self.assertEqual(detect()[1], pending, 'An unacknowledged/failed wave must retry')
            matrix = root / 'wave.json'
            matrix.write_text(json.dumps(pending))
            subprocess.run([str(core), 'ack-upstream', '--matrix-file', str(matrix)],
                           env=env, check=True)
            result, pending = detect()
            self.assertEqual(result['found_updates'], 'false')
            self.assertEqual(pending, [])
            projects['added-later'] = project
            config.write_text(json.dumps(projects))
            self.assertEqual([entry['project'] for entry in detect()[1]], ['added-later'])


if __name__ == '__main__':
    unittest.main()
