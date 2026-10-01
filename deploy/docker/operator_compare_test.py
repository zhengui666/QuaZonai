"""Historical qualification fails closed while retaining useful observations."""
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
                  'common_recipe_sha256': SHA,
                  'operator_build_command': comparison.OPERATOR_RECIPE + ' --bin catalog-prepare --bin polymarket-history'}
        environment = dict.fromkeys(('buildx_version', 'rustc', 'runner_os', 'runner_arch', 'memory',
                                    'docker_version', 'runner_image', 'runner_image_version',
                                    'runner_name', 'run_id', 'run_attempt'), 'same-observation')
        environment.update(builder_image=BUILDKIT, cpu_count=4, cpu_models=['same-cpu'], builder_name='old-builder',
                           runner_boot_id='11111111-2222-3333-4444-555555555555',
                           initial_cache=EMPTY_CACHE, initial_cache_empty=True,
                           containerd={'Name': 'containerd', 'Version': '1.0', 'Details': {'GitCommit': 'same-commit'}})
        full = {'size_bytes': 1000, 'compressed_archive_bytes': 500,
                'application_binary_sha256': {'server': SHA, 'runtime': SHA},
                'stripped_application_binary_bytes': {'server': 100, 'runtime': 200}}
        old = {'schema_version': 1, 'revision': comparison.OLD_REVISION, 'version': 'same-release-tag',
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
        candidate['candidate'].update(size_bytes=850, compressed_archive_bytes=450)
        candidate['payload'] = {'operator_payload_bytes': 650, 'stripped_binary_bytes': {'source-tools': 530},
                                'launcher_bytes': {'catalog-prepare': 50, 'polymarket-history': 50}}
        candidate['hosted_comparison'].update(variant='candidate', independent_cold_full_build_seconds=510,
                                               same_builder_warm_full_build_seconds=6)
        candidate['hosted_comparison']['environment']['builder_name'] = 'candidate-builder'
        candidate['hosted_comparison']['recipe'].update(revision=COMMIT, operator_build_command=comparison.OPERATOR_RECIPE + ' --bin source-tools')
        return old, candidate

    def test_actual_size_reductions_and_slower_timings(self):
        result = comparison.compare(*self.reports(), COMMIT)
        self.assertTrue(result['admissible'], result['reasons'])
        self.assertEqual(result['delta'], {'full_image_bytes': -150, 'full_compressed_archive_bytes': -50,
                                          'operator_payload_bytes': -150, 'stripped_elf_bytes': -170, 'launcher_bytes': 100})
        self.assertEqual(result['build_seconds']['independent_cold_full_build_seconds']['delta'], 10)
        self.assertEqual(result['build_seconds']['same_builder_warm_full_build_seconds']['delta'], 1)
        self.assertTrue(result['application_binaries_identical'])

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
        candidate['candidate']['application_binary_sha256']['server'] = 'f' * 64
        del candidate['candidate']['application_binary_sha256']['runtime']
        candidate['candidate']['compressed_archive_bytes'] = 510
        result = comparison.compare(old, candidate, COMMIT)
        self.assertFalse(result['admissible'])
        fields = {reason['field'] for reason in result['reasons']}
        for suffix in ('revision', 'memory', 'cpu_models', 'runner_image_version', 'docker_version', 'containerd',
                       'initial_cache_empty', 'application_binary_sha256.server', 'application_binary_sha256.runtime',
                       'application_binaries_identical', 'full_compressed_archive_bytes'):
            self.assertTrue(any(field.endswith(suffix) for field in fields), suffix)
        self.assertIs(result['candidate'], candidate)
        self.assertEqual(result['build_seconds']['independent_cold_full_build_seconds']['candidate'], 510)

    def test_binary_identity_alone_blocks_admission_and_false_equal_values_are_invalid(self):
        for replacement in ({'server': 'f' * 64, 'runtime': SHA}, False, {}, {'server': False, 'runtime': False}):
            old, candidate = self.reports()
            candidate['candidate']['application_binary_sha256'] = replacement
            if replacement is False:
                old['candidate']['application_binary_sha256'] = False
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
            candidate['candidate']['application_binary_sha256']['server'] = 'f' * 64
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

    def test_source_verification_rejects_compiler_strip_feature_lock_and_recipe_changes(self):
        dockerfile = Path(comparison.__file__).with_name('Dockerfile').read_text()
        old_dockerfile = dockerfile.replace('--bin source-tools', '--bin catalog-prepare --bin polymarket-history').replace(
            'install -Dm755 target/release/source-tools /operator/bin/source-tools',
            'install -Dm755 target/release/catalog-prepare /operator/bin/catalog-prepare \\\n && install -Dm755 target/release/polymarket-history /operator/bin/polymarket-history').replace(
            'strip /operator/bin/source-tools', 'strip /operator/bin/catalog-prepare /operator/bin/polymarket-history').replace(
            ' && install -m755 deploy/docker/operator/catalog-prepare deploy/docker/operator/polymarket-history /operator/bin/ \\\n', '')
        with tempfile.TemporaryDirectory() as temporary:
            old, candidate = Path(temporary) / 'old', Path(temporary) / 'candidate'
            for source, body in ((old, old_dockerfile), (candidate, dockerfile)):
                (source / 'deploy/docker').mkdir(parents=True)
                (source / 'apps/web').mkdir(parents=True)
                for name in comparison.LOCKED_INPUTS:
                    (source / name).write_text('same pinned input\n')
                (source / 'deploy/docker/Dockerfile').write_text(body)
            def git(args):
                return '' if args[-2:] == ['status', '--porcelain'] else comparison.OLD_REVISION if args[2] == str(old) else COMMIT
            with patch.object(comparison.cost, 'run', side_effect=git):
                comparison.verify_sources(old, candidate, COMMIT)
                for before, after in (('CARGO_PROFILE_RELEASE_DEBUG=0', 'RUSTFLAGS=-Copt-level=0 CARGO_PROFILE_RELEASE_DEBUG=0'),
                                      ('-p server -p runtime', '-p server -p runtime --features something'),
                                      ('strip /out/server', 'strip --strip-debug /out/server'),
                                      ('polymarket-history,catalog-prepare', 'catalog-prepare')):
                    (candidate / 'deploy/docker/Dockerfile').write_text(dockerfile.replace(before, after))
                    with self.subTest(after=after), self.assertRaises(ValueError):
                        comparison.verify_sources(old, candidate, COMMIT)
                (candidate / 'deploy/docker/Dockerfile').write_text(dockerfile)
                (candidate / 'Cargo.lock').write_text('changed')
                with self.assertRaises(ValueError):
                    comparison.verify_sources(old, candidate, COMMIT)

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
        record = {'tag': 'quazonai-bundle/application:ci', 'original_id': None, 'status': 'complete'}
        for state, current in (('complete', ''), ('blocked', ''), ('complete', 'sha256:' + SHA)):
            with self.subTest(state=state, current=current), tempfile.TemporaryDirectory() as temporary:
                directory = Path(temporary)
                record['status'] = state
                comparison.write_json(directory / 'archive-tags.json', [record])
                with patch.object(comparison.cost, 'run', return_value=current):
                    result = comparison.cleanup_owned(directory, resources)
                self.assertEqual(result['archive_tags'], [record])
                if state == 'complete' and not current:
                    self.assertEqual(result['status'], 'complete')
                else:
                    self.assertEqual(result['status'], 'blocked')
                    self.assertEqual(result['errors'][0]['kind'], 'archive-alias')
                self.assertEqual(json.loads((directory / 'cleanup.json').read_text()), result)

    def measure_fixture(self, directory, variant='candidate', build_failure=False, cache=EMPTY_CACHE, free=50_000_000_000, extraction_failure=False):
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
                patch.object(comparison.cost, 'report', return_value=copy.deepcopy(reports[variant])):
            result = comparison.measure(Path('old'), Path('candidate'), COMMIT, variant, directory, 'ci', BUILDKIT, time.time() + 600)
        return result, calls, builds

    def test_sequential_variants_use_distinct_empty_builders_same_flags_and_cleanup(self):
        with tempfile.TemporaryDirectory() as temporary:
            owners = []
            for variant in ('old', 'candidate'):
                result, calls, builds = self.measure_fixture(Path(temporary) / variant, variant)
                self.assertEqual(result['measurement_status'], 'complete', result)
                self.assertEqual(result['schema_version'], 2)
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


if __name__ == '__main__':
    unittest.main()
