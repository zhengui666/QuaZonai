"""Version selection and switching failures; real containers are covered by smoke.py."""
import json
import os
from pathlib import Path
import subprocess
import socket
import tempfile
import unittest
from unittest.mock import patch

import codex
import manage


class CodexTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        self.config = {'root': str(self.root), 'codex_runtime_image': 'quazonai-codex:test',
                       'codex_image': 'ghcr.io/zhengui666/quazonai-codex@sha256:' + 'a' * 64,
                       'docker_socket': '/var/run/docker.sock', 'bundle': str(manage.BUNDLE)}
        self.env = self.root / '.env'
        self.env.write_text('# keep this comment\nCODEX_VERSION=0.156.1\n')

    def test_env_is_data_and_only_accepts_an_exact_release(self):
        self.assertEqual(codex.read_env(self.env)[0], '0.156.1')
        for text in ('CODEX_VERSION=latest', 'CODEX_VERSION=^0.156.1', 'CODEX_VERSION=$(touch attack)',
                     'CODEX_VERSION=0.156.1\nCODEX_VERSION=0.157.0', 'DOCKER_HOST=remote',
                     'CODEX_VERSION=v0.157.0', 'CODEX_VERSION=01.2.3', 'CODEX_VERSION=1.2.3-01'):
            with self.subTest(text=text):
                self.env.write_text(text)
                with self.assertRaises(ValueError):
                    codex.read_env(self.env)
        self.assertFalse((self.root / 'attack').exists())

    def test_version_selection_is_a_registry_selector_not_an_npm_request(self):
        self.assertEqual(codex.requested_version('latest'), 'latest')
        self.assertEqual(codex.requested_version('0.156.1'), '0.156.1')
        for value in ('^0.157.0', 'file:/tmp/package', '1.x', 'v0.157.0'):
            with self.subTest(value=value), self.assertRaises(ValueError):
                codex.requested_version(value)
        self.assertNotIn('registry.npmjs.org', Path(codex.__file__).read_text())
        self.assertFalse(hasattr(codex, 'build'))

    def test_pull_freezes_the_actual_image_before_verifying_it(self):
        image = 'sha256:' + 'a' * 64
        for version, reference in (('latest', None), ('0.157.0', self.config['codex_image'])):
            calls = []
            def docker(config, *args, **kwargs):
                calls.append(args)
                if args[0] == 'pull':
                    return ''
                return json.dumps([{'Id': image, 'Os': 'linux', 'Architecture': 'amd64',
                                    'Config': {'Labels': {'org.opencontainers.image.version': '0.157.0'}}}])
            with self.subTest(version=version), patch.object(codex, 'docker', side_effect=docker), patch.object(
                codex, 'verify_candidate'
            ) as verify:
                self.assertEqual(codex.pull_candidate(self.config, version, reference=reference), ('0.157.0', image))
                self.assertEqual(calls[0], ('pull', reference or 'ghcr.io/zhengui666/quazonai-codex:latest'))
                verify.assert_called_once_with(self.config, '0.157.0', image)
                self.assertFalse(any('build' in call for call in calls))

    def test_reported_env_write_failure_restores_original_image_and_text(self):
        before = self.env.read_text()
        original_write = manage.atomic_text
        writes = 0
        def write(path, text):
            nonlocal writes
            writes += 1
            original_write(path, text)
            if writes == 1:
                raise OSError('reported after replace')
        with patch.object(codex, 'image_id', return_value='old-image'), patch.object(codex, 'docker') as docker, patch.object(
            manage, 'atomic_text', side_effect=write
        ), self.assertRaises(OSError):
            codex.switch(self.config, '0.157.0', 'candidate-image', before)
        self.assertEqual(self.env.read_text(), before)
        self.assertEqual([call.args[1:] for call in docker.call_args_list], [
            ('image', 'tag', 'candidate-image', self.config['codex_runtime_image']),
            ('image', 'tag', 'old-image', self.config['codex_runtime_image']),
        ])

    def test_sandbox_failure_does_not_activate_a_version_only_candidate(self):
        before = self.env.read_text()
        image = 'sha256:' + 'a' * 64
        def docker(config, *args, **kwargs):
            if args[0] == 'pull':
                return ''
            if args[:2] == ('image', 'inspect'):
                return json.dumps([{'Id': image, 'Os': 'linux', 'Architecture': 'amd64',
                                    'Config': {'Labels': {'org.opencontainers.image.version': '0.157.0'}}}])
            if args[-1] == '--version':
                return 'codex-cli 0.157.0'
            self.assertIn('sandbox', args)
            self.assertEqual(args[-1], '/usr/bin/true')
            self.assertIn('no-new-privileges:true', args)
            self.assertIn('--cap-drop', args)
            self.assertNotIn('--privileged', args)
            self.assertNotIn('--mount', args)
            self.assertNotIn('--volume', args)
            raise subprocess.CalledProcessError(1, ['docker', 'run'])
        with patch.object(manage, 'configuration', return_value=self.config), patch.object(
            codex, 'docker', side_effect=docker
        ), patch.object(codex, 'switch') as switch, self.assertRaisesRegex(ValueError, 'codex.apparmor'):
            codex.update(self.root, '0.157.0')
        switch.assert_not_called()
        self.assertEqual(self.env.read_text(), before)

    def test_failed_pull_has_no_build_or_switch_fallback(self):
        before = self.env.read_text()
        with patch.object(manage, 'configuration', return_value=self.config), patch.object(
            codex, 'docker', side_effect=subprocess.CalledProcessError(1, ['docker', 'pull'])
        ) as docker, patch.object(codex, 'switch') as switch, self.assertRaises(subprocess.CalledProcessError):
            codex.update(self.root, '0.157.0')
        switch.assert_not_called()
        docker.assert_called_once_with(self.config, 'pull', 'ghcr.io/zhengui666/quazonai-codex:0.157.0')
        self.assertEqual(self.env.read_text(), before)

    def test_active_runs_or_sessions_leave_shared_version_untouched(self):
        for active in ('run', 'session'):
            with self.subTest(active=active), patch.object(manage, 'configuration', return_value=self.config), patch.object(
                codex, 'pull_candidate', return_value=('0.157.0', 'candidate')
            ), patch.object(manage, 'require_idle', side_effect=ValueError('active run') if active == 'run' else None), patch.object(
                codex, 'require_stopped', side_effect=ValueError('active session') if active == 'session' else None
            ), patch.object(codex, 'switch') as switch, self.assertRaises(ValueError):
                codex.update(self.root, '0.157.0')
            switch.assert_not_called()
            self.assertEqual(codex.read_env(self.env)[0], '0.156.1')

    def test_never_started_orphan_is_removed_only_under_explicit_recovery(self):
        calls = []
        def docker(config, *args, **kwargs):
            calls.append(args)
            return 'container-id' if args[0] == 'container' and args[1] == 'ls' else 'created'
        with patch.object(codex, 'docker', side_effect=docker), self.assertRaises(ValueError):
            codex.require_stopped(self.config)
        self.assertFalse(any(args[:2] == ('container', 'rm') for args in calls))
        with manage.locked(self.root), patch.object(codex, 'docker', side_effect=docker):
            codex.require_stopped(self.config, recover_created=True)
        self.assertEqual(calls[-1], ('container', 'rm', 'container-id'))

    def test_success_updates_only_the_requested_version(self):
        with patch.object(codex, 'image_id', return_value='old'), patch.object(codex, 'docker'):
            codex.switch(self.config, '0.157.0', 'candidate', self.env.read_text())
        self.assertEqual(self.env.read_text(), '# keep this comment\nCODEX_VERSION=0.157.0\n')

    def test_legacy_installation_keeps_identity_and_native_home(self):
        endpoint = self.root / 'docker.sock'
        native = self.root / 'separate-home'
        native.mkdir()
        marker = native / 'history-preserved.txt'
        marker.write_text('existing native directory\n')
        legacy = {'root': str(self.root / 'installation'), 'project': 'quazonai-existing',
                  'uid': os.getuid(), 'gid': os.getgid(), 'codex_home': str(native), 'password': 'test-only'}
        with socket.socket(socket.AF_UNIX) as daemon:
            daemon.bind(str(endpoint))
            with patch.dict(os.environ, {'DOCKER_HOST': 'unix://' + str(endpoint)}):
                configured = manage.container_codex_configuration(legacy)
        self.assertEqual({key: configured[key] for key in legacy}, legacy)
        self.assertEqual(configured['codex_runtime_image'], 'quazonai-codex:quazonai-existing')
        self.assertEqual(configured['docker_socket'], str(endpoint))
        self.assertEqual(marker.read_text(), 'existing native directory\n')
        # The target bundle's public entry selects its new manager directly;
        # it never invokes the old installed updater/archive allowlist.
        with patch.object(manage.sys, 'argv', ['manage.py', 'apply-update', '--directory', legacy['root']]), patch.object(
            manage, 'apply_update'
        ) as upgrade, patch.object(manage, 'download_update') as old_download:
            manage.main()
        upgrade.assert_called_once_with(Path(legacy['root']))
        old_download.assert_not_called()


    def test_binary_version_and_platform_mismatches_are_not_switched(self):
        image = 'sha256:' + 'a' * 64
        for actual, platform in (('0.156.1', 'amd64'), ('0.157.0', 'arm64')):
            with self.subTest(actual=actual, platform=platform), patch.object(codex, 'docker', side_effect=[
                '', json.dumps([{'Id': image, 'Os': 'linux', 'Architecture': platform,
                                 'Config': {'Labels': {'org.opencontainers.image.version': actual}}}])
            ]), patch.object(codex, 'verify_candidate') as verify, self.assertRaises(ValueError):
                codex.pull_candidate(self.config, '0.157.0')
            verify.assert_not_called()
        with patch.object(codex, 'docker', return_value='codex-cli 0.156.1'), self.assertRaisesRegex(ValueError, 'does not match'):
            codex.verify_candidate(self.config, '0.157.0', image)

    def test_default_install_uses_manifest_without_an_env_or_build_file(self):
        self.env.unlink()
        self.config['codex_version'] = '0.157.0'
        with patch.object(codex, 'image_id', return_value=None), patch.object(
            codex, 'require_stopped'
        ), patch.object(codex, 'pull_candidate', return_value=('0.157.0', 'candidate')) as pull, patch.object(
            codex, 'switch'
        ) as switch:
            codex.prepare(self.config, self.root)
        pull.assert_called_once_with(self.config, '0.157.0', reference=self.config['codex_image'])
        switch.assert_called_once_with(self.config, '0.157.0', 'candidate', None)

    def test_application_upgrade_preserves_independent_codex_selection(self):
        self.config['codex_version'] = '0.157.0'
        with patch.object(codex, 'image_id', return_value='existing'), patch.object(
            codex, 'docker', return_value='0.156.1'
        ), patch.object(codex, 'pull_candidate') as pull, patch.object(codex, 'switch') as switch:
            codex.prepare(self.config, self.root)
        pull.assert_not_called()
        switch.assert_not_called()
        self.assertEqual(codex.read_env(self.env)[0], '0.156.1')

    def test_pending_update_refuses_independent_switch_before_download(self):
        (self.root / 'pending.json').write_text('{}')
        with patch.object(manage, 'configuration', return_value=self.config), patch.object(
            codex, 'pull_candidate'
        ) as pull, self.assertRaisesRegex(ValueError, 'pending'):
            codex.update(self.root, 'latest')
        pull.assert_not_called()


if __name__ == '__main__':
    unittest.main()
