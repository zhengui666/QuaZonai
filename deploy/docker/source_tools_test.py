"""Installed source isolation/compatibility; real packaged converters run in smoke.py."""
import io
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tarfile
import tempfile
import unittest
from unittest.mock import MagicMock, patch

import manage
import release
import smoke
from release_test import metadata


INVENTORY = [
    {'id': 'coinbase-candles', 'capabilities': ['plan', 'download', 'verify', 'convert', 'prepare'],
     'public_network_operations': ['download']},
    {'id': 'hf-snapshot', 'capabilities': ['plan', 'download', 'verify'],
     'public_network_operations': ['plan', 'download']},
    {'id': 'binance-vision-spot-klines', 'capabilities': ['plan', 'inspect', 'freeze', 'verify', 'convert', 'prepare'],
     'public_network_operations': []},
]


class InstalledSourceTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        self.installation = self.root / 'installation'
        self.input = self.root / 'original source [行情]'
        self.output = self.root / 'prepared output'
        for path in (self.installation, self.input, self.output):
            path.mkdir()
        self.release = metadata()
        self.config = {'root': str(self.installation), 'uid': os.getuid(), 'gid': os.getgid(),
                       'project': 'quazonai-source-test',
                       'codex_home': str(self.root / 'private-codex'), 'bundle': str(self.root / 'bundle'),
                       'unit_directory': str(self.root / 'units'), 'docker_socket': '/run/docker.sock'}
        self.calls = []
        self.addCleanup(patch.stopall)
        patch.object(manage, 'configuration', return_value=self.config).start()
        patch.object(manage, 'manifest', return_value=self.release).start()
        patch.dict(os.environ, {'DOCKER_HOST': 'unix:///run/docker.sock'}).start()
        self.run = patch.object(manage, 'run', side_effect=self.response).start()

    def response(self, args, **kwargs):
        self.calls.append(args)
        if args[:2] == ['docker', 'info']:
            return '[]'
        if args[:3] == ['docker', 'image', 'inspect']:
            return json.dumps({'org.opencontainers.image.revision': self.release['revision']})
        if args[-1] == 'plugins':
            self.assertEqual(args[args.index('--network') + 1], 'none')
            return json.dumps(INVENTORY)
        raise AssertionError(args)

    def command(self, arguments, inputs=None, output=None):
        return manage.source_command(self.installation, inputs or [], output, arguments)

    def test_registry_owns_precise_public_network_policy(self):
        for operation, plugin, expected in [('plan', 'coinbase-candles', 'none'),
                                            ('plan', 'hf-snapshot', 'bridge'),
                                            ('verify', 'hf-snapshot', 'none'),
                                            ('verify', 'coinbase-candles', 'none')]:
            with self.subTest(operation=operation, plugin=plugin):
                command = self.command([operation, plugin])
                self.assertEqual(command[command.index('--network') + 1], expected)
        for operation in ('download', 'convert', 'prepare'):
            command = self.command([operation, 'coinbase-candles', '--output', str(self.output / 'new')],
                                   [self.input], self.output)
            self.assertEqual(command[command.index('--network') + 1], 'bridge' if operation == 'download' else 'none')
        for operation in ('plan', 'inspect', 'verify', 'freeze', 'convert', 'prepare'):
            arguments = [operation, 'binance-vision-spot-klines']
            if operation in ('freeze', 'convert', 'prepare'):
                arguments += ['--output', str(self.output / 'new')]
            command = self.command(arguments, [self.input], self.output)
            self.assertEqual(command[command.index('--network') + 1], 'none')
        for arguments in (['plugins'], ['--help'], ['download', 'hf-snapshot', '--help']):
            self.calls.clear()
            command = self.command(arguments)
            self.assertEqual(command[command.index('--network') + 1], 'none')
            self.assertFalse(any(call[-1] == 'plugins' for call in self.calls))

    def test_literal_identity_mounts_and_pinned_binary_environment(self):
        command = self.command(['prepare', 'coinbase-candles', '--output', str(self.output / 'new')],
                               [self.input], self.output)
        self.assertIn(f'type=bind,source={self.input},target={self.input},readonly', command)
        self.assertIn(f'type=bind,source={self.output},target={self.output}', command)
        self.assertIn(self.release['image'], command)
        self.assertIn('QZ_OPERATOR_IMAGE=' + self.release['image'], command)
        self.assertIn('QZ_OPERATOR_INSTALLED=1', command)
        self.assertEqual(command[command.index('--user') + 1], f'{os.getuid()}:{os.getgid()}')
        self.assertEqual(command[command.index('--entrypoint') + 1], '/usr/bin/python3')
        self.assertIn('/opt/quazonai/operator/source_plugins.py', command)
        for flag in ('--read-only', '--init', '--no-healthcheck'):
            self.assertIn(flag, command)
        self.assertNotIn(str(self.installation), command)
        self.assertNotIn('/run/docker.sock', command)
        self.assertNotIn('--privileged', command)
        self.assertIn('io.quazonai.source.installation=' + self.config['project'], command)
        self.assertIn('io.quazonai.source.owner=' + str(os.getuid()), command)
        self.assertRegex(command[command.index('--name') + 1], r'^quazonai-source-[0-9a-f]{32}$')

    def test_reserved_roots_ancestors_and_state_are_rejected_before_docker(self):
        for path in [Path('/'), Path('/tmp'), Path('/opt'), Path('/usr'), Path('/proc'),
                     Path('/sys'), Path('/dev'), Path('/etc'), self.installation,
                     self.root, self.root / 'private-codex', self.root / 'units']:
            path.mkdir(exist_ok=True) if path.parent == self.root else None
            with self.subTest(path=path), self.assertRaises((ValueError, OSError)):
                self.command(['plugins'], [path])
        for reserved in ('/opt/quazonai/operator/bin', '/usr/lib', '/proc/self', '/sys/kernel', '/dev/shm'):
            with self.subTest(path=reserved), self.assertRaises((ValueError, OSError)):
                self.command(['plugins'], [Path(reserved)])
        self.run.assert_not_called()

    def test_symlink_ownership_and_mount_overlap_reject_before_docker(self):
        linked = self.root / 'linked'
        linked.symlink_to(self.input, target_is_directory=True)
        child = self.input / 'nested'
        child.mkdir()
        for inputs, output in [([linked], None), ([self.input, child], None),
                               ([self.input], self.input), ([self.input], child)]:
            with self.subTest(inputs=inputs, output=output), self.assertRaises(ValueError):
                self.command(['plugins'], inputs, output)
        with self.assertRaises(ValueError):
            manage.source_path(self.input, {**self.config, 'uid': os.getuid() + 1})
        self.run.assert_not_called()

    def test_parent_traversal_cannot_discard_a_symlink_before_validation(self):
        (self.input / 'link').symlink_to(self.root, target_is_directory=True)
        (self.input / 'data').mkdir()
        for path in (self.input / 'link' / '..' / 'data', self.output / '..' / self.output.name):
            with self.subTest(path=path), self.assertRaisesRegex(ValueError, 'parent traversal'):
                self.command(['plugins'], [path])
        with self.assertRaisesRegex(ValueError, 'parent traversal'):
            self.command(['plugins'], [], self.output / '..' / self.output.name)
        self.run.assert_not_called()

    def test_unsafe_or_unmounted_output_and_reused_artifacts_reject(self):
        reused = self.output / 'partial'
        reused.mkdir()
        sentinel = reused / 'original'
        sentinel.write_bytes(b'unchanged failed artifact')
        linked = self.output / 'link'
        linked.symlink_to(self.input, target_is_directory=True)
        for destination in [str(self.input / 'new'), 'relative', str(self.output),
                            str(self.output / '..' / 'escape'), str(reused), str(linked / 'new')]:
            with self.subTest(destination=destination), self.assertRaises(ValueError):
                self.command(['prepare', 'coinbase-candles', '--output', destination], [], self.output)
        with self.assertRaises(ValueError):
            self.command(['convert', 'coinbase-candles', '--output', str(self.output / 'new')])
        self.assertEqual(sentinel.read_bytes(), b'unchanged failed artifact')
        self.run.assert_not_called()

    def test_archive_freezing_requires_new_owned_output_before_docker(self):
        for destination, output in [(str(self.output / 'new'), None), ('relative', self.output),
                                    (str(self.input / 'new'), self.output), (str(self.output), self.output)]:
            with self.subTest(destination=destination, output=output), self.assertRaises(ValueError):
                self.command(['freeze', 'binance-vision-spot-klines', '--output', destination], [self.input], output)
        self.run.assert_not_called()

    def test_installed_archive_fixture_reproduces_original_rows_and_synthetic_clocks(self):
        data = Path(__file__).resolve().parents[2] / 'runtimes/data'
        with patch.object(sys, 'path', [str(data), *sys.path]):
            import acquire
            import binance_vision as vision
            import providers
        template, original, frozen = self.root / 'template', self.root / 'archive-original', self.root / 'frozen'
        template.mkdir()
        original.mkdir()
        smoke.source_fixture(template, acquire.plan('coinbase-candles', providers.Selection('BTC-USD', 0, 180, 60)))
        selection = vision.Selection('BTCUSDT', 'BTC', 'USDT', '2024-01-01', '1m')
        plan = vision.plan(selection)
        smoke.archive_source_fixture(original, template, plan)
        before = {path.name: path.read_bytes() for path in original.iterdir()}
        manifest = vision.freeze(selection, original / plan['archive_name'],
            original / (plan['archive_name'] + '.CHECKSUM'), frozen, provenance_path=original / 'provenance.json')
        self.assertEqual(vision.verify(frozen)['integrity'], 'VERIFIED')
        self.assertEqual(manifest['provenance_kind'], 'SYNTHETIC')
        self.assertEqual(manifest['counts']['rows'], '3')
        self.assertEqual(manifest['counts']['missing_buckets'], '1437')
        rows = [json.loads(line) for line in (frozen / 'records.jsonl').read_text().splitlines()]
        self.assertTrue(all(row['observed_at'] is None and row['historical_available_at'] is None for row in rows))
        self.assertEqual({row['declared_observed_at'] for row in rows}, {'2024-01-02T00:00:01Z'})
        self.assertEqual({row['source_timestamp_unit'] for row in rows}, {'ms'})
        self.assertEqual(before, {path.name: path.read_bytes() for path in original.iterdir()})
        native_selection = json.loads((original / 'selection.json').read_bytes())['selection']
        self.assertEqual(native_selection['bar_types'], ['BTCUSDT.BINANCE-1-MINUTE-LAST-EXTERNAL'])
        self.assertEqual(native_selection['event_start_ns'], rows[0]['event_end_ns'])
        self.assertEqual(native_selection['event_end_ns'], str(int(rows[-1]['event_end_ns']) + 60_000_000_000))

    def test_registry_cannot_give_archive_freezing_or_inspection_network_access(self):
        for operation in ('inspect', 'freeze'):
            invalid = [{'id': 'example', 'capabilities': [operation], 'public_network_operations': [operation]}]
            arguments = [operation, 'example']
            if operation == 'freeze':
                arguments += ['--output', str(self.output / 'new')]
            with self.subTest(operation=operation), patch.object(manage, 'run', side_effect=[
                    '[]', json.dumps({'org.opencontainers.image.revision': self.release['revision']}), json.dumps(invalid)]), \
                    self.assertRaisesRegex(ValueError, 'offline'):
                self.command(arguments, [self.input], self.output)

    def test_native_override_pending_update_and_remote_daemon_reject(self):
        for override in ('--native-bin', '--native-bin=/other'):
            with self.assertRaises(ValueError):
                self.command(['convert', 'coinbase-candles', override])
        (self.installation / 'pending.json').write_text('{}')
        with self.assertRaises(ValueError):
            self.command(['plugins'])
        (self.installation / 'pending.json').unlink()
        with patch.dict(os.environ, {'DOCKER_HOST': 'ssh://other'}), self.assertRaises(ValueError):
            self.command(['plugins'])
        self.run.assert_not_called()

    def test_wrong_image_revision_and_missing_network_policy_fail_closed(self):
        with patch.object(manage, 'run', side_effect=['[]', '{}']), self.assertRaises(ValueError):
            self.command(['plugins'])
        for inventory in ([{'id': 'coinbase-candles', 'capabilities': ['plan']}],
                          [{'id': 'coinbase-candles', 'capabilities': ['plan'], 'public_network_operations': ['invented']}],
                          INVENTORY + [INVENTORY[0]]):
            with patch.object(manage, 'run', side_effect=['[]', json.dumps({'org.opencontainers.image.revision': self.release['revision']}), json.dumps(inventory)]), self.assertRaises(ValueError):
                self.command(['plan', 'coinbase-candles'])
        invalid = [{'id': 'example', 'capabilities': ['verify'], 'public_network_operations': ['verify']}]
        with patch.object(manage, 'run', side_effect=['[]', json.dumps({'org.opencontainers.image.revision': self.release['revision']}), json.dumps(invalid)]), self.assertRaises(ValueError):
            self.command(['verify', 'example'])

    def test_source_cli_execs_docker_without_shell_or_detaching(self):
        command = ['docker', 'run', '--rm', 'pinned-image']
        with patch.object(manage, 'source_command', return_value=command) as prepare, patch.object(os, 'execvp') as execute:
            manage.run_source(['--directory', str(self.installation), '--read-only', str(self.input),
                               '--invocation-id', 'a' * 32, '--', 'plugins'])
        prepare.assert_called_once_with(self.installation, [self.input], None, ['plugins'], 'a' * 32)
        execute.assert_called_once_with('docker', command)

    def test_invocation_owns_both_inventory_and_actual_container(self):
        command = manage.source_command(self.installation, [], None, ['plan', 'coinbase-candles'], 'b' * 32)
        inventory = next(call for call in self.calls if call[-1] == 'plugins')
        self.assertIn('quazonai-source-' + 'b' * 32 + '-inventory', inventory)
        for arguments in (inventory, command):
            self.assertIn('io.quazonai.source.invocation=' + 'b' * 32, arguments)
        self.calls.clear()
        for invalid in ('../different', ''):
            with self.assertRaises(ValueError):
                manage.source_command(self.installation, [], None, ['plugins'], invalid)
        self.assertEqual(self.calls, [])

    def test_real_installed_preflight_rejects_reuse_without_starting_docker(self):
        bundle = Path(self.config['bundle'])
        bundle.mkdir()
        shutil.copyfile(manage.__file__, bundle / 'manage.py')
        (bundle / 'release.json').write_text(json.dumps(self.release))
        (self.installation / 'installation.json').write_text(json.dumps(self.config))
        reused = self.output / 'existing'
        reused.mkdir()
        original = reused / 'catalog-metadata.json'
        original.write_bytes(b'unchanged original publication')
        smoke.verify_source_output_reuse(self.config, self.root,
                                        ['prepare', 'coinbase-candles', '--output', str(reused)],
                                        [self.input], self.output)
        self.assertEqual(original.read_bytes(), b'unchanged original publication')
        self.assertEqual(self.calls, [])
        facts = json.loads(next((self.root / 'diagnostics').glob('*.json')).read_text())
        self.assertEqual(facts['state'], 'rejected_before_launch')
        self.assertEqual(facts['docker_calls'], 0)
        self.assertTrue(facts['container_cleanup_confirmed'])


