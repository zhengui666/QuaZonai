"""Integrated qualification fails closed while retaining useful observations."""
import copy
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time
import unittest
from unittest.mock import patch

import operator_compare as comparison


SHA = 'a' * 64
COMMIT = 'b' * 40
BUILDKIT = 'moby/buildkit@sha256:' + SHA
EMPTY_CACHE = 'ID        RECLAIMABLE   SIZE      LAST ACCESSED\nReclaimable:\t0B\nTotal:\t\t0B'


class ComparisonTests(unittest.TestCase):
    def reports(self):
        recipe = {'revision': comparison.OLD_REVISION, 'base_images': ['same@sha256:' + SHA],
                  'locked_inputs_sha256': dict.fromkeys(comparison.LOCKED_INPUTS, SHA),
                  'rust_image': 'rust:1.98.1-bookworm@sha256:' + SHA, 'platform': 'linux/amd64',
                  'common_recipe_sha256': SHA, 'tree': 'd' * 40, 'layout': 'standalone',
                  'native_build_sha256': SHA,
                  'common_inputs_sha256': dict.fromkeys(('deploy/docker/native-inputs.mjs', '.dockerignore', 'deploy/docker/Dockerfile.dockerignore'), SHA),
                  'native_identity': {'input_sha256': SHA, 'recipe_sha256': SHA, 'platform': 'linux/amd64'},
                  'operator_build_command': comparison.OPERATOR_RECIPE + ' --bin catalog-prepare --bin polymarket-history'}
        environment = dict.fromkeys(('buildx_version', 'rustc', 'runner_os', 'runner_arch', 'memory',
                                    'docker_version', 'runner_image', 'runner_image_version',
                                    'runner_name', 'run_id', 'run_attempt'), 'same-observation')
        environment.update(builder_image=BUILDKIT, cpu_count=4, cpu_models=['same-cpu'], builder_name='old-builder',
                           runner_boot_id='11111111-2222-3333-4444-555555555555',
                           initial_cache=EMPTY_CACHE, initial_cache_empty=True,
                           containerd={'Name': 'containerd', 'Version': '1.0', 'Details': {'GitCommit': 'same-commit'}})
        full = {'size_bytes': 1000, 'compressed_archive_bytes': 500,
                'application_elf_sha256': {'server': SHA, 'runtime': SHA},
                'stripped_application_binary_bytes': {'server': 100, 'runtime': 200}}
        old = {'schema_version': comparison.REPORT_SCHEMA, 'validation_errors': [], 'layout': 'standalone',
               'source_native_identity': dict(recipe['native_identity']), 'revision': comparison.OLD_REVISION, 'version': 'same-release-tag',
               'measurement_status': 'complete', 'cleanup': {'status': 'complete'},
               'candidate': full, 'baseline': copy.deepcopy(full),
               'payload': {'operator_payload_bytes': 800, 'stripped_binary_bytes': {'catalog-prepare': 400, 'polymarket-history': 300}, 'launcher_bytes': {}},
               'hosted_comparison': {'variant': 'old', 'old_revision': comparison.OLD_REVISION,
                                     'candidate_revision': COMMIT, 'recipe': recipe, 'environment': environment,
                                     'harness': {'revision': COMMIT, 'tree': 'd' * 40, 'file_sha256': dict.fromkeys(comparison.HARNESS_FILES, SHA)},
                                     'runtime_packages': 'same-packages', 'independent_cold_full_build_seconds': 500,
                                     'same_builder_warm_full_build_seconds': 5}}
        candidate = copy.deepcopy(old)
        candidate['revision'] = COMMIT
        candidate['layout'] = 'shared'
        candidate['source_native_identity']['input_sha256'] = 'c' * 64
        candidate['source_native_identity']['recipe_sha256'] = 'e' * 64
        candidate['candidate'].update(size_bytes=850, compressed_archive_bytes=450)
        candidate['payload'] = {'operator_payload_bytes': 650, 'stripped_binary_bytes': {'source-tools': 530},
                                'launcher_bytes': {'catalog-prepare': 50, 'polymarket-history': 50}}
        candidate['hosted_comparison'].update(variant='candidate', independent_cold_full_build_seconds=510,
                                               same_builder_warm_full_build_seconds=6)
        candidate['hosted_comparison']['environment']['builder_name'] = 'candidate-builder'
        candidate['hosted_comparison']['recipe'].update(revision=COMMIT, layout='shared',
            native_identity=dict(candidate['source_native_identity']),
            operator_build_command=comparison.OPERATOR_RECIPE + ' --bin source-tools')
        for report, names in ((old, ('catalog-prepare', 'polymarket-history')), (candidate, ('source-tools',))):
            report['payload']['layout'] = report['layout']
            report['payload']['elf_sha256'] = dict.fromkeys(('server', 'runtime', *names), SHA)
            report['payload']['native_build'] = {
                'schema_version': 2, **report['source_native_identity'],
                'elf_sha256': dict(report['payload']['elf_sha256']),
                'original_native_build_elapsed_seconds': 100,
                'original_disk_before_bytes': 1_000, 'original_disk_after_bytes': 800}
        return old, candidate

    def test_actual_size_reductions_and_slower_timings(self):
        result = comparison.compare(*self.reports(), COMMIT)
        self.assertTrue(result['admissible'], result['reasons'])
        self.assertEqual(result['delta'], {'full_image_bytes': -150, 'full_compressed_archive_bytes': -50,
                                          'operator_payload_bytes': -150, 'stripped_elf_bytes': -170, 'launcher_bytes': 100})
        self.assertEqual(result['build_seconds']['independent_cold_full_build_seconds']['delta'], 10)
        self.assertEqual(result['build_seconds']['same_builder_warm_full_build_seconds']['delta'], 1)
        self.assertTrue(result['application_binaries_identical'])
        self.assertEqual(result['schema_version'], comparison.REPORT_SCHEMA)

    def test_legacy_schema_wrong_layout_and_stale_producer_are_rejected_without_losing_observations(self):
        for side in ('old', 'candidate'):
            for mutation in ('schema1', 'schema2', 'producer-schema1', 'wrong-layout', 'wrong-payload-layout',
                             'source-input', 'source-recipe', 'producer-input', 'producer-recipe',
                             'producer-elf', 'payload-application', 'extra-elf', 'missing-elf', 'reported-error'):
                old, candidate = self.reports()
                report = old if side == 'old' else candidate
                if mutation.startswith('schema'):
                    report['schema_version'] = int(mutation[-1])
                elif mutation == 'producer-schema1':
                    report['payload']['native_build']['schema_version'] = 1
                elif mutation == 'wrong-layout':
                    report['layout'] = 'shared' if side == 'old' else 'standalone'
                elif mutation == 'wrong-payload-layout':
                    report['payload']['layout'] = 'shared' if side == 'old' else 'standalone'
                elif mutation.startswith(('source-', 'producer-')) and mutation != 'producer-elf':
                    section, key = mutation.split('-')
                    target = report['source_native_identity'] if section == 'source' else report['payload']['native_build']
                    target[key + '_sha256'] = 'f' * 64
                elif mutation == 'producer-elf':
                    report['payload']['native_build']['elf_sha256']['server'] = 'f' * 64
                elif mutation == 'payload-application':
                    report['payload']['elf_sha256']['server'] = 'f' * 64
                    report['payload']['native_build']['elf_sha256']['server'] = 'f' * 64
                elif mutation == 'extra-elf':
                    report['payload']['elf_sha256']['unexpected'] = SHA
                elif mutation == 'missing-elf':
                    del report['payload']['elf_sha256']['runtime']
                else:
                    report['validation_errors'] = [{'kind': 'producer', 'error': 'original observation'}]
                with self.subTest(side=side, mutation=mutation):
                    result = comparison.compare(old, candidate, COMMIT)
                    self.assertFalse(result['admissible'])
                    self.assertTrue(result['reasons'])
                    self.assertIs(result[side], report)
                    self.assertEqual(result['delta']['full_image_bytes'], -150)
                    self.assertEqual(result['build_seconds']['independent_cold_full_build_seconds']['candidate'], 510)

    def test_no_reduction_harness_tree_mismatch_and_archive_limit_block_admission(self):
        for mutation in ('image', 'archive', 'payload', 'elf', 'harness-tree', 'archive-limit'):
            old, candidate = self.reports()
            if mutation == 'image':
                candidate['candidate']['size_bytes'] = old['candidate']['size_bytes']
            elif mutation == 'archive':
                candidate['candidate']['compressed_archive_bytes'] = old['candidate']['compressed_archive_bytes']
            elif mutation == 'payload':
                candidate['payload']['operator_payload_bytes'] = old['payload']['operator_payload_bytes']
            elif mutation == 'elf':
                candidate['payload']['stripped_binary_bytes']['source-tools'] = 700
            elif mutation == 'harness-tree':
                candidate['hosted_comparison']['recipe']['tree'] = 'e' * 40
            else:
                candidate['baseline']['compressed_archive_bytes'] = comparison.cost.MAX_ARCHIVE
            with self.subTest(mutation=mutation):
                self.assertFalse(comparison.compare(old, candidate, COMMIT)['admissible'])

    def test_every_required_environment_field_blocks_when_missing_false_or_different(self):
        fields = ('cpu_count', 'memory', 'cpu_models', 'docker_version', 'containerd', 'runner_image_version',
                  'runner_boot_id', 'runner_name', 'run_id', 'run_attempt', 'builder_image', 'rustc', 'buildx_version')
        for field in fields:
            for mutation in ('missing', 'false', 'different'):
                with self.subTest(field=field, mutation=mutation):
                    old, candidate = self.reports()
                    environment = candidate['hosted_comparison']['environment']
                    if mutation == 'missing':
                        del environment[field]
                    else:
                        environment[field] = False if mutation == 'false' else 'different'
                    result = comparison.compare(old, candidate, COMMIT)
                    self.assertFalse(result['admissible'])
                    self.assertTrue(any(reason['field'].endswith(field) for reason in result['reasons']))
                    self.assertEqual(result['delta']['full_image_bytes'], -150)

    def test_collects_all_mismatches_and_preserves_raw_reports_and_timings(self):
        old, candidate = self.reports()
        candidate['revision'] = 'c' * 40
        candidate['hosted_comparison']['environment'].update(memory='different', cpu_models=['different'],
            runner_image_version='different', docker_version='different', containerd=False, initial_cache_empty=False)
        candidate['candidate']['application_elf_sha256']['server'] = 'f' * 64
        del candidate['candidate']['application_elf_sha256']['runtime']
        candidate['candidate']['compressed_archive_bytes'] = 510
        result = comparison.compare(old, candidate, COMMIT)
        self.assertFalse(result['admissible'])
        fields = {reason['field'] for reason in result['reasons']}
        for suffix in ('revision', 'memory', 'cpu_models', 'runner_image_version', 'docker_version', 'containerd',
                       'initial_cache_empty', 'application_elf_sha256.server', 'application_elf_sha256.runtime',
                       'application_binaries_identical', 'full_compressed_archive_bytes'):
            self.assertTrue(any(field.endswith(suffix) for field in fields), suffix)
        self.assertIs(result['candidate'], candidate)
        self.assertEqual(result['build_seconds']['independent_cold_full_build_seconds']['candidate'], 510)

    def test_binary_identity_alone_blocks_admission_and_false_equal_values_are_invalid(self):
        for replacement in ({'server': 'f' * 64, 'runtime': SHA}, False, {}, {'server': False, 'runtime': False}):
            old, candidate = self.reports()
            candidate['candidate']['application_elf_sha256'] = replacement
            if replacement is False:
                old['candidate']['application_elf_sha256'] = False
            result = comparison.compare(old, candidate, COMMIT)
            self.assertFalse(result['application_binaries_identical'])
            self.assertFalse(result['admissible'])
            self.assertTrue(result['required_size_reductions_observed'])

    def test_rejects_missing_baseline_blocked_measurement_shared_builder_and_bad_numbers(self):
        for mutation in ('baseline', 'status', 'builder', 'cache', 'nan', 'boolean-size', 'missing-payload', 'harness'):
            old, candidate = self.reports()
            if mutation == 'baseline':
                del candidate['baseline']
            elif mutation == 'status':
                candidate['measurement_status'] = 'blocked'
            elif mutation == 'builder':
                candidate['hosted_comparison']['environment']['builder_name'] = 'old-builder'
            elif mutation == 'cache':
                candidate['hosted_comparison']['environment']['initial_cache'] = EMPTY_CACHE.replace('Total:\t\t0B', 'Total:\t\t1B')
            elif mutation == 'nan':
                candidate['hosted_comparison']['independent_cold_full_build_seconds'] = float('nan')
            elif mutation == 'boolean-size':
                candidate['candidate']['size_bytes'] = True
            elif mutation == 'missing-payload':
                del candidate['payload']
            else:
                candidate['hosted_comparison']['harness']['file_sha256'] = {}
            with self.subTest(mutation=mutation):
                self.assertFalse(comparison.compare(old, candidate, COMMIT)['admissible'])

    def test_cli_always_writes_summary_before_nonzero_exit_for_incomparable_or_missing_inputs(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            old, candidate = self.reports()
            candidate['hosted_comparison']['environment']['memory'] = 'different'
            candidate['candidate']['application_elf_sha256']['server'] = 'f' * 64
            old_path, candidate_path = directory / 'old.json', directory / 'candidate.json'
            comparison.write_json(old_path, old)
            comparison.write_json(candidate_path, candidate)
            command = [sys.executable, '-B', comparison.__file__, 'compare', '--old', str(old_path),
                       '--candidate', str(candidate_path), '--revision', COMMIT, '--output', str(directory / 'result.json')]
            for mutation in ('incomparable', 'missing', 'malformed'):
                if mutation == 'missing':
                    candidate_path.unlink()
                elif mutation == 'malformed':
                    candidate_path.write_text('{"size": NaN}')
                completed = subprocess.run(command, capture_output=True, text=True)
                self.assertEqual(completed.returncode, 1)
                result = json.loads((directory / 'result.json').read_text())
                self.assertFalse(result['admissible'])
                self.assertTrue(result['reasons'])
                self.assertIn('old', result)

    def test_cli_retains_nonfinite_exponents_and_oversized_timing_integers(self):
        timing = 'independent_cold_full_build_seconds'
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            old, candidate = self.reports()
            old_path, candidate_path, output_path = (directory / name for name in ('old.json', 'candidate.json', 'result.json'))
            comparison.write_json(old_path, old)
            command = [sys.executable, '-B', comparison.__file__, 'compare', '--old', str(old_path),
                       '--candidate', str(candidate_path), '--revision', COMMIT, '--output', str(output_path)]
            for token, field in (('1e999', timing), ('-1e999', timing), ('Infinity', timing),
                                 ('1e999', 'uncompared_diagnostic'), (str(10 ** 1000), timing)):
                with self.subTest(token=token[:12], field=field):
                    report = copy.deepcopy(candidate)
                    report['hosted_comparison'][field] = 'NUMBER_TOKEN'
                    original = json.dumps(report).replace('"NUMBER_TOKEN"', token)
                    candidate_path.write_text(original)
                    output_path.unlink(missing_ok=True)
                    completed = subprocess.run(command, capture_output=True, text=True)
                    self.assertEqual(completed.returncode, 1)
                    self.assertNotIn('Traceback', completed.stderr)
                    result = json.loads(output_path.read_text())
                    self.assertFalse(result['admissible'])
                    self.assertEqual(result['old'], old)
                    if token == str(10 ** 1000):
                        self.assertEqual(result['candidate']['hosted_comparison'][timing], 10 ** 1000)
                        self.assertIsNone(result['build_seconds'][timing]['candidate'])
                        self.assertTrue(any(reason['kind'] == 'malformed' and reason['field'].endswith(timing)
                                            for reason in result['reasons']))
                    else:
                        self.assertEqual(result['candidate']['original_text'], original)
                        self.assertIn('Non-finite JSON number', result['candidate']['input_error'])
                        self.assertTrue(any(reason['kind'] == 'input-error' and reason['field'] == 'candidate'
                                            for reason in result['reasons']))

    def source_fixture(self, directory):
        root = Path(comparison.__file__).resolve().parents[2]
        old, candidate = directory / 'old', directory / 'candidate'
        dockerfile = (root / 'deploy/docker/Dockerfile').read_text()
        helper = (root / 'deploy/docker/native-build.sh').read_text()
        old_helper = helper
        for before, after in reversed(comparison.NATIVE_SUBSTITUTIONS):
            self.assertEqual(old_helper.count(after), 1)
            old_helper = old_helper.replace(after, before)
        for source in (old, candidate):
            for name in (*comparison.LOCKED_INPUTS, '.dockerignore', 'deploy/docker/Dockerfile.dockerignore',
                         'deploy/docker/native-inputs.mjs', 'deploy/docker/native-build.sh', 'deploy/docker/Dockerfile',
                         *(('deploy/docker/operator/' + name) for name in comparison.LAUNCHERS)):
                destination = source / name
                destination.parent.mkdir(parents=True, exist_ok=True)
                destination.write_bytes((root / name).read_bytes())
        (old / 'deploy/docker/Dockerfile').write_text(dockerfile.replace(comparison.LAUNCHER_COPIES, ''))
        (old / 'deploy/docker/native-build.sh').write_text(old_helper)
        return old, candidate

    def source_git(self, old, candidate):
        def git(args):
            if args[-2:] == ['status', '--porcelain'] or 'merge-base' in args:
                return ''
            if args[-1] == 'HEAD^{tree}':
                return 'd' * 40
            return comparison.OLD_REVISION if args[2] == str(old) else COMMIT
        return git

    def native_fixture(self, source):
        return {'schema_version': 1, 'platform': 'linux/amd64',
                'input_sha256': SHA if source.name == 'old' else 'c' * 64,
                'recipe_sha256': SHA if source.name == 'old' else 'e' * 64}

    def test_source_verification_rejects_compiler_strip_feature_lock_and_recipe_changes(self):
        with tempfile.TemporaryDirectory() as temporary:
            old, candidate = self.source_fixture(Path(temporary))
            with patch.object(comparison.cost, 'run', side_effect=self.source_git(old, candidate)), \
                    patch.object(comparison.cost, 'native_identity', side_effect=self.native_fixture) as native:
                recipes = comparison.verify_sources(old, candidate, COMMIT)
                self.assertEqual([call.args[0] for call in native.call_args_list], [old, candidate])
                self.assertNotEqual(recipes[0]['native_identity'], recipes[1]['native_identity'])
                self.assertEqual(recipes[0]['common_recipe_sha256'], recipes[1]['common_recipe_sha256'])
                for name, before, after in (
                    ('native-build.sh', 'CARGO_PROFILE_RELEASE_DEBUG=0', 'RUSTFLAGS=-Copt-level=0 CARGO_PROFILE_RELEASE_DEBUG=0'),
                    ('native-build.sh', '-p server -p runtime', '-p server -p runtime --features something'),
                    ('native-build.sh', 'strip /out/server', 'strip --strip-debug /out/server'),
                    ('native-build.sh', 'polymarket-history,catalog-prepare', 'catalog-prepare'),
                    ('native-build.sh', 'disk_after=', 'unexpected=1\n    disk_after='),
                    ('native-build.sh', '"source-tools":"%s"', '"extra":"%s"'),
                    ('Dockerfile', 'COPY --chmod=755 deploy/docker/operator/catalog-prepare', 'COPY --chmod=777 deploy/docker/operator/catalog-prepare'),
                    ('Dockerfile', 'sh deploy/docker/native-build.sh server', 'sh deploy/docker/native-build.sh server; echo changed'),
                    ('native-inputs.mjs', '// Conservative', '// Changed'),
                    ('Dockerfile.dockerignore', '**/auth.json', '**/credentials.json')):
                    path = candidate / 'deploy/docker' / name
                    body = path.read_text()
                    self.assertIn(before, body)
                    path.write_text(body.replace(before, after))
                    with self.subTest(name=name, after=after), self.assertRaises(ValueError):
                        comparison.verify_sources(old, candidate, COMMIT)
                    path.write_text(body)
                (candidate / 'Cargo.lock').write_text('changed')
                with self.assertRaisesRegex(ValueError, 'locked_inputs'):
                    comparison.verify_sources(old, candidate, COMMIT)

    def test_source_verification_rejects_dirty_wrong_revisions_and_unapproved_launcher(self):
        with tempfile.TemporaryDirectory() as temporary:
            old, candidate = self.source_fixture(Path(temporary))
            clean = self.source_git(old, candidate)
            for bad in ('dirty', 'wrong-head', 'wrong-tree'):
                def git(args):
                    if bad == 'dirty' and args[-2:] == ['status', '--porcelain']:
                        return ' M changed-file'
                    if bad == 'wrong-head' and args[-1] == 'HEAD':
                        return 'e' * 40
                    if bad == 'wrong-tree' and args[-1] == 'HEAD^{tree}':
                        return 'invalid'
                    return clean(args)
                with self.subTest(bad=bad), patch.object(comparison.cost, 'run', side_effect=git), \
                        self.assertRaises(ValueError):
                    comparison.verify_sources(old, candidate, COMMIT)
            with patch.object(comparison.cost, 'run', side_effect=clean), \
                    patch.object(comparison.cost, 'native_identity', side_effect=self.native_fixture):
                with self.assertRaises(ValueError):
                    comparison.verify_sources(old, candidate, comparison.OLD_REVISION)
                launcher = candidate / 'deploy/docker/operator/catalog-prepare'
                launcher.write_text(launcher.read_text().replace('"$@"', '$@'))
                with self.assertRaisesRegex(ValueError, 'launcher'):
                    comparison.verify_sources(old, candidate, COMMIT)

    def test_source_verification_requires_both_ancestors_and_valid_native_identity(self):
        with tempfile.TemporaryDirectory() as temporary:
            old, candidate = self.source_fixture(Path(temporary))
            with patch.object(comparison.cost, 'run', side_effect=self.source_git(old, candidate)) as run, \
                    patch.object(comparison.cost, 'native_identity', side_effect=self.native_fixture):
                comparison.verify_sources(old, candidate, COMMIT)
                for ancestor in (comparison.OLD_REVISION, comparison.PRIOR_PACKAGING_REVISION):
                    self.assertIn(unittest.mock.call(['git', '-C', str(candidate), 'merge-base', '--is-ancestor', ancestor, COMMIT]), run.call_args_list)
            with patch.object(comparison.cost, 'run', side_effect=self.source_git(old, candidate)), \
                    patch.object(comparison.cost, 'native_identity', return_value={'schema_version': 1}), \
                    self.assertRaisesRegex(ValueError, 'malformed identity'):
                comparison.verify_sources(old, candidate, COMMIT)

    def test_source_verification_does_not_normalize_common_recipe_bytes(self):
        with tempfile.TemporaryDirectory() as temporary:
            old, candidate = self.source_fixture(Path(temporary))
            with patch.object(comparison.cost, 'run', side_effect=self.source_git(old, candidate)), \
                    patch.object(comparison.cost, 'native_identity', side_effect=self.native_fixture):
                for name in ('Dockerfile', 'native-build.sh'):
                    path = candidate / 'deploy/docker' / name
                    original = path.read_bytes()
                    path.write_bytes(original.replace(b'\n', b'\r\n'))
                    with self.subTest(name=name), self.assertRaises(ValueError):
                        comparison.verify_sources(old, candidate, COMMIT)
                    path.write_bytes(original)

    def test_preflight_writes_failure_evidence_without_building(self):
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary) / 'preflight.json'
            arguments = ['operator_compare.py', 'verify-sources', '--old-source', 'old',
                         '--candidate-source', 'candidate', '--revision', COMMIT, '--output', str(output)]
            with patch.object(sys, 'argv', arguments), \
                    patch.object(comparison, 'harness_identity', return_value={'revision': COMMIT}), \
                    patch.object(comparison, 'verify_sources', side_effect=ValueError('changed common input')), \
                    patch.object(comparison, 'measured_build') as build, \
                    self.assertRaises(SystemExit) as stopped:
                comparison.main()
            self.assertEqual(stopped.exception.code, 1)
            build.assert_not_called()
            report = json.loads(output.read_text())
            self.assertEqual(report['status'], 'blocked')
            self.assertIn('changed common input', report['error'])

    def test_measurement_refuses_a_local_native_build(self):
        with patch.dict('os.environ', {}, clear=True), patch.object(comparison.cost, 'run') as run:
            with self.assertRaisesRegex(ValueError, 'GitHub-hosted'):
                comparison.measure(Path('old'), Path('candidate'), COMMIT, 'candidate', Path('out'), 'ci', BUILDKIT)
            run.assert_not_called()

    def test_binutils_diagnostics_retain_original_binary_and_hash_actual_sections(self):
        original_run = comparison.cost.run
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary) / 'candidate-elf'
            resources = {'containers': {}}
            def run(args):
                if args[:2] == ['docker', 'create']:
                    return SHA
                if args[:2] == ['docker', 'cp']:
                    shutil.copyfile('/usr/bin/true', args[-1])
                    return ''
                return original_run(args)
            with patch.object(comparison.cost, 'run', side_effect=run):
                result = comparison.extract_application(directory, 'sha256:' + SHA, 'owned', resources)
            for name in ('server', 'runtime'):
                self.assertEqual((directory / name).read_bytes(), Path('/usr/bin/true').read_bytes())
                self.assertEqual(result[name]['sha256'], hashlib.sha256(Path('/usr/bin/true').read_bytes()).hexdigest())
                self.assertIn('.text', result[name]['sections'])
                self.assertIn('Program Headers', (directory / (name + '-readelf.txt')).read_text())

    def test_cleanup_preserves_reassigned_resources_and_keeps_every_failure(self):
        owner = 'operator-measure-' + '1' * 32
        resources = {'owner': owner, 'builder_image': BUILDKIT, 'builder_id': SHA, 'builder_created': True,
                     'containers': {owner + '-extract-candidate-elf': SHA}, 'images': {'quazonai-operator-measure:' + owner + '-full': 'sha256:' + SHA},
                     'diagnosed_images': ['quazonai-operator-measure:' + owner + '-full']}
        for reassigned in (False, True):
            calls = []
            def run(args):
                calls.append(args)
                identity = 'f' * 64 if reassigned else SHA
                if args[:3] == ['docker', 'container', 'inspect']:
                    return json.dumps([{'Id': identity, 'Config': {'Image': BUILDKIT, 'Labels': {'quazonai.measurement': owner}}}])
                if args[:3] == ['docker', 'buildx', 'inspect']:
                    return 'Name: ' + owner + '\nDriver: docker-container'
                if args[:3] == ['docker', 'image', 'inspect']:
                    return 'sha256:' + identity
                return ''
            with tempfile.TemporaryDirectory() as temporary, patch.object(comparison.cost, 'run', side_effect=run):
                result = comparison.cleanup_owned(Path(temporary), resources)
            self.assertEqual(result['status'], 'blocked' if reassigned else 'complete')
            self.assertEqual(len(result['errors']), 3 if reassigned else 0)
            self.assertEqual(len(result['removed']), 0 if reassigned else 3)
            self.assertFalse(any('prune' in args for args in calls))
            if reassigned:
                self.assertFalse(any('rm' in args for args in calls))

    def test_cleanup_never_removes_unowned_names_even_with_matching_ids(self):
        resources = {'owner': 'operator-measure-' + '1' * 32, 'builder_image': BUILDKIT, 'builder_id': None,
                     'containers': {'unrelated-container': SHA}, 'images': {'unrelated:tag': 'sha256:' + SHA}}
        with tempfile.TemporaryDirectory() as temporary, patch.object(comparison.cost, 'run') as run:
            result = comparison.cleanup_owned(Path(temporary), resources)
        self.assertEqual(result['status'], 'blocked')
        self.assertEqual(len(result['errors']), 2)
        run.assert_not_called()

    def test_owned_cleanup_accounts_for_archive_alias_outcomes_and_final_identity(self):
        resources = {'owner': 'operator-measure-' + '1' * 32, 'builder_id': None, 'containers': {}, 'images': {}}
        record = {'tag': 'quazonai-bundle/application:ci', 'original_id': None, 'status': 'complete',
                  'measured_id': 'sha256:' + SHA, 'observed_before_cleanup': 'sha256:' + SHA,
                  'observed_after_cleanup': None, 'outcome': 'removed'}
        for state, current in (('complete', ''), ('blocked', ''), ('complete', 'sha256:' + SHA)):
            with self.subTest(state=state, current=current), tempfile.TemporaryDirectory() as temporary:
                directory = Path(temporary)
                record['status'] = state
                comparison.write_json(directory / 'archive-tags.json', [record])
                with patch.object(comparison.cost, 'run', return_value=current):
                    result = comparison.cleanup_owned(directory, resources)
                self.assertEqual(result['archive_tags'][0], record)
                if state == 'complete' and not current:
                    self.assertEqual(result['status'], 'complete')
                else:
                    self.assertEqual(result['status'], 'blocked')
                    self.assertEqual(result['errors'][0]['kind'], 'archive-alias')
                self.assertEqual(json.loads((directory / 'cleanup.json').read_text()), result)

    def test_final_alias_readback_uncertainty_blocks_the_next_variant(self):
        import operator_cost_test
        fixture = operator_cost_test.CostTests()
        payload, expected, application = fixture.native_fixture()
        record = fixture.cleanup_record()
        resources = {'owner': 'operator-measure-' + '1' * 32, 'builder_id': None, 'containers': {}, 'images': {}}
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            old, candidate = root / 'old', root / 'candidate'
            old.mkdir(); candidate.mkdir()
            journal = root / 'archive-tags.json'
            comparison.write_json(journal, [record])
            with patch.object(comparison.cost, 'run', return_value='sha256:' + 'b' * 64):
                cleanup = comparison.cleanup_owned(old, resources, journal)
            self.assertEqual(cleanup['status'], 'blocked')
            retained = json.loads(journal.read_text())
            self.assertEqual(retained[0], record)
            self.assertEqual(retained[-1]['status'], 'blocked')
            self.assertEqual(retained[-1]['phase'], 'post-measurement-cleanup')
            for index, phase in enumerate(('candidate-start', 'candidate-end', 'baseline-start', 'baseline-end')):
                (candidate / (phase + '.json')).write_text(json.dumps({'monotonic_ns': index}))
            baseline = {'id': 'sha256:' + SHA, 'size_bytes': 1000, 'application_elf_sha256': application,
                        'stripped_application_binary_bytes': {'server': 100, 'runtime': 200}}
            full = {**copy.deepcopy(baseline), 'size_bytes': 1500, 'payload': payload}
            with patch.object(comparison.cost, 'native_identity', return_value=expected), \
                    patch.object(comparison.cost, 'measure_image', side_effect=[baseline, full]) as images, \
                    patch.object(comparison.cost, 'archive_bytes') as archive:
                result = comparison.cost.report(candidate, 'base', 'full', COMMIT, 'ci', emit=False,
                                                strict=False, cleanup_evidence=journal)
            images.assert_called()
            self.assertEqual(images.call_count, 2)
            archive.assert_not_called()
            self.assertTrue(result['validation_errors'])
            self.assertEqual(result['payload'], payload)
            self.assertEqual(result['candidate']['application_elf_sha256'], application)
            self.assertEqual(json.loads(journal.read_text()), retained)

    def test_resumed_pair_preserves_uncertain_preexisting_variant_journals(self):
        for state in ('pending', 'blocked', 'malformed'):
            with self.subTest(state=state), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                old, candidate = root / 'old', root / 'candidate'
                old.mkdir(); candidate.mkdir()
                legacy = old / 'archive-tags.json'
                body = '[' if state == 'malformed' else json.dumps([{'status': state}])
                legacy.write_text(body)
                shared = comparison.pair_archive_evidence(candidate)
                self.assertEqual(shared, root / 'archive-tags.json')
                retained = shared.read_bytes()
                self.assertFalse(comparison.cost.archive_cleanup_confirmed(shared))
                self.assertEqual(comparison.pair_archive_evidence(old), shared)
                self.assertEqual(shared.read_bytes(), retained)
                self.assertEqual(legacy.read_text(), body)
                with patch.object(comparison.cost, 'run') as docker, \
                        patch.object(comparison.cost.subprocess, 'Popen') as save, \
                        self.assertRaisesRegex(ValueError, 'unconfirmed'):
                    comparison.cost.archive_bytes('sha256:' + SHA, 'ci', shared)
                docker.assert_not_called()
                save.assert_not_called()

    def measure_fixture(self, directory, variant='candidate', build_failure=False, cache=EMPTY_CACHE, free=50_000_000_000, extraction_failure=False, report_cleanup_paths=None):
        old, candidate = self.reports()
        reports = {'old': old, 'candidate': candidate}
        environment = copy.deepcopy(reports[variant]['hosted_comparison']['environment'])
        calls, builds = [], []
        def run(args):
            calls.append(args)
            if args[:3] == ['docker', 'info', '--format']:
                return str(directory)
            if args[:3] in (['docker', 'image', 'ls'], ['docker', 'buildx', 'ls']):
                return ''
            if args[:3] == ['docker', 'container', 'inspect']:
                return json.dumps([{'Id': SHA, 'Config': {'Image': BUILDKIT}}])
            if args[:3] == ['docker', 'buildx', 'inspect']:
                return 'Name: ' + args[-1] + '\nDriver: docker-container'
            if args[:3] == ['docker', 'buildx', 'du']:
                return cache
            if args[:3] == ['docker', 'image', 'inspect']:
                return 'sha256:' + SHA
            return 'observed fixture'
        def build(args, **kwargs):
            builds.append(args)
            if build_failure:
                raise subprocess.TimeoutExpired(args, 1)
        def environment_identity(builder, builder_image):
            environment['builder_name'] = builder
            return environment
        diagnostics = {name: {'sha256': SHA, 'size_bytes': size} for name, size in (('server', 100), ('runtime', 200))}
        with patch.dict('os.environ', {'GITHUB_ACTIONS': 'true', 'RUNNER_ENVIRONMENT': 'github-hosted'}), \
                patch.object(comparison.cost.shutil, 'disk_usage', return_value=shutil._ntuple_diskusage(100_000_000_000, 0, free)), \
                patch.object(comparison, 'harness_identity', return_value=candidate['hosted_comparison']['harness']), \
                patch.object(comparison, 'verify_sources', return_value=[old['hosted_comparison']['recipe'], candidate['hosted_comparison']['recipe']]), \
                patch.object(comparison, 'environment_identity', side_effect=environment_identity), \
                patch.object(comparison.cost, 'run', side_effect=run), \
                patch.object(comparison.subprocess, 'run', side_effect=build), \
                patch.object(comparison, 'extract_application', return_value=diagnostics,
                             side_effect=ValueError('Incomplete extraction') if extraction_failure else None), \
                patch.object(comparison.cost, 'image_identity', return_value={'id': 'sha256:' + SHA}), \
                patch.object(comparison.cost, 'report', return_value=copy.deepcopy(reports[variant])) as report:
            result = comparison.measure(Path('old'), Path('candidate'), COMMIT, variant, directory, 'ci', BUILDKIT, time.time() + 600)
            if report_cleanup_paths is not None:
                report_cleanup_paths.append(report.call_args.kwargs['cleanup_evidence'])
        return result, calls, builds

    def test_sequential_variants_use_distinct_empty_builders_same_flags_and_cleanup(self):
        with tempfile.TemporaryDirectory() as temporary:
            owners, cleanup_paths = [], []
            for variant in ('old', 'candidate'):
                result, calls, builds = self.measure_fixture(Path(temporary) / variant, variant, report_cleanup_paths=cleanup_paths)
                self.assertEqual(result['measurement_status'], 'complete', result)
                self.assertEqual(result['schema_version'], comparison.REPORT_SCHEMA)
                self.assertEqual(len(builds), 3)
                self.assertEqual(builds[0], builds[1])
                self.assertEqual(builds[2][builds[2].index('--target') + 1], 'application-base')
                owner = builds[0][builds[0].index('--builder') + 1]
                owners.append(owner)
                self.assertTrue(all(args[args.index('--builder') + 1] == owner for args in builds))
                self.assertTrue(all(not any(arg.startswith('--cache') or arg == '--no-cache' for arg in args) for args in builds))
                self.assertIn(['docker', 'buildx', 'rm', owner], calls)
                self.assertEqual(len(result['cleanup']['removed']), 3)
            self.assertNotEqual(*owners)
            self.assertEqual(cleanup_paths, [Path(temporary) / 'archive-tags.json'] * 2)

    def test_failed_build_keeps_samples_and_removes_only_owned_builder(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            result, calls, builds = self.measure_fixture(directory, build_failure=True)
            self.assertEqual(result['measurement_status'], 'blocked')
            self.assertIn('TimeoutExpired', result['error'])
            self.assertEqual(len(result['cleanup']['removed']), 1)
            for phase in ('candidate-start', 'candidate-end'):
                self.assertIn('disk_free_bytes', result['phase_observations'][phase])
            self.assertFalse(any(args[:3] == ['docker', 'image', 'rm'] for args in calls))

    def test_incomplete_elf_extraction_preserves_image_for_failure_diagnosis(self):
        with tempfile.TemporaryDirectory() as temporary:
            result, calls, builds = self.measure_fixture(Path(temporary), extraction_failure=True)
            self.assertEqual(result['measurement_status'], 'blocked')
            self.assertEqual(result['cleanup']['status'], 'blocked')
            self.assertIn('image preserved', result['cleanup']['errors'][0]['error'])
            self.assertFalse(any(args[:3] == ['docker', 'image', 'rm'] for args in calls))
            self.assertTrue(any(args[:3] == ['docker', 'buildx', 'rm'] for args in calls))

    def test_low_disk_and_nonempty_cache_block_before_building(self):
        for values in ({'free': comparison.MIN_FREE_BYTES - 1}, {'cache': EMPTY_CACHE.replace('0B', '1B')}, {'cache': ''}):
            with tempfile.TemporaryDirectory() as temporary:
                result, calls, builds = self.measure_fixture(Path(temporary), **values)
                self.assertEqual(result['measurement_status'], 'blocked')
                self.assertFalse(builds)
                self.assertFalse(any('prune' in args for args in calls))
                if 'free' in values:
                    self.assertFalse(any('create' in args for args in calls))

    def test_workflow_is_one_bounded_pair_and_only_harness_changes_trigger(self):
        workflow = Path(comparison.__file__).resolve().parents[2] / '.github/workflows/operator-cost-comparison.yml'
        body = workflow.read_text()
        paths = body.split('    paths:\n')[1].split('  workflow_dispatch:')[0]
        self.assertEqual([line.strip() for line in paths.splitlines() if line.strip()], [
            "- '.github/workflows/operator-cost-comparison.yml'", "- 'deploy/docker/operator_cost*'", "- 'deploy/docker/operator_compare*'"])
        self.assertIn('timeout-minutes: 95', body)
        self.assertIn('for variant in old candidate', body)
        self.assertIn('85 * 60', body)
        self.assertNotIn('matrix:', body)
        self.assertNotIn('prune', body)
        self.assertIn('if: always()', body)
        self.assertIn('ref: ' + comparison.OLD_REVISION, body)
        self.assertIn('test "$CANDIDATE_REVISION" != ' + comparison.OLD_REVISION, body)
        self.assertNotIn('bfa3cfc', body)
        self.assertIn('HARNESS_REVISION: ${{ github.event.pull_request.head.sha || inputs.candidate_revision }}', body)
        self.assertIn('fetch-depth: 0', body)
        self.assertLess(body.index('operator_compare.py verify-sources'), body.index('docker/setup-buildx-action'))


if __name__ == '__main__':
    unittest.main()
