"""Cost measurement gates must reject missing or mismatched evidence."""
import gzip
import io
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import operator_cost as cost
import release
from release_test import metadata


class CostTests(unittest.TestCase):
    def test_archive_count_measures_actual_gzip_bytes_and_enforces_limit(self):
        counted = cost.CountedArchive()
        with gzip.GzipFile(fileobj=counted, mode='wb', compresslevel=1, mtime=0) as stream:
            stream.write(b'actual archive bytes' * 100)
        original = io.BytesIO()
        with gzip.GzipFile(filename=counted.name.removesuffix('.gz'), fileobj=original,
                           mode='wb', compresslevel=1, mtime=0) as stream:
            stream.write(b'actual archive bytes' * 100)
        self.assertEqual(counted.bytes, len(original.getvalue()))
        with patch.object(cost, 'MAX_ARCHIVE', counted.bytes + 1), self.assertRaises(ValueError):
            counted.write(b'over limit')

    def test_image_revision_is_measured_and_checked(self):
        value = [{'Id': 'sha256:' + 'a' * 64, 'Size': 123,
                  'Config': {'Labels': {'org.opencontainers.image.revision': 'b' * 40}}}]
        with patch.object(cost, 'run', return_value=json.dumps(value)):
            self.assertEqual(cost.image_identity('candidate', 'b' * 40)['size_bytes'], 123)
            with self.assertRaises(ValueError):
                cost.image_identity('candidate', 'c' * 40)

    def test_missing_build_observation_does_not_produce_report(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            cost.mark(root, 'candidate-start')
            with self.assertRaises(FileNotFoundError):
                cost.report(root, 'base', 'candidate', 'a' * 40, 'ci')
            self.assertFalse((root / 'report.json').exists())

    def test_measurement_matches_actual_release_tag_header_and_compression(self):
        selected = metadata()
        identity = 'sha256:' + 'e' * 64
        previous = 'sha256:' + 'f' * 64
        local = 'quazonai-bundle/application:' + selected['version']
        payload = b'Docker archive fixture with original bytes' * 500
        tags, saves, compressions = {}, [], []
        original_gzip = gzip.GzipFile
        def compressed(*args, **kwargs):
            compressions.append((Path(kwargs['fileobj'].name).name, kwargs['compresslevel'], kwargs['mtime']))
            return original_gzip(*args, **kwargs)
        class SavedImage:
            def __init__(self, args, **kwargs):
                saves.append(args[-1])
                self.stdout = io.BytesIO(payload)
            def __enter__(self):
                return self
            def __exit__(self, *args):
                return False
            def wait(self):
                return 0
            def kill(self):
                pass
        def docker(args, **kwargs):
            if args[:3] == ['docker', 'image', 'ls']:
                return tags.get(args[-1], '')
            if args[:3] == ['docker', 'image', 'tag']:
                tags[args[-1]] = args[-2] if args[-2].startswith('sha256:') else identity
            elif args[:3] == ['docker', 'image', 'rm']:
                for name in args[3:]:
                    tags.pop(name, None)
            elif args[:3] == ['docker', 'image', 'inspect']:
                return identity
            return ''
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / 'release.json').write_text(json.dumps(selected))
            with patch.object(release, 'run', side_effect=docker), patch.object(cost, 'run', side_effect=docker), \
                    patch.object(cost.subprocess, 'Popen', SavedImage), patch.object(cost.gzip, 'GzipFile', side_effect=compressed):
                release.archive_images(root)
                actual = (root / 'quazonai-image-application.tar.gz').stat().st_size
                self.assertEqual(saves[0], local)
                self.assertEqual(compressions[0], ('quazonai-image-application.tar.gz', 1, 0))
                for original in (None, previous, identity):
                    tags.clear()
                    if original:
                        tags[local] = original
                    tags['unrelated:original'] = previous
                    measured = cost.archive_bytes(identity, selected['version'])
                    self.assertEqual(measured, actual)
                    self.assertEqual(saves[-1], saves[0])
                    self.assertEqual(compressions[-1], compressions[0])
                    self.assertEqual(tags.get(local), original)
                    self.assertEqual(tags['unrelated:original'], previous)

    def test_concurrently_reassigned_archive_tag_is_preserved(self):
        identity = 'sha256:' + 'a' * 64
        changed = 'sha256:' + 'b' * 64
        with patch.object(cost, 'run', side_effect=['', identity, '', changed, changed]) as docker, \
                self.assertRaisesRegex(ValueError, 'current assignment was preserved'):
            cost.archive_bytes(identity, 'ci')
        self.assertFalse(any(call.args[0][:3] == ['docker', 'image', 'rm'] for call in docker.call_args_list))


if __name__ == '__main__':
    unittest.main()