class LegacySourceBundleTests(unittest.TestCase):
    def test_current_archive_retains_old_updater_exact_member_contract(self):
        # Frozen from the installed schema-2 updater, not computed from new code.
        old_members = {'manage.py', 'deploy.sh', 'update.sh', 'compose.yaml', 'release.json', 'README.md',
                       'codex.py', 'codex-update.sh', 'codex-login.sh', 'runtime.sh', 'codex.apparmor', '.env.example'}
        selected = metadata()
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            release.bundle(selected['version'], selected['revision'], selected['image'], root,
                           runtime_image=selected['runtime_image'], codex_version=selected['codex_version'],
                           codex_image=selected['codex_image'])
            data = (root / 'quazonai-deploy.tar.gz').read_bytes()
            with tarfile.open(fileobj=io.BytesIO(data), mode='r:gz') as archive:
                members = archive.getmembers()
                self.assertEqual(len(members), len(old_members))
                self.assertEqual({item.name for item in members}, old_members)
                self.assertTrue(all(item.isfile() and item.size <= 512_000 for item in members))
                manifest = json.load(archive.extractfile('release.json'))
                self.assertEqual(manifest, selected)
                self.assertEqual(manifest['schema_version'], 2)


class SourceSmokeRecoveryTests(unittest.TestCase):
    def setUp(self):
        self.config = {'project': 'quazonai-owned-test', 'uid': 1001,
                       'image': 'sha256:' + 'a' * 64, 'root': '/unused-installation', 'bundle': '/unused-bundle'}
        self.invocation = 'b' * 32
        self.container = 'c' * 64
        self.inspected = {'Id': self.container, 'Name': '/quazonai-source-' + self.invocation,
                          'Config': {'Image': self.config['image'], 'Labels': {
                              'io.quazonai.source.invocation': self.invocation,
                              'io.quazonai.source.installation': self.config['project'],
                              'io.quazonai.source.owner': str(self.config['uid'])}}}

    def test_timeout_without_remote_identity_remains_uncertain(self):
        with patch.object(smoke, 'source_container_ids', return_value=[]), patch.object(smoke.subprocess, 'run') as stop:
            self.assertFalse(smoke.reconcile_source_invocation(self.config, self.invocation, uncertain=True))
            self.assertTrue(smoke.reconcile_source_invocation(self.config, self.invocation, uncertain=False))
        stop.assert_not_called()

    def test_only_verified_owned_container_is_stopped_and_absence_confirmed(self):
        with patch.object(smoke, 'source_container_ids', side_effect=[[self.container], [], []]), \
                patch.object(manage, 'run', return_value=json.dumps(self.inspected)), \
                patch.object(smoke.subprocess, 'run', return_value=subprocess.CompletedProcess([], 0)) as stop:
            self.assertTrue(smoke.reconcile_source_invocation(self.config, self.invocation, uncertain=True))
        self.assertEqual(stop.call_args.args[0], ['docker', 'stop', '--time', '10', self.container])

    def test_wrong_owner_or_unconfirmed_stop_never_deletes_a_container(self):
        self.inspected['Config']['Labels']['io.quazonai.source.owner'] = 'someone-else'
        with patch.object(smoke, 'source_container_ids', return_value=[self.container]), \
                patch.object(manage, 'run', return_value=json.dumps(self.inspected)), \
                patch.object(smoke.subprocess, 'run') as stop:
            self.assertFalse(smoke.reconcile_source_invocation(self.config, self.invocation, uncertain=True))
        stop.assert_not_called()
        self.inspected['Config']['Labels']['io.quazonai.source.owner'] = str(self.config['uid'])
        with patch.object(smoke, 'source_container_ids', return_value=[self.container]), \
                patch.object(manage, 'run', return_value=json.dumps(self.inspected)), \
                patch.object(smoke.subprocess, 'run', return_value=subprocess.CompletedProcess([], 1)) as stop:
            self.assertFalse(smoke.reconcile_source_invocation(self.config, self.invocation, uncertain=True))
        self.assertEqual(stop.call_count, 1)
        self.assertEqual(stop.call_args.args[0][1], 'stop')

    def test_local_timeout_preserves_diagnostics_and_partial_output(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            partial = root / 'partial-native-output'
            partial.write_bytes(b'original partial bytes')
            process = MagicMock()
            process.pid = 123456
            process.returncode = -15
            process.__enter__.return_value = process
            process.communicate.side_effect = [subprocess.TimeoutExpired('source manager', 180), ('', 'interrupted native diagnostic')]
            with patch.object(smoke.subprocess, 'Popen', return_value=process) as launch, \
                    patch.object(smoke.os, 'killpg') as kill, \
                    patch.object(smoke, 'reconcile_source_invocation', return_value=False) as reconcile, \
                    self.assertRaisesRegex(RuntimeError, 'outcome is uncertain'):
                smoke.invoke_installed_source(self.config, root, ['plugins'])
            self.assertIs(launch.call_args.kwargs['start_new_session'], True)
            self.assertEqual(kill.call_args.args[0], process.pid)
            self.assertIs(reconcile.call_args.kwargs['uncertain'], True)
            evidence = json.loads(next((root / 'diagnostics').glob('*.json')).read_text())
            self.assertEqual(evidence['state'], 'container_outcome_uncertain')
            self.assertTrue(evidence['local_timeout'])
            self.assertFalse(evidence['container_cleanup_confirmed'])
            self.assertLess(next((root / 'diagnostics').glob('*.json')).stat().st_size, 4096)
            self.assertNotIn('interrupted native diagnostic', json.dumps(evidence))
            self.assertTrue(set(evidence) <= {'invocation', 'operation', 'state', 'installation',
                                             'owner_uid', 'image', 'container_names', 'cli_returncode',
                                             'local_timeout', 'container_cleanup_confirmed'})
            self.assertEqual(partial.read_bytes(), b'original partial bytes')

    def test_failed_source_smoke_never_removes_its_mount_root(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary) / 'preserved-source'
            root.mkdir()
            with patch.object(smoke, 'processor_identity', return_value=('original',)), \
                    patch.object(smoke.tempfile, 'mkdtemp', return_value=str(root)), \
                    patch.object(smoke, 'invoke_installed_source', side_effect=RuntimeError('uncertain invocation')), \
                    patch.object(smoke.shutil, 'rmtree') as remove, \
                    self.assertRaisesRegex(RuntimeError, 'uncertain invocation'):
                smoke.verify_installed_sources({**self.config, 'version': 'v1.2.3'})
            remove.assert_not_called()
            self.assertTrue(root.is_dir())

    def test_ci_uploads_only_invocation_fact_json_from_both_acceptance_paths(self):
        root = Path(__file__).resolve().parents[2]
        for name in ('.github/actions/container/action.yml', '.github/workflows/release-version.yml'):
            content = (root / name).read_text()
            marker = 'name: Retain bounded failed source invocation facts'
            self.assertEqual(content.count(marker), 1)
            selected = content.split(marker, 1)[1].split('if-no-files-found: ignore', 1)[0]
            self.assertIn('if: failure()', selected)
            self.assertIn('path: ${{ runner.temp }}/quazonai-source*/diagnostics/*.json', selected)
            self.assertNotIn('*.stdout', selected)
            self.assertNotIn('*.stderr', selected)


if __name__ == '__main__':
    unittest.main()
