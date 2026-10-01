"""Dependency cache policy/key/pruning contracts; service acceptance stays in smoke.py."""
from contextlib import contextmanager
import importlib.util
import os
from pathlib import Path
import subprocess
import tempfile
import tomllib
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
ACTION = ROOT / '.github/actions/container'
spec = importlib.util.spec_from_file_location('native_cache', ACTION / 'native-cache.py')
cache = importlib.util.module_from_spec(spec)
spec.loader.exec_module(cache)


@contextmanager
def cwd(path):
    old = Path.cwd()
    os.chdir(path)
    try:
        yield
    finally:
        os.chdir(old)


class NativeCacheTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix='quazonai-cache-test-')
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.source = self.root / 'source with spaces'
        self.source.mkdir()
        for name, contents in {'Cargo.toml': '[workspace]\nmembers=["app"]\n',
                               'Cargo.lock': 'version=4\n', 'rust-toolchain.toml': '[toolchain]\nchannel="1.98.1"\n',
                               'app/Cargo.toml': '[package]\nname="job"\nversion="0.1.0"\n'}.items():
            path = self.source / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(contents)
        self.env = {'GITHUB_WORKSPACE': str(self.root), 'GITHUB_EVENT_NAME': 'push',
                    'GITHUB_REF': 'refs/heads/dev', 'GITHUB_REPOSITORY': 'owner/repo',
                    'RUNNER_OS': 'Linux', 'RUNNER_ARCH': 'X64', 'ImageOS': 'ubuntu24', 'ImageVersion': '20260927.1',
                    'CARGO_BUILD_JOBS': '2', 'CARGO_PROFILE_RELEASE_DEBUG': '0',
                    'CARGO_HOME': str(self.root / 'cargo-home'), 'GITHUB_OUTPUT': str(self.root / 'output')}
        self.compiler = {'rustc': 'rustc 1.98.1 (full identity)', 'cargo': 'cargo 1.98.1', 'cc': 'gcc 14'}

    def key(self, env=None, compiler=None):
        with patch.object(cache, 'run', return_value='app/Cargo.toml\0'), patch.object(cache, 'config_files', return_value=[]):
            return cache.cache_key(self.source, self.env if env is None else env, self.compiler if compiler is None else compiler)

    def test_dev_only_and_fork_restore_only(self):
        self.assertEqual(cache.policy(self.env), (True, True))
        for ref in ['refs/heads/main', 'refs/heads/feature', 'refs/tags/v1.0.0']:
            self.assertEqual(cache.policy({**self.env, 'GITHUB_REF': ref}), (False, False))
        pr = {**self.env, 'GITHUB_EVENT_NAME': 'pull_request', 'GITHUB_BASE_REF': 'dev', 'CACHE_HEAD_REPOSITORY': 'owner/repo'}
        self.assertEqual(cache.policy(pr), (True, True))
        self.assertEqual(cache.policy({**pr, 'CACHE_HEAD_REPOSITORY': 'fork/repo'}), (True, False))
        self.assertEqual(cache.policy({**pr, 'CACHE_HEAD_REPOSITORY': ''}), (True, False))
        self.assertEqual(cache.policy({**pr, 'GITHUB_BASE_REF': 'main'}), (False, False))
        for event in ['workflow_dispatch', 'workflow_call', 'pull_request_target']:
            self.assertEqual(cache.policy({**pr, 'GITHUB_EVENT_NAME': event}), (False, False))

    def test_key_binds_every_manifest_lock_and_toolchain_but_not_source(self):
        original = self.key()
        self.assertRegex(original, r'^quazonai-native-job-default-release-v1-[a-f0-9]{64}$')
        for name in ['Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml', 'app/Cargo.toml']:
            path = self.source / name
            before = path.read_text()
            path.write_text(before + '\n# changed\n')
            self.assertNotEqual(self.key(), original, name)
            path.write_text(before)
        (self.source / 'app/main.rs').write_text('fn main() {}')
        self.assertEqual(self.key(), original)
        self.assertEqual(self.key(env={**self.env, 'GITHUB_SHA': 'new-head'}), original)

    def test_key_binds_runner_compiler_profile_flags(self):
        original = self.key()
        for name in ['RUNNER_OS', 'RUNNER_ARCH', 'ImageOS', 'ImageVersion', 'CARGO_BUILD_JOBS',
                     'CARGO_PROFILE_RELEASE_DEBUG', 'CARGO_HOME', 'RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS', 'CC', 'CFLAGS']:
            self.assertNotEqual(self.key(env={**self.env, name: 'changed'}), original, name)
        self.assertNotEqual(self.key(compiler={**self.compiler, 'rustc': 'different compiler'}), original)
        self.assertEqual(self.key(env={**self.env, 'GITHUB_TOKEN': 'never-key-or-log-this'}), original)

    def test_missing_hash_inputs_and_unsupported_layout_skip(self):
        for name in ['RUNNER_OS', 'RUNNER_ARCH', 'ImageOS', 'ImageVersion']:
            with self.subTest(name=name), self.assertRaises(ValueError):
                self.key(env={**self.env, name: ''})
        for name in cache.LAYOUT_ENV:
            with self.subTest(name=name), self.assertRaises(ValueError):
                self.key(env={**self.env, name: '/different/target'})
        with self.assertRaises(ValueError):
            self.key(compiler={'rustc': ''})
        (self.source / 'Cargo.lock').write_text('')
        with self.assertRaises(ValueError):
            self.key()

    def test_cargo_configuration_is_hashed_and_layout_override_rejected(self):
        config = self.root / 'config.toml'
        config.write_text('[build]\nrustflags=["-C", "opt-level=2"]\n')
        with patch.object(cache, 'run', return_value='app/Cargo.toml\0'), patch.object(cache, 'config_files', return_value=[config]):
            original = cache.cache_key(self.source, self.env, self.compiler)
            config.write_text('[build]\nrustflags=["-C", "opt-level=3"]\n')
            self.assertNotEqual(cache.cache_key(self.source, self.env, self.compiler), original)
            for setting in ['target', 'target-dir', 'build-dir']:
                config.write_text(f'[build]\n{setting}="elsewhere"\n')
                with self.assertRaises(ValueError):
                    cache.cache_key(self.source, self.env, self.compiler)

    def test_source_paths_dot_nested_and_spaces_and_escape(self):
        for source in [self.root, self.source]:
            with cwd(source):
                self.assertEqual(cache.source_root(self.env), source)
        with cwd(self.root.parent), self.assertRaises(ValueError):
            cache.source_root(self.env)
        (self.source / 'target').symlink_to(self.root)
        with cwd(self.source), self.assertRaises(ValueError):
            cache.source_root(self.env)
        unsafe = self.root / 'source[glob]'
        unsafe.mkdir()
        with cwd(unsafe), self.assertRaises(ValueError):
            cache.source_root(self.env)

    def test_size_ceiling_and_archive_headroom(self):
        limit, reserve = cache.MAX_BYTES, cache.HEADROOM_BYTES
        for size, free, allowed in [(0, limit * 3, False), (limit, limit + reserve, True),
                                    (limit + 1, limit * 3, False), (1, reserve, False),
                                    (100, reserve + 100, True)]:
            self.assertEqual(cache.save_allowed(size, free), allowed)

    def test_measured_dependency_tree_fits_the_bounded_three_gib_pilot(self):
        # Actual first hosted cold run after native workspace pruning; no archive
        # was saved under the initial 2 GiB ceiling.
        measured = 2_435_668_032
        self.assertEqual(cache.MAX_BYTES, 3 * 1024**3)
        self.assertGreater(measured, 2 * 1024**3)
        self.assertTrue(cache.save_allowed(measured, measured + cache.HEADROOM_BYTES))
        self.assertFalse(cache.save_allowed(measured, measured + cache.HEADROOM_BYTES - 1))

    def test_restore_headroom_includes_archive_extraction_and_reserve(self):
        required = 2 * cache.MAX_BYTES + cache.HEADROOM_BYTES
        self.assertEqual(required, 7 * 1024**3)
        for free, enabled in [(required - 1, False), (required, True)]:
            (self.root / 'output').unlink(missing_ok=True)
            with cwd(self.source), patch.dict(os.environ, self.env, clear=True), patch.object(
                cache, 'measure', return_value=(0, free)
            ), patch.object(cache, 'run', return_value='observed compiler'), patch.object(cache, 'cache_key', return_value='exact-key'):
                cache.main('prepare')
            self.assertIn(f'enabled={str(enabled).lower()}\n', (self.root / 'output').read_text())

    def test_prepare_skips_without_headroom_or_complete_identity(self):
        with cwd(self.source), patch.dict(os.environ, self.env, clear=True), patch.object(cache, 'measure', return_value=(0, 1)):
            cache.main('prepare')
        self.assertEqual((self.root / 'output').read_text(), 'enabled=false\ncan-save=false\n')
        (self.root / 'output').unlink()
        with cwd(self.source), patch.dict(os.environ, {**self.env, 'ImageVersion': ''}, clear=True), patch.object(
            cache, 'measure', return_value=(0, 10 * cache.MAX_BYTES)
        ), patch.object(cache, 'run', return_value='compiler'):
            cache.main('prepare')
        self.assertEqual((self.root / 'output').read_text(), 'enabled=false\ncan-save=false\n')

    def test_prune_uses_exact_offline_workspace_clean_and_cold_fallback(self):
        release = self.source / 'target/release'
        release.mkdir(parents=True)
        (release / 'job').write_text('old workspace executable')
        command = ['rustup', 'run', '1.98.1', 'cargo', 'clean', '--locked', '--offline', '--release', '--workspace']
        with patch.object(cache.subprocess, 'run', return_value=subprocess.CompletedProcess(command, 0)) as execute:
            cache.prune(self.source)
        execute.assert_called_once_with(command, timeout=120)
        with patch.object(cache.subprocess, 'run', return_value=subprocess.CompletedProcess(command, 1)):
            cache.prune(self.source)
        self.assertFalse(release.exists())

    def test_restore_primes_metadata_with_bounded_dry_run_before_offline_clean(self):
        release = self.source / 'target/release'
        release.mkdir(parents=True)
        with patch.object(cache.subprocess, 'run', return_value=subprocess.CompletedProcess([], 0)) as execute:
            cache.prune(self.source, prime=True)
        self.assertEqual([call.args[0] for call in execute.call_args_list], [
            ['rustup', 'run', '1.98.1', 'cargo', 'clean', '--locked', '--release', '--workspace', '--dry-run'],
            ['rustup', 'run', '1.98.1', 'cargo', 'clean', '--locked', '--offline', '--release', '--workspace']])
        self.assertTrue(all(call.kwargs == {'timeout': 120} for call in execute.call_args_list))
        with patch.object(cache.subprocess, 'run', side_effect=[subprocess.TimeoutExpired('cargo', 120), subprocess.CompletedProcess([], 0)]):
            cache.prune(self.source, prime=True)
        self.assertFalse(release.exists())

    def test_nonexact_or_unconfirmed_restore_is_discarded(self):
        for restored in ['prefix-match-only', '']:
            release = self.source / 'target/release'
            release.mkdir(parents=True)
            (release / 'job').write_text('stale output')
            with cwd(self.source), patch.dict(os.environ, {**self.env, 'CACHE_EXPECTED_KEY': 'exact-key', 'CACHE_MATCHED_KEY': restored}, clear=True), patch.object(
                cache, 'measure', return_value=(1, 100000)
            ), patch.object(cache, 'prune') as prune:
                cache.main('prune')
            prune.assert_not_called()
            self.assertFalse(release.exists())

    def test_save_check_prunes_before_measuring_and_skips_oversize(self):
        events = []
        def measure(_root, phase):
            events.append(phase)
            return cache.MAX_BYTES + 1, cache.MAX_BYTES * 10
        with cwd(self.source), patch.dict(os.environ, self.env, clear=True), patch.object(cache, 'measure', side_effect=measure), patch.object(
            cache, 'prune', side_effect=lambda _root: events.append('prune')
        ):
            cache.main('save-check')
        self.assertEqual(events, ['after image assembly', 'prune', 'before save'])
        self.assertEqual((self.root / 'output').read_text(), 'save=false\n')


