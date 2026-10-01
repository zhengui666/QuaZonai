"""Cost measurement gates must reject missing or mismatched evidence."""
import gzip
import hashlib
import io
import json
from pathlib import Path
import tempfile
import subprocess
import time
import unittest
from unittest.mock import patch

import operator_cost as cost
import release
from release_test import metadata


class CostTests(unittest.TestCase):
    def payload(self, root, layout='single'):
        (root / 'bin').mkdir()
        names = ('source-tools',) if layout == 'single' else ('catalog-prepare', 'polymarket-history')
        for name in names:
            elf = root / 'bin' / name
            elf.write_bytes(b'\x7fELF\x02\x01\x01' + b'\0' * 9 + b'\x03\0\x3e\0' + b'fixture')
            elf.chmod(0o755)
        if layout == 'single':
            for name in ('catalog-prepare', 'polymarket-history'):
                launcher = root / 'bin' / name
                launcher.write_text('#!/bin/sh\nexec /opt/quazonai/operator/bin/source-tools ' + name + ' "$@"\n')
                launcher.chmod(0o755)
        (root / 'source_plugins.py').write_text('# fixture operator payload\n')
        (root / 'build-metrics.json').write_text(json.dumps({'revision': 'a' * 40, 'native_build_elapsed_seconds': 3}))

    def test_actual_elf_sizes_are_separate_from_launchers_and_include_every_payload_file(self):
        for layout in ('single', 'legacy'):
            with self.subTest(layout=layout), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                self.payload(root, layout)
                measured = cost.payload_inventory(root, 'a' * 40, layout)
                expected = {'source-tools'} if layout == 'single' else {'catalog-prepare', 'polymarket-history'}
                self.assertEqual(set(measured['stripped_binary_bytes']), expected)
                self.assertEqual(set(measured['launcher_bytes']), {'catalog-prepare', 'polymarket-history'} if layout == 'single' else set())
                self.assertEqual(measured['operator_payload_bytes'], sum(path.stat().st_size for path in root.rglob('*') if path.is_file()))
                for name, digest in measured['operator_payload_sha256'].items():
                    self.assertEqual(digest, hashlib.sha256((root / name).read_bytes()).hexdigest())

    def test_rejects_missing_nonexecutable_symlinked_and_unexpected_elf_payloads(self):
        for mutation in ('missing', 'nonexecutable', 'symlink', 'extra-elf', 'script-for-elf', 'bad-elf', 'symlink-directory', 'nonexecutable-launcher'):
            with self.subTest(mutation=mutation), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                self.payload(root)
                elf = root / 'bin/source-tools'
                if mutation == 'missing':
                    elf.unlink()
                elif mutation == 'nonexecutable':
                    elf.chmod(0o644)
                elif mutation == 'symlink':
                    elf.rename(root / 'actual')
                    elf.symlink_to('../actual')
                elif mutation == 'extra-elf':
                    (root / 'unexpected').write_bytes(elf.read_bytes())
                elif mutation == 'script-for-elf':
                    elf.write_text('#!/bin/sh\nexit 0\n')
                elif mutation == 'bad-elf':
                    elf.write_bytes(b'\x7fELF')
                elif mutation == 'symlink-directory':
                    (root / 'bin').rename(root / 'actual-bin')
                    (root / 'bin').symlink_to('actual-bin', target_is_directory=True)
                elif mutation == 'nonexecutable-launcher':
                    (root / 'bin/catalog-prepare').chmod(0o644)
                with self.assertRaises(ValueError):
                    cost.payload_inventory(root, 'a' * 40, 'single')

    def test_payload_producer_revision_and_layout_must_match(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            self.payload(root)
            with self.assertRaisesRegex(ValueError, 'Native build'):
                cost.payload_inventory(root, 'b' * 40, 'single')
            with self.assertRaises(ValueError):
                cost.payload_inventory(root, 'a' * 40, 'legacy')

    def test_image_payload_probe_uses_the_resolved_id_and_runs_the_inventory(self):
        identity = 'sha256:' + 'a' * 64
        paths = ['/opt/quazonai/bin/server', '/opt/quazonai/bin/runtime']
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            self.payload(root)
            calls = []
            def run(args):
                calls.append(args)
                if '/usr/bin/stat' in args:
                    return '100\n200'
                if '/usr/bin/sha256sum' in args:
                    return '\n'.join('f' * 64 + '  ' + name for name in paths)
                self.assertIn('/usr/bin/python3', args)
                # Exercise the exact program shipped into Python in the image.
                program = args[-1].replace("'/opt/quazonai/operator'", repr(str(root)))
                with patch('sys.stdout', new_callable=io.StringIO) as output:
                    exec(program, {})
                return output.getvalue()
            with patch.object(cost, 'image_identity', return_value={'id': identity, 'size_bytes': 1000}), \
                    patch.object(cost, 'run', side_effect=run):
                actual = cost.measure_image('mutable-tag', 'a' * 40, 'single')
            self.assertEqual(actual['stripped_application_binary_bytes'], {'server': 100, 'runtime': 200})
            self.assertEqual(set(actual['payload']['stripped_binary_bytes']), {'source-tools'})
            self.assertTrue(all(identity in args and 'mutable-tag' not in args for args in calls))

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

    def test_report_keeps_measured_bytes_if_archive_measurement_fails(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            for phase in ('candidate-start', 'candidate-end', 'baseline-start', 'baseline-end'):
                cost.mark(directory, phase)
            baseline = {'id': 'sha256:' + 'a' * 64, 'size_bytes': 100}
            candidate = {'id': 'sha256:' + 'b' * 64, 'size_bytes': 200, 'payload': {'operator_payload_bytes': 100}}
            with patch.object(cost, 'measure_image', side_effect=[baseline, candidate]), \
                    patch.object(cost, 'archive_bytes', side_effect=[50, TimeoutError('bounded archive')]):
                with self.assertRaises(TimeoutError):
                    cost.report(directory, 'base', 'full', 'a' * 40, 'ci')
            observations = json.loads((directory / 'image-observations.json').read_text())
            self.assertEqual(observations['baseline']['size_bytes'], 100)
            self.assertEqual(observations['candidate']['size_bytes'], 200)
            self.assertEqual(observations['baseline']['compressed_archive_bytes'], 50)
            self.assertEqual(observations['payload']['operator_payload_bytes'], 100)
            self.assertFalse((directory / 'report.json').exists())

    def test_hosted_deadline_bounds_commands_and_expiration_fails_closed(self):
        with patch.object(cost, 'COMMAND_DEADLINE', time.time() + 30):
            self.assertGreater(cost.command_timeout(), 0)
            self.assertLessEqual(cost.command_timeout(), 30)
            with patch.object(cost.subprocess, 'run') as run:
                cost.run(['docker', 'version'])
                self.assertLessEqual(run.call_args.kwargs['timeout'], 30)
        with patch.object(cost, 'COMMAND_DEADLINE', time.time() - 1), self.assertRaises(TimeoutError):
            cost.run(['docker', 'version'])

    def test_expired_save_uses_reserved_time_to_restore_or_remove_archive_alias(self):
        identity, previous = 'sha256:' + 'a' * 64, 'sha256:' + 'b' * 64
        local = 'quazonai-bundle/application:ci'
        for original in (None, previous, identity):
            with self.subTest(original=original), tempfile.TemporaryDirectory() as temporary:
                now, tags, calls = [1], {}, []
                if original:
                    tags[local] = original
                def docker(args, **kwargs):
                    calls.append((args, now[0], kwargs['timeout']))
                    output = ''
                    if args[:3] == ['docker', 'image', 'ls']:
                        output = tags.get(args[-1], '')
                    elif args[:3] == ['docker', 'image', 'inspect']:
                        output = identity
                    elif args[:3] == ['docker', 'image', 'tag']:
                        tags[args[-1]] = args[-2]
                    elif args[:3] == ['docker', 'image', 'rm']:
                        tags.pop(args[-1], None)
                    return subprocess.CompletedProcess(args, 0, output, '')
                class ExpiredSave:
                    def __init__(self, args, **kwargs):
                        self.stdout = io.BytesIO(b'partial archive')
                    def __enter__(self):
                        return self
                    def __exit__(self, *args):
                        return False
                    def wait(self):
                        now[0] = 11
                        return 124
                    def kill(self):
                        pass
                evidence = Path(temporary) / 'archive-tags.json'
                with patch.object(cost, 'COMMAND_DEADLINE', 10), patch.object(cost, 'CLEANUP_DEADLINE', 190), \
                        patch.object(cost.time, 'time', side_effect=lambda: now[0]), \
                        patch.object(cost.subprocess, 'run', side_effect=docker), \
                        patch.object(cost.subprocess, 'Popen', ExpiredSave):
                    with self.assertRaisesRegex(ValueError, 'archive measurement failed'):
                        cost.archive_bytes(identity, 'ci', evidence)
                    self.assertEqual(cost.COMMAND_DEADLINE, 10)
                self.assertEqual(tags.get(local), original)
                record = json.loads(evidence.read_text())[0]
                self.assertEqual(record['status'], 'complete')
                self.assertEqual(record['original_id'], original)
                self.assertEqual(record['observed_after_cleanup'], original)
                cleanup_calls = [call for call in calls if call[1] == 11]
                self.assertTrue(cleanup_calls)
                self.assertTrue(all(timeout == 179 for _, _, timeout in cleanup_calls))

    def test_archive_cleanup_failure_is_retained_and_no_reserve_refuses_mutation(self):
        identity = 'sha256:' + 'a' * 64
        with patch.object(cost, 'COMMAND_DEADLINE', 10), patch.object(cost, 'CLEANUP_DEADLINE', None), \
                patch.object(cost, 'run') as docker:
            with self.assertRaisesRegex(ValueError, 'reserved alias cleanup time'):
                cost.archive_bytes(identity, 'ci')
            docker.assert_not_called()
        with tempfile.TemporaryDirectory() as temporary:
            evidence = Path(temporary) / 'archive-tags.json'
            changed = 'sha256:' + 'b' * 64
            with patch.object(cost, 'run', side_effect=['', identity, '', changed, changed]), \
                    self.assertRaisesRegex(ValueError, 'current assignment was preserved'):
                cost.archive_bytes(identity, 'ci', evidence)
            record = json.loads(evidence.read_text())[0]
            self.assertEqual(record['status'], 'blocked')
            self.assertEqual(record['observed_before_cleanup'], changed)
            self.assertIn('current assignment was preserved', record['error'])

    def test_concurrently_reassigned_archive_tag_is_preserved(self):
        identity = 'sha256:' + 'a' * 64
        changed = 'sha256:' + 'b' * 64
        with patch.object(cost, 'run', side_effect=['', identity, '', changed, changed]) as docker, \
                self.assertRaisesRegex(ValueError, 'current assignment was preserved'):
            cost.archive_bytes(identity, 'ci')
        self.assertFalse(any(call.args[0][:3] == ['docker', 'image', 'rm'] for call in docker.call_args_list))


if __name__ == '__main__':
    unittest.main()
