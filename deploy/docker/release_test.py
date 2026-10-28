"""Exact source/artifact selection and version policy; no registry mutation."""
import json
import os
from pathlib import Path
import sys
import tarfile
import tempfile
import unittest
from unittest.mock import patch
import release
import smoke
from release_support import DATABASE_IMAGE, validate_manifest, version

REVISION = 'a' * 40
class ReleaseTests(unittest.TestCase):
    def test_image_completion_waits_for_independent_assessment(self):
        details = [{'trialRecords': [{'status': 'COMPUTED', 'error': None}],
                    'final': {'status': status, 'reason': 'Stop before heldout'}}
                   for status in ['STARTED', 'COMPUTED']]
        with patch.object(smoke, 'api', side_effect=details) as api, \
             patch.object(smoke.time, 'sleep') as sleep:
            self.assertIs(smoke.completed(None, 'http://test-only', 1), details[-1])
        self.assertEqual(api.call_count, 2)
        sleep.assert_called_once_with(1)

    def test_image_completion_still_rejects_terminal_noncomputed_results(self):
        for status in ['STOPPED', 'FAILED']:
            detail = {'trialRecords': [{'status': 'COMPUTED', 'error': None}],
                      'final': {'status': status, 'reason': 'Terminal controlled failure'}}
            with self.subTest(status=status), patch.object(smoke, 'api', return_value=detail) as api, \
                 patch.object(smoke.time, 'sleep') as sleep:
                with self.assertRaisesRegex(AssertionError, status):
                    smoke.completed(None, 'http://test-only', 1)
                self.assertEqual(api.call_count, 1)
                sleep.assert_not_called()

    def test_image_smoke_uses_model_only_settings_and_distinct_restore_values(self):
        stored = None
        writes = []
        def api(_opener, _origin, path, body=None, method=None):
            nonlocal stored
            self.assertEqual(path, 'settings')
            if method == 'PATCH':
                writes.append(body); stored = body.copy()
            return {'serviceTier': None, **stored}
        model = {'provider': 'faux', 'modelId': 'controlled'}
        with patch.object(smoke, 'api', side_effect=api):
            empty = smoke.model_settings(None, 'http://test-only')
            original = smoke.model_settings(None, 'http://test-only', model)
            changed = smoke.model_settings(None, 'http://test-only')
        self.assertEqual(writes, [empty, original, changed])
        self.assertEqual(original, {'defaultModel': model, 'thinkingLevel': None, 'serviceTier': None})
        self.assertNotEqual(original, changed)
        self.assertEqual(empty, changed)

    def test_source_ci_must_be_successful_exact_push_not_pr(self):
        def item(id, event, conclusion='success', revision=REVISION):
            return {'id': id, 'event': event, 'head_sha': revision, 'path': '.github/workflows/ci.yml', 'status': 'completed', 'conclusion': conclusion}
        with patch.object(release, 'api', return_value=[{'workflow_runs': [item(1, 'push'), item(2, 'pull_request'), item(3, 'push', revision='b' * 40)]}]):
            self.assertTrue(release.ci_ready(REVISION)); self.assertEqual(release.ci_run_id(REVISION), 1)
        with patch.object(release, 'api', return_value=[{'workflow_runs': [item(4, 'push', 'failure')]}]):
            with self.assertRaisesRegex(ValueError, 'exact-push'): release.ci_run_id(REVISION)

    def test_wait_cli_preserves_the_verified_success_snapshot(self):
        successful = {'id': 42, 'event': 'push', 'head_sha': REVISION,
                      'path': '.github/workflows/ci.yml', 'status': 'completed', 'conclusion': 'success'}
        # A second list read can be missing or older than the already verified
        # success. Do not discard its run identity before writing the output.
        for subsequent in [[], [{**successful, 'status': 'in_progress', 'conclusion': None}]]:
            with self.subTest(subsequent=subsequent), tempfile.TemporaryDirectory(dir=os.environ.get('TMPDIR')) as temporary:
                output = Path(temporary) / 'github-output'
                snapshots = [{'workflow_runs': [successful]}], [{'workflow_runs': subsequent}]
                with patch.object(release, 'api', side_effect=snapshots) as api, \
                     patch.dict(os.environ, {'GITHUB_OUTPUT': str(output)}), \
                     patch.object(sys, 'argv', ['release.py', 'wait-ci', '--revision', REVISION]):
                    release.main()
                self.assertEqual(output.read_text(), 'ci_run_id=42\n')
                self.assertEqual(api.call_count, 1)

    def test_wait_ci_polls_and_returns_the_latest_exact_push_across_pages(self):
        successful = {'id': 42, 'event': 'push', 'head_sha': REVISION,
                      'path': '.github/workflows/ci.yml', 'status': 'completed', 'conclusion': 'success'}
        unrelated = [{**successful, 'id': 43, 'event': 'pull_request'},
                     {**successful, 'id': 44, 'head_sha': 'b' * 40}]
        snapshots = [
            [{'workflow_runs': [*unrelated, {**successful, 'status': 'in_progress', 'conclusion': None}]}],
            [{'workflow_runs': [{**successful, 'id': 41}]}, {'workflow_runs': [*unrelated, successful]}],
        ]
        with patch.object(release, 'api', side_effect=snapshots), \
             patch.object(release.time, 'sleep') as sleep, patch('builtins.print'):
            self.assertEqual(release.wait_ci(REVISION), 42)
        sleep.assert_called_once_with(30)

    def test_wait_ci_does_not_replace_a_newer_failure_with_an_older_success(self):
        successful = {'id': 41, 'event': 'push', 'head_sha': REVISION,
                      'path': '.github/workflows/ci.yml', 'status': 'completed', 'conclusion': 'success'}
        with patch.object(release, 'api', return_value=[{'workflow_runs': [
                successful, {**successful, 'id': 42, 'conclusion': 'failure'}]}]), \
             patch.object(release.time, 'sleep') as sleep:
            with self.assertRaisesRegex(ValueError, 'Exact-source checks failed'):
                release.wait_ci(REVISION)
        sleep.assert_not_called()

    def test_wait_ci_keeps_its_deadline_for_missing_or_nonqualifying_runs(self):
        successful = {'id': 42, 'event': 'push', 'head_sha': REVISION,
                      'path': '.github/workflows/ci.yml', 'status': 'completed', 'conclusion': 'success'}
        for runs in [[], [{**successful, 'event': 'pull_request'}],
                     [{**successful, 'head_sha': 'b' * 40}],
                     [{**successful, 'path': '.github/workflows/dev-release.yml'}]]:
            with self.subTest(runs=runs), patch.object(release, 'api', return_value=[{'workflow_runs': runs}]), \
                 patch.object(release.time, 'monotonic', side_effect=[0, 1, 115 * 60 + 1]), \
                 patch.object(release.time, 'sleep'), patch('builtins.print'):
                with self.assertRaisesRegex(TimeoutError, 'Exact-source CI did not complete'):
                    release.wait_ci(REVISION)

    def test_build_ci_run_still_requires_a_fresh_successful_source_read(self):
        with patch.object(release, 'api', return_value=[{'workflow_runs': [{
                'id': 42, 'event': 'push', 'head_sha': REVISION, 'path': '.github/workflows/ci.yml',
                'status': 'in_progress', 'conclusion': None}]}]):
            with self.assertRaisesRegex(ValueError, 'Successful exact-push core CI is required'):
                release.ci_run_id(REVISION)

    def test_original_main_dev_tag_policy_and_schema4_bundle(self):
        self.assertEqual(version('v2.0.0-dev.20260930123456.123'), 'v2.0.0-dev.20260930123456.123')
        for tag in ['latest', 'v01.2.3', 'v1.2.3-01', 'v1.2.3+build']:
            with self.assertRaises(ValueError): version(tag)
        with patch.dict(os.environ, {'RELEASE_BRANCH': 'main'}):
            with self.assertRaisesRegex(ValueError, 'dev publisher'): release.verify('v2.0.0-dev.20260930123456.123', REVISION)
        with tempfile.TemporaryDirectory(dir=os.environ.get('TMPDIR')) as temporary:
            root = Path(temporary)
            manifest = release.bundle('v2.0.0', REVISION, 'sha256:' + 'b' * 64, root, runtime_image='sha256:' + 'c' * 64)
            self.assertEqual(manifest['schema_version'], 4)
            self.assertEqual(manifest['database_image'], DATABASE_IMAGE)
            with tarfile.open(root / 'quazonai-deploy.tar.gz') as archive:
                self.assertEqual(set(archive.getnames()), release.BUNDLE_FILES)
                self.assertEqual(json.load(archive.extractfile('release.json')), manifest)
            with self.assertRaises(ValueError): validate_manifest({**manifest, 'schema_version': 3})

if __name__ == '__main__': unittest.main()
