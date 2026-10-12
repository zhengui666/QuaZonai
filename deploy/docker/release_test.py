"""Exact source/artifact selection and version policy; no registry mutation."""
import json
import os
from pathlib import Path
import tarfile
import tempfile
import unittest
from unittest.mock import patch
import release
from release_support import DATABASE_IMAGE, validate_manifest, version

REVISION = 'a' * 40
class ReleaseTests(unittest.TestCase):
    def test_source_ci_must_be_successful_exact_push_not_pr(self):
        def item(id, event, conclusion='success', revision=REVISION):
            return {'id': id, 'event': event, 'head_sha': revision, 'path': '.github/workflows/ci.yml', 'status': 'completed', 'conclusion': conclusion}
        with patch.object(release, 'api', return_value=[{'workflow_runs': [item(1, 'push'), item(2, 'pull_request'), item(3, 'push', revision='b' * 40)]}]):
            self.assertTrue(release.ci_ready(REVISION)); self.assertEqual(release.ci_run_id(REVISION), 1)
        with patch.object(release, 'api', return_value=[{'workflow_runs': [item(4, 'push', 'failure')]}]):
            with self.assertRaisesRegex(ValueError, 'exact-push'): release.ci_run_id(REVISION)

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
