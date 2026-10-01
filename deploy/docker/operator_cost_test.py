"""Cost measurement gates must reject missing or mismatched evidence."""
import copy
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

    def native_fixture(self):
        expected = {'input_sha256': '1' * 64, 'recipe_sha256': '2' * 64, 'platform': 'linux/amd64'}
        hashes = {name: str(index) * 64 for index, name in enumerate(
            ('server', 'runtime', 'catalog-prepare', 'polymarket-history'), 3)}
        native = {'schema_version': 2, **expected, 'elf_sha256': hashes.copy(),
                  'original_native_build_elapsed_seconds': 1576,
                  'original_disk_before_bytes': 123456, 'original_disk_after_bytes': 45678}
        payload = {'native_build': native, 'elf_sha256': hashes.copy(),
                   'stripped_binary_bytes': {'catalog-prepare': 321, 'polymarket-history': 654}}
        application = {name: hashes[name] for name in ('server', 'runtime')}
        return payload, expected, application

    def test_cached_producer_retains_original_measurements(self):
        payload, expected, application = self.native_fixture()
        before = copy.deepcopy(payload)
        # Packaging revision is deliberately not a producer field. Reuse is
        # admitted only by current native inputs, recipe and measured ELF bytes.
        cost.validate_native_build(payload, expected, application)
        self.assertEqual(payload, before)
        self.assertEqual(payload['native_build']['original_native_build_elapsed_seconds'], 1576)

    def test_stale_inputs_recipe_platform_or_elf_are_rejected(self):
        for key in ('input_sha256', 'recipe_sha256', 'platform'):
            payload, expected, application = self.native_fixture()
            payload['native_build'][key] = 'stale'
            with self.subTest(field=key), self.assertRaises(ValueError):
                cost.validate_native_build(payload, expected, application)
        for which in ('producer', 'measured', 'baseline'):
            payload, expected, application = self.native_fixture()
            if which == 'producer':
                payload['native_build']['elf_sha256']['catalog-prepare'] = 'a' * 64
            elif which == 'measured':
                payload['elf_sha256']['server'] = 'a' * 64
            else:
                application['runtime'] = 'a' * 64
            with self.subTest(which=which), self.assertRaises(ValueError):
                cost.validate_native_build(payload, expected, application)

    def test_historical_or_incomplete_producer_is_not_reinterpreted(self):
        for change in ('historical', 'missing', 'negative', 'bool', 'missing-elf', 'empty-binary'):
            payload, expected, application = self.native_fixture()
            if change == 'historical':
                payload['native_build'] = {'revision': 'a' * 40, 'native_build_elapsed_seconds': 1576}
            elif change == 'missing':
                del payload['native_build']['original_disk_before_bytes']
            elif change == 'negative':
                payload['native_build']['original_native_build_elapsed_seconds'] = -1
            elif change == 'bool':
                payload['native_build']['original_disk_after_bytes'] = True
            elif change == 'missing-elf':
                del payload['elf_sha256']['server']
            else:
                payload['stripped_binary_bytes']['catalog-prepare'] = 0
            with self.subTest(change=change), self.assertRaises(ValueError):
                cost.validate_native_build(payload, expected, application)

    def test_packaging_version_and_platform_are_checked_separately(self):
        value = [{'Id': 'sha256:' + 'a' * 64, 'Size': 123, 'Os': 'linux', 'Architecture': 'amd64',
                  'Config': {'Labels': {'org.opencontainers.image.revision': 'b' * 40,
                                       'org.opencontainers.image.version': 'v1'}}}]
        with patch.object(cost, 'run', return_value=json.dumps(value)):
            cost.image_identity('candidate', 'b' * 40, 'v1')
            with self.assertRaisesRegex(ValueError, 'packaging version'):
                cost.image_identity('candidate', 'b' * 40, 'v2')
        value[0]['Architecture'] = 'arm64'
        with patch.object(cost, 'run', return_value=json.dumps(value)), self.assertRaisesRegex(ValueError, 'platform'):
            cost.image_identity('candidate', 'b' * 40, 'v1')

    def test_report_separates_current_packaging_time_from_original_producer(self):
        payload, expected, application = self.native_fixture()
        original = copy.deepcopy(payload)
        def measured(args):
            if '/usr/bin/stat' in args:
                return '100\n200'
            if '/usr/bin/sha256sum' in args:
                return '\n'.join(application[name] + '  /opt/quazonai/bin/' + name for name in ('server', 'runtime'))
            if '/usr/bin/python3' in args:
                return json.dumps(payload)
            return ''
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for phase, seconds in (('candidate-start', 10), ('candidate-end', 13),
                                   ('baseline-start', 13), ('baseline-end', 14)):
                (root / (phase + '.json')).write_text(json.dumps({'monotonic_ns': seconds * 1_000_000_000}))
            with patch.object(cost, 'native_identity', return_value=expected), \
                    patch.object(cost, 'image_identity', side_effect=[{'id': 'base', 'size_bytes': 1000},
                                                                      {'id': 'candidate', 'size_bytes': 1500}]), \
                    patch.object(cost, 'run', side_effect=measured), \
                    patch.object(cost, 'archive_bytes', side_effect=[100, 150]), patch('sys.stdout', new_callable=io.StringIO):
                cost.report(root, 'base', 'candidate', 'b' * 40, 'new-packaging-version')
            report = json.loads((root / 'report.json').read_text())
            self.assertEqual(report['schema_version'], 2)
            self.assertEqual(report['revision'], 'b' * 40)
            self.assertEqual(report['version'], 'new-packaging-version')
            self.assertEqual(report['normal_candidate_build_seconds'], 3)
            self.assertEqual(report['payload']['native_build'], original['native_build'])
            self.assertEqual(report['payload']['native_build']['original_native_build_elapsed_seconds'], 1576)

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
