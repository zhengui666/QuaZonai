"""Comparison evidence cannot silently mix revisions, recipes or cache scopes."""
import copy
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import operator_compare as comparison


class ComparisonTests(unittest.TestCase):
    def reports(self):
        recipe = {'base_images': ['same@sha256:' + 'a' * 64], 'locked_inputs_sha256': {'Cargo.lock': 'same'},
                  'rust_image': 'same-rust', 'platform': 'linux/amd64'}
        environment = {'builder_image': 'same-builder', 'buildx_version': 'same-buildx', 'rustc': 'same-rustc',
                       'runner_os': 'Linux', 'runner_arch': 'X64', 'cpu_count': 4, 'memory': 'same-memory',
                       'cpu_models': ['same-cpu'], 'builder_name': 'old-builder'}
        old = {'revision': comparison.OLD_REVISION, 'version': 'same-release-tag',
               'candidate': {'size_bytes': 1000, 'compressed_archive_bytes': 500,
                             'application_binary_sha256': {'server': 'same-server', 'runtime': 'same-runtime'}},
               'payload': {'operator_payload_bytes': 800, 'stripped_binary_bytes': {'catalog-prepare': 400, 'polymarket-history': 300},
                           'launcher_bytes': {}},
               'hosted_comparison': {'variant': 'old', 'old_revision': comparison.OLD_REVISION,
                                     'candidate_revision': 'b' * 40, 'recipe': recipe, 'environment': environment,
                                     'harness': {'revision': 'b' * 40, 'tree': 'd' * 40, 'file_sha256': {}},
                                     'runtime_packages': 'same-packages', 'independent_cold_full_build_seconds': 500,
                                     'same_builder_warm_full_build_seconds': 5}}
        candidate = copy.deepcopy(old)
        candidate['revision'] = 'b' * 40
        candidate['candidate']['size_bytes'] = 850
        candidate['candidate']['compressed_archive_bytes'] = 450
        candidate['payload'] = {'operator_payload_bytes': 650, 'stripped_binary_bytes': {'source-tools': 530},
                                'launcher_bytes': {'catalog-prepare': 50, 'polymarket-history': 50}}
        candidate['hosted_comparison'].update(variant='candidate', independent_cold_full_build_seconds=510,
                                               same_builder_warm_full_build_seconds=6)
        candidate['hosted_comparison']['environment']['builder_name'] = 'candidate-builder'
        return old, candidate

    def test_compares_old_full_with_candidate_full_and_discloses_slower_timings(self):
        old, candidate = self.reports()
        result = comparison.compare(old, candidate, 'b' * 40)
        self.assertEqual(result['delta']['full_image_bytes'], -150)
        self.assertEqual(result['delta']['full_compressed_archive_bytes'], -50)
        self.assertEqual(result['delta']['stripped_elf_bytes'], -170)
        self.assertEqual(result['delta']['launcher_bytes'], 100)
        self.assertEqual(result['build_seconds']['independent_cold_full_build_seconds']['delta'], 10)
        self.assertEqual(result['build_seconds']['same_builder_warm_full_build_seconds']['delta'], 1)
        self.assertTrue(result['required_size_reductions_observed'])
        self.assertTrue(result['application_binaries_identical'])

    def test_fails_closed_for_mixed_revision_tag_toolchain_resources_or_builder(self):
        for mutation in ('revision', 'tag', 'rustc', 'builder', 'packages', 'recipe', 'resources', 'harness'):
            with self.subTest(mutation=mutation):
                old, candidate = self.reports()
                detail = candidate['hosted_comparison']
                if mutation == 'revision':
                    candidate['revision'] = 'c' * 40
                elif mutation == 'tag':
                    candidate['version'] = 'different-tag'
                elif mutation == 'rustc':
                    detail['environment']['rustc'] = 'other-rust'
                elif mutation == 'builder':
                    detail['environment']['builder_name'] = 'old-builder'
                elif mutation == 'packages':
                    detail['runtime_packages'] = 'other-packages'
                elif mutation == 'recipe':
                    detail['recipe']['locked_inputs_sha256']['Cargo.lock'] = 'other-lock'
                elif mutation == 'resources':
                    detail['environment']['cpu_count'] = 8
                elif mutation == 'harness':
                    detail['harness']['tree'] = 'e' * 40
                with self.assertRaises(ValueError):
                    comparison.compare(old, candidate, 'b' * 40)

    def test_size_regression_is_recorded_as_failed_admission(self):
        old, candidate = self.reports()
        candidate['candidate']['compressed_archive_bytes'] = 510
        candidate['candidate']['application_binary_sha256']['server'] = 'changed-server'
        result = comparison.compare(old, candidate, 'b' * 40)
        self.assertFalse(result['required_size_reductions_observed'])
        self.assertFalse(result['application_binaries_identical'])
        self.assertEqual(result['delta']['full_compressed_archive_bytes'], 10)
        old, candidate = self.reports()
        candidate['payload']['stripped_binary_bytes']['source-tools'] = 700
        self.assertFalse(comparison.compare(old, candidate, 'b' * 40)['required_size_reductions_observed'])

    def test_measurement_refuses_a_local_native_build(self):
        with patch.dict('os.environ', {}, clear=True), patch.object(comparison.cost, 'run') as run:
            with self.assertRaisesRegex(ValueError, 'GitHub-hosted'):
                comparison.measure(Path('old'), Path('candidate'), 'b' * 40, 'candidate', Path('out'), 'ci', 'builder')
            run.assert_not_called()

    def test_source_verification_rejects_changed_lockfile_toolchain_and_optional_features(self):
        with tempfile.TemporaryDirectory() as temporary:
            old, candidate = Path(temporary) / 'old', Path(temporary) / 'candidate'
            for source, bins in ((old, '--bin catalog-prepare --bin polymarket-history'),
                                  (candidate, '--bin source-tools')):
                (source / 'deploy/docker').mkdir(parents=True)
                (source / 'apps/web').mkdir(parents=True)
                for name in ('Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml', 'apps/web/package-lock.json'):
                    (source / name).write_text('same pinned input\n')
                (source / 'deploy/docker/Dockerfile').write_text(
                    'FROM rust:1.98.1-bookworm@sha256:' + 'a' * 64 + ' AS server\n'
                    'ENV RUSTUP_TOOLCHAIN=1.98.1 CARGO_BUILD_JOBS=2\n'
                    'RUN ' + comparison.OPERATOR_RECIPE + ' ' + bins + ' && true\n')
            def git(args):
                if args[-2:] == ['status', '--porcelain']:
                    return ''
                return comparison.OLD_REVISION if args[2] == str(old) else 'b' * 40
            with patch.object(comparison.cost, 'run', side_effect=git):
                self.assertEqual(len(comparison.verify_sources(old, candidate, 'b' * 40)), 2)
                for name in ('Cargo.lock', 'rust-toolchain.toml', 'deploy/docker/Dockerfile'):
                    path = candidate / name
                    original = path.read_text()
                    path.write_text(original.replace('polymarket-history,catalog-prepare', 'catalog-prepare')
                                    if name.endswith('Dockerfile') else 'different input\n')
                    with self.subTest(name=name), self.assertRaises(ValueError):
                        comparison.verify_sources(old, candidate, 'b' * 40)
                    path.write_text(original)

    def test_cold_and_warm_builds_use_one_new_builder_without_cache_import_then_baseline(self):
        recipe = {'revision': 'b' * 40, 'base_images': ['pinned-prerequisite'], 'rust_image': 'pinned-rust'}
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            builds, docker_calls = [], []
            def run(args):
                docker_calls.append(args)
                return 'observed fixture'
            def build(args, **kwargs):
                builds.append(args)
            with patch.dict('os.environ', {'GITHUB_ACTIONS': 'true', 'RUNNER_ENVIRONMENT': 'github-hosted'}), \
                    patch.object(comparison, 'harness_identity', return_value={'revision': 'b' * 40, 'tree': 'd' * 40}), \
                    patch.object(comparison, 'verify_sources', return_value=[recipe, recipe]), \
                    patch.object(comparison.cost, 'run', side_effect=run), \
                    patch.object(comparison.subprocess, 'run', side_effect=build), \
                    patch.object(comparison.cost, 'image_identity', return_value={'id': 'sha256:' + 'a' * 64}), \
                    patch.object(comparison.cost, 'report', return_value={'payload': {'stripped_binary_bytes': {'source-tools': 123}}}):
                comparison.measure(Path('old'), Path('candidate'), 'b' * 40, 'candidate', directory,
                                   'ci', 'moby/buildkit@sha256:' + 'c' * 64)
            self.assertEqual(len(builds), 3)
            self.assertEqual(builds[0], builds[1])
            self.assertNotIn('--target', builds[0])
            self.assertEqual(builds[2][builds[2].index('--target') + 1], 'application-base')
            self.assertFalse(any(arg.startswith('--cache') or arg == '--no-cache' for args in builds for arg in args))
            created = next(args for args in docker_calls if args[:3] == ['docker', 'buildx', 'create'])
            builder = created[created.index('--name') + 1]
            self.assertEqual(created[created.index('--driver') + 1], 'docker-container')
            self.assertTrue(all(args[args.index('--builder') + 1] == builder for args in builds))
            self.assertEqual(docker_calls[-1], ['docker', 'buildx', 'rm', builder])
            self.assertEqual(json.loads((directory / 'report.json').read_text())['hosted_comparison']['shipped_elf_count'], 1)

    def test_failed_build_still_records_disk_and_time_samples(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            with patch.object(comparison.subprocess, 'run', side_effect=RuntimeError('build failed')):
                with self.assertRaisesRegex(RuntimeError, 'build failed'):
                    comparison.measured_build(directory, 'candidate', ['no-build'])
            for phase in ('candidate-start', 'candidate-end'):
                self.assertIn('disk_free_bytes', json.loads((directory / (phase + '.json')).read_text()))
            self.assertGreaterEqual(comparison.elapsed(directory, 'candidate'), 0)


if __name__ == '__main__':
    unittest.main()