class NativeCacheWorkflowTests(unittest.TestCase):
    def test_runtime_http_target_cannot_collide_with_registry_http(self):
        # Native Cargo 1.98.1 reproduction: an auto target named http makes
        # clean --workspace remove the unrelated registry crate's metadata.
        # Keep the original source/test coverage with an explicit unique name.
        manifest = tomllib.loads((ROOT / 'apps/runtime/Cargo.toml').read_text())
        targets = [target for target in manifest['test'] if target.get('path') == 'tests/http.rs']
        self.assertEqual(len(targets), 1)
        self.assertEqual(targets[0]['name'], 'runtime_http')
        self.assertNotIn('required-features', targets[0])
        self.assertTrue((ROOT / 'apps/runtime/tests/http.rs').is_file())

    def test_opt_in_preserves_independent_acceptance_and_explicit_save_order(self):
        action = (ACTION / 'action.yml').read_text()
        self.assertIn("cache-native-dependencies:\n    description:", action)
        self.assertIn("default: 'false'", action)
        markers = ['test "$(git rev-parse HEAD)" = "$EXPECTED_HEAD"',
                   'id: native-cache\n', 'uses: actions/cache/restore@',
                   'Remove all restored workspace outputs', 'cargo build --locked --release -p job',
                   'node runtimes/native/build-native-image.mjs', 'id: native-cache-save-check',
                   'uses: actions/cache/save@', 'rm -rf target/release', 'id: codex',
                   'uses: docker/build-push-action@', 'python3 -B deploy/docker/smoke.py']
        indexes = [action.index(marker) for marker in markers]
        self.assertEqual(indexes, sorted(indexes))
        self.assertEqual(action.count('actions/cache/'), 2)
        self.assertEqual(action.count('@55cc8345863c7cc4c66a329aec7e433d2d1c52a9'), 2)
        self.assertNotIn('restore-keys:', action)
        self.assertNotIn('~/.cargo', action)
        self.assertIn("steps.native-cache.outputs.can-save == 'true'", action)
        self.assertIn("steps.native-cache-save-check.outputs.save == 'true'", action)
        for step in ['Build the scientific image from the same source', 'Verify real installation, upgrade, failure retry and database restore']:
            section = action.split(f'    - name: {step}\n')[1].split('    - name:')[0]
            self.assertNotIn('      if:', section)
            self.assertIn('working-directory: ${{ inputs.source }}', section)

    def test_container_dev_events_and_dev_push_release_opt_in(self):
        workflow = (ROOT / '.github/workflows/container.yml').read_text()
        self.assertIn('permissions:\n  contents: read\n', workflow)
        self.assertIn("cache-native-dependencies: ${{ (github.event_name == 'push' && github.ref == 'refs/heads/dev') || (github.event_name == 'pull_request' && github.base_ref == 'dev') }}", workflow)
        self.assertIn('ref: ${{ github.event.pull_request.head.sha || github.sha }}', workflow)
        release = (ROOT / '.github/workflows/release-version.yml').read_text()
        self.assertEqual(release.count('cache-native-dependencies:'), 1)
        self.assertIn("cache-native-dependencies: ${{ inputs.branch == 'dev' && github.event_name == 'push' && github.ref == 'refs/heads/dev' }}", release)
        self.assertNotIn('cache-native-dependencies:', (ROOT / '.github/workflows/dev-image.yml').read_text())
        self.assertIn('source: source', (ROOT / '.github/workflows/dev-image.yml').read_text())


if __name__ == '__main__':
    unittest.main()
