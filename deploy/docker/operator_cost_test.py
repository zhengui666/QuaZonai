"""Cost measurement gates must reject missing or mismatched evidence."""
import copy
import gzip
import hashlib
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

    def write_native_fixture(self, root, layout='shared'):
        expected = {'input_sha256': '1' * 64, 'recipe_sha256': '2' * 64, 'platform': 'linux/amd64'}
        payload_root, application_root = root / 'operator', root / 'application'
        (payload_root / 'bin').mkdir(parents=True)
        application_root.mkdir()
        operators = ('source-tools',) if layout == 'shared' else ('catalog-prepare', 'polymarket-history')
        hashes = {}
        for name in ('server', 'runtime', *operators):
            path = (application_root if name in ('server', 'runtime') else payload_root / 'bin') / name
            header = b'\x7fELF\x02\x01\x01' + b'\0' * 9 + b'\x03\0\x3e\0'
            path.write_bytes(header + name.encode() * 8)
            path.chmod(0o755)
            hashes[name] = hashlib.sha256(path.read_bytes()).hexdigest()
        if layout == 'shared':
            for name in ('catalog-prepare', 'polymarket-history'):
                path = payload_root / 'bin' / name
                path.write_text('#!/bin/sh\nexec /opt/quazonai/operator/bin/source-tools ' + name + ' "$@"\n')
                path.chmod(0o755)
        for name in ('source_plugins.py', 'acquire.py', 'providers.py', 'snapshot.py', 'binance_vision.py', 'hf_dataset.py'):
            (payload_root / name).write_text('# actual fixture bytes\n')
        native = {'schema_version': 2, **expected, 'elf_sha256': hashes.copy(),
                  'original_native_build_elapsed_seconds': 1576,
                  'original_disk_before_bytes': 123456, 'original_disk_after_bytes': 45678}
        (payload_root / 'build-metrics.json').write_text(json.dumps(native))
        application = {name: hashes[name] for name in ('server', 'runtime')}
        return payload_root, application_root, expected, application

    def native_fixture(self, layout='shared'):
        with tempfile.TemporaryDirectory() as temporary:
            root, app, expected, application = self.write_native_fixture(Path(temporary), layout)
            return cost.payload_inventory(root, layout, app), expected, application

    def test_both_explicit_layouts_measure_only_actual_elf_bytes(self):
        for layout, names in (('shared', {'source-tools'}),
                              ('standalone', {'catalog-prepare', 'polymarket-history'})):
            payload, expected, application = self.native_fixture(layout)
            with self.subTest(layout=layout):
                cost.validate_native_build(payload, expected, application, layout)
                self.assertEqual(set(payload['stripped_binary_bytes']), names)
                self.assertEqual(set(payload['elf_sha256']), names | {'server', 'runtime'})
                self.assertEqual(payload['inventory_errors'], [])
                self.assertEqual(payload['operator_payload_bytes'], sum(payload['operator_payload_file_bytes'].values()))
                wrong = 'standalone' if layout == 'shared' else 'shared'
                with self.assertRaises(ValueError):
                    cost.validate_native_build(payload, expected, application, wrong)
        with self.assertRaisesRegex(ValueError, 'Unknown'):
            cost.payload_program('legacy')

    def test_module_expectation_follows_current_and_historical_source_declarations(self):
        original = {'source_plugins.py', 'acquire.py', 'providers.py', 'snapshot.py', 'binance_vision.py'}
        for hf in (False, True):
            with self.subTest(hf=hf), tempfile.TemporaryDirectory() as temporary:
                source = Path(temporary)
                dockerfile = source / 'deploy/docker/Dockerfile'
                dockerfile.parent.mkdir(parents=True)
                modules = sorted(original | ({'hf_dataset.py'} if hf else set()))
                dockerfile.write_text('COPY ' + ' '.join('runtimes/data/' + name for name in modules)
                                      + ' /opt/quazonai/operator/\n'
                                      + '# COPY runtimes/data/hf_dataset.py /ignored/\n')
                self.assertEqual(cost.source_operator_modules(source), set(modules))

    def test_missing_extra_or_misclassified_elf_and_nonregular_payload_are_rejected(self):
        for layout in ('shared', 'standalone'):
            for change in ('missing-elf', 'extra-elf', 'elf-as-script', 'bad-header',
                           'extra-file', 'missing-module', 'missing-hf-module', 'symlink', 'not-executable', 'extra-directory'):
                with self.subTest(layout=layout, change=change), tempfile.TemporaryDirectory() as temporary:
                    root, app, expected, application = self.write_native_fixture(Path(temporary), layout)
                    name = 'source-tools' if layout == 'shared' else 'catalog-prepare'
                    path = root / 'bin' / name
                    if change == 'missing-elf':
                        path.unlink()
                    elif change == 'extra-elf':
                        extra = root / 'unexpected-elf'
                        extra.write_bytes(path.read_bytes())
                        extra.chmod(0o755)
                    elif change == 'elf-as-script':
                        path.write_text('#!/bin/sh\nexit 0\n')
                    elif change == 'bad-header':
                        path.write_bytes(b'\x7fELF' + b'\0' * 40)
                    elif change == 'extra-file':
                        (root / 'unrecorded.txt').write_text('extra')
                    elif change == 'missing-module':
                        (root / 'binance_vision.py').unlink()
                    elif change == 'missing-hf-module':
                        (root / 'hf_dataset.py').unlink()
                    elif change == 'symlink':
                        path.unlink()
                        path.symlink_to(app / 'server')
                    elif change == 'not-executable':
                        path.chmod(0o644)
                    else:
                        (root / 'unexpected-directory').mkdir()
                    payload = cost.payload_inventory(root, layout, app)
                    with self.assertRaises(ValueError):
                        cost.validate_native_build(payload, expected, application, layout)

    def test_both_launchers_must_have_exact_fixed_body_and_mode(self):
        for name in ('catalog-prepare', 'polymarket-history'):
            for change in ('missing', 'elf', 'script', 'mode', 'symlink'):
                with self.subTest(name=name, change=change), tempfile.TemporaryDirectory() as temporary:
                    root, app, expected, application = self.write_native_fixture(Path(temporary))
                    path = root / 'bin' / name
                    if change == 'missing':
                        path.unlink()
                    elif change == 'elf':
                        path.write_bytes((app / 'server').read_bytes())
                    elif change == 'script':
                        path.write_text(path.read_text().replace('"$@"', '"$*"'))
                    elif change == 'mode':
                        path.chmod(0o644)
                    else:
                        other = root / ('saved-' + name)
                        path.rename(other)
                        path.symlink_to(other)
                    payload = cost.payload_inventory(root, 'shared', app)
                    with self.assertRaises(ValueError):
                        cost.validate_native_build(payload, expected, application)

    def test_serialized_image_inventory_executes_the_same_measurement(self):
        with tempfile.TemporaryDirectory() as temporary:
            root, app, expected, application = self.write_native_fixture(Path(temporary))
            program = cost.payload_program().replace(repr('/opt/quazonai/operator'), repr(str(root)))
            program = program.replace(repr('/opt/quazonai/bin'), repr(str(app)))
            with patch('sys.stdout', new_callable=io.StringIO) as output:
                exec(program, {})
            payload = json.loads(output.getvalue())
            self.assertEqual(payload, cost.payload_inventory(root, 'shared', app))
            cost.validate_native_build(payload, expected, application)

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
        for key in ('input_sha256', 'recipe_sha256'):
            payload, expected, application = self.native_fixture()
            expected[key] = 'f' * 64
            with self.subTest(current_source=key), self.assertRaisesRegex(ValueError, 'current-source'):
                cost.validate_native_build(payload, expected, application)
        for which in ('producer', 'measured', 'baseline'):
            payload, expected, application = self.native_fixture()
            if which == 'producer':
                payload['native_build']['elf_sha256']['source-tools'] = 'a' * 64
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
                payload['stripped_binary_bytes']['source-tools'] = 0
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
            self.assertEqual(report['schema_version'], 3)
            self.assertEqual(report['layout'], 'shared')
            self.assertEqual(report['source_native_identity'], expected)
            self.assertEqual(report['validation_errors'], [])
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

    def test_all_mismatches_and_raw_sizes_are_retained_before_failure(self):
        for strict in (True, False):
            payload, expected, application = self.native_fixture()
            payload['native_build']['input_sha256'] = 'a' * 64
            payload['native_build']['recipe_sha256'] = 'b' * 64
            payload['native_build']['elf_sha256']['runtime'] = 'c' * 64
            baseline = {'id': 'base', 'size_bytes': 1000,
                        'application_elf_sha256': application,
                        'stripped_application_binary_bytes': {'server': 100, 'runtime': 200}}
            candidate = {'id': 'candidate', 'size_bytes': 1500, 'payload': payload,
                         'application_elf_sha256': {**application, 'server': 'd' * 64},
                         'stripped_application_binary_bytes': {'server': 101, 'runtime': 200}}
            with self.subTest(strict=strict), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                for phase, seconds in (('candidate-start', 10), ('candidate-end', 13),
                                       ('baseline-start', 13), ('baseline-end', 14)):
                    (root / (phase + '.json')).write_text(json.dumps({'monotonic_ns': seconds * 1_000_000_000}))
                with patch.object(cost, 'native_identity', return_value=expected), \
                        patch.object(cost, 'measure_image', side_effect=[baseline, candidate]), \
                        patch.object(cost, 'archive_bytes', side_effect=[100, 150]) as archives:
                    if strict:
                        with self.assertRaisesRegex(ValueError, 'complete observations retained'):
                            cost.report(root, 'base', 'candidate', 'b' * 40, 'ci', emit=False)
                    else:
                        returned = cost.report(root, 'base', 'candidate', 'b' * 40, 'ci', emit=False, strict=False)
                    self.assertEqual(archives.call_count, 2)
                report = json.loads((root / 'report.json').read_text())
                self.assertEqual(report, json.loads((root / 'image-observations.json').read_text()))
                self.assertEqual(len(report['validation_errors']), 4)
                self.assertEqual(report['payload'], payload)
                self.assertEqual(report['baseline']['compressed_archive_bytes'], 100)
                self.assertEqual(report['candidate']['compressed_archive_bytes'], 150)
                self.assertEqual(report['observations']['candidate-start']['monotonic_ns'], 10_000_000_000)
                if not strict:
                    self.assertEqual(returned, report)

    def test_invalid_current_source_identity_preserves_independent_image_observations(self):
        for source_identity in (ValueError('unsupported native input'), {}, {'input_sha256': 'stale'}):
            payload, expected, application = self.native_fixture()
            before = {'id': 'base', 'size_bytes': 1000, 'application_elf_sha256': application,
                      'stripped_application_binary_bytes': {'server': 100, 'runtime': 200}}
            after = {**copy.deepcopy(before), 'id': 'candidate', 'size_bytes': 1500, 'payload': payload}
            with self.subTest(source_identity=source_identity), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                for index, phase in enumerate(('candidate-start', 'candidate-end', 'baseline-start', 'baseline-end')):
                    (root / (phase + '.json')).write_text(json.dumps({'monotonic_ns': index}))
                identity_options = ({'side_effect': source_identity} if isinstance(source_identity, Exception)
                                    else {'return_value': source_identity})
                with patch.object(cost, 'native_identity', **identity_options), \
                        patch.object(cost, 'measure_image', side_effect=[before, after]), \
                        patch.object(cost, 'archive_bytes', side_effect=[100, 150]) as archives:
                    result = cost.report(root, 'base', 'candidate', 'b' * 40, 'ci', emit=False, strict=False)
                self.assertEqual(archives.call_count, 2)
                self.assertTrue(result['validation_errors'])
                self.assertEqual(result['payload'], payload)
                self.assertEqual(result['baseline']['compressed_archive_bytes'], 100)
                self.assertEqual(result['candidate']['compressed_archive_bytes'], 150)
                self.assertEqual(result, json.loads((root / 'report.json').read_text()))

    def cleanup_record(self, status='complete'):
        return {'tag': 'quazonai-bundle/application:ci', 'original_id': None,
                'measured_id': 'sha256:' + 'a' * 64, 'status': status,
                'observed_before_cleanup': 'sha256:' + 'a' * 64,
                'observed_after_cleanup': None, 'outcome': 'removed'}

    def test_unresolved_or_malformed_journal_blocks_direct_archive_retry(self):
        completed = self.cleanup_record()
        invalid = [json.dumps([self.cleanup_record(state)]) for state in ('pending', 'blocked')]
        invalid += ['[', '{}', json.dumps([{'status': 'complete'}]),
                    json.dumps([{**completed, 'observed_after_cleanup': 'sha256:' + 'b' * 64}])]
        for body in invalid:
            with self.subTest(body=body), tempfile.TemporaryDirectory() as temporary:
                journal = Path(temporary) / 'archive-tags.json'
                journal.write_text(body)
                with patch.object(cost, 'run') as docker, patch.object(cost.subprocess, 'Popen') as save, \
                        self.assertRaises(ValueError):
                    cost.archive_bytes('sha256:' + 'a' * 64, 'ci', journal)
                docker.assert_not_called()
                save.assert_not_called()
                self.assertEqual(journal.read_text(), body)

    def test_shared_pair_journal_survives_old_candidate_and_report_retry(self):
        for restored in (False, True):
            with self.subTest(restored=restored), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                journal = root / 'archive-tags.json'
                alias = 'quazonai-bundle/application:ci'
                original = 'sha256:' + 'f' * 64
                images = {name: 'sha256:' + digit * 64 for name, digit in
                          (('old-base', 'a'), ('old-full', 'b'), ('candidate-base', 'c'), ('candidate-full', 'd'))}
                tags, mutations, saves, reports = {alias: original}, [], [], []
                def docker(args):
                    if args[:3] == ['docker', 'image', 'ls']:
                        return tags.get(args[-1], '')
                    if args[:3] == ['docker', 'image', 'inspect']:
                        return args[-1]
                    if args[:3] == ['docker', 'image', 'tag']:
                        mutations.append(args)
                        if args[-2] == original and not restored:
                            raise ValueError('Original alias restoration is unconfirmed')
                        tags[args[-1]] = args[-2]
                        return ''
                    raise AssertionError(args)
                class SavedImage:
                    def __init__(self, args, **kwargs):
                        saves.append(tags[alias])
                        self.first = len(saves) == 1
                        self.stdout = io.BytesIO(b'actual gzip stream fixture')
                    def __enter__(self): return self
                    def __exit__(self, *args): return False
                    def wait(self): return 1 if self.first else 0
                    def kill(self): pass
                for variant in ('old', 'candidate', 'candidate'):
                    directory = root / variant
                    directory.mkdir(exist_ok=True)
                    for index, phase in enumerate(('candidate-start', 'candidate-end', 'baseline-start', 'baseline-end')):
                        (directory / (phase + '.json')).write_text(json.dumps({'monotonic_ns': index}))
                    layout = 'standalone' if variant == 'old' else 'shared'
                    payload, expected, application = self.native_fixture(layout)
                    before = {'id': images[variant + '-base'], 'size_bytes': 1000,
                              'application_elf_sha256': application,
                              'stripped_application_binary_bytes': {'server': 100, 'runtime': 200}}
                    after = {**copy.deepcopy(before), 'id': images[variant + '-full'], 'size_bytes': 1500, 'payload': payload}
                    with patch.object(cost, 'native_identity', return_value=expected), \
                            patch.object(cost, 'measure_image', side_effect=[before, after]) as observations, \
                            patch.object(cost, 'run', side_effect=docker), \
                            patch.object(cost.subprocess, 'Popen', SavedImage):
                        result = cost.report(directory, variant + '-base', variant + '-full', 'b' * 40, 'ci',
                                             layout=layout, emit=False, strict=False, cleanup_evidence=journal)
                    self.assertEqual(observations.call_count, 2)
                    self.assertEqual(result['payload'], payload)
                    self.assertEqual(result['candidate']['application_elf_sha256'], application)
                    self.assertEqual(result['candidate']['size_bytes'], 1500)
                    reports.append(result)
                self.assertTrue(reports[0]['validation_errors'])
                self.assertEqual(len(saves), 6 if restored else 1)
                self.assertEqual(len(mutations), 12 if restored else 2)
                if restored:
                    self.assertEqual(tags[alias], original)
                    self.assertEqual(reports[1]['validation_errors'], [])
                    self.assertEqual(reports[2]['validation_errors'], [])
                else:
                    self.assertEqual(tags[alias], images['old-base'])
                    self.assertEqual(len(json.loads(journal.read_text())), 1)
                    self.assertEqual(json.loads(journal.read_text())[0]['status'], 'blocked')
                    for result in reports[1:]:
                        for image in ('baseline', 'candidate'):
                            self.assertNotIn('compressed_archive_bytes', result[image])
                            self.assertIn('previous temporary alias cleanup is unconfirmed', result[image]['archive_error'])
                        self.assertTrue(result['validation_errors'])

    def test_archive_failure_continues_only_after_confirmed_alias_cleanup(self):
        for cleanup_status in ('complete', 'pending', 'blocked'):
            payload, expected, application = self.native_fixture()
            before = {'id': 'base', 'size_bytes': 1000, 'application_elf_sha256': application,
                      'stripped_application_binary_bytes': {'server': 100, 'runtime': 200}}
            after = {**copy.deepcopy(before), 'id': 'candidate', 'size_bytes': 1500, 'payload': payload}
            def archive(image, version, evidence):
                if image == 'base':
                    evidence.write_text(json.dumps([self.cleanup_record(cleanup_status)]))
                    raise ValueError('Original archive stream failed')
                return 150
            with self.subTest(cleanup=cleanup_status), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                for index, phase in enumerate(('candidate-start', 'candidate-end', 'baseline-start', 'baseline-end')):
                    (root / (phase + '.json')).write_text(json.dumps({'monotonic_ns': index}))
                with patch.object(cost, 'native_identity', return_value=expected), \
                        patch.object(cost, 'measure_image', side_effect=[before, after]), \
                        patch.object(cost, 'archive_bytes', side_effect=archive) as archives:
                    result = cost.report(root, 'base', 'candidate', 'b' * 40, 'ci', emit=False, strict=False)
                self.assertEqual(archives.call_count, 2 if cleanup_status == 'complete' else 1)
                self.assertTrue(result['validation_errors'])
                self.assertEqual(result['payload'], payload)
                if cleanup_status == 'complete':
                    self.assertEqual(result['candidate']['compressed_archive_bytes'], 150)
                else:
                    self.assertIn('previous temporary alias cleanup is unconfirmed', result['candidate']['archive_error'])
                self.assertEqual(result, json.loads((root / 'image-observations.json').read_text()))

    def test_partial_image_failure_keeps_identity_and_measures_other_image(self):
        payload, expected, application = self.native_fixture()
        after = {'id': 'candidate', 'size_bytes': 1500, 'payload': payload,
                 'application_elf_sha256': application,
                 'stripped_application_binary_bytes': {'server': 100, 'runtime': 200}}
        def measure(image, revision, version, layout, observations):
            if image == 'base':
                observations.update({'id': 'base', 'size_bytes': 1000})
                raise ValueError('Application stat failed')
            return after
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for index, phase in enumerate(('candidate-start', 'candidate-end', 'baseline-start', 'baseline-end')):
                (root / (phase + '.json')).write_text(json.dumps({'monotonic_ns': index}))
            with patch.object(cost, 'native_identity', return_value=expected), \
                    patch.object(cost, 'measure_image', side_effect=measure) as images, \
                    patch.object(cost, 'archive_bytes', side_effect=[100, 150]) as archives:
                result = cost.report(root, 'base', 'candidate', 'b' * 40, 'ci', emit=False, strict=False)
            self.assertEqual(images.call_count, 2)
            self.assertEqual(archives.call_count, 2)
            self.assertEqual(result['baseline']['id'], 'base')
            self.assertEqual(result['baseline']['size_bytes'], 1000)
            self.assertIn('Application stat failed', result['baseline']['measurement_error'])
            self.assertEqual(result['payload'], payload)
            self.assertTrue(result['validation_errors'])

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
