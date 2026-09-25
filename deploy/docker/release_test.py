"""Focused release/upgrade regressions; service acceptance lives in smoke.py."""
import argparse
import ast
import contextlib
import io
import json
import os
from pathlib import Path
import socket
import stat
import subprocess
import tarfile
import tempfile
import unittest
from unittest.mock import patch

import manage
import release


def metadata(tag="v1.2.3"):
    return {"schema_version": 1, "version": tag, "revision": "a" * 40,
            "image": "sha256:" + "b" * 64, "database_image": manage.DATABASE_IMAGE}


class ManifestTests(unittest.TestCase):
    def test_versions(self):
        for value in ("v0.0.0", "v2.3.4", "v2.3.4-rc.1", "v2.3.4-01a"):
            self.assertEqual(manage.version(value), value)
        for value in ("latest", "2.3.4", "v01.2.3", "v1.2.3-01", "v1.2.3+build", "v1.2.3-", "v1.2.3/x"):
            with self.subTest(value=value), self.assertRaises(ValueError):
                manage.version(value)

    def test_digest_and_database_binding(self):
        self.assertEqual(manage.validate_manifest(metadata()), metadata())
        for changes in ({"schema_version": 2}, {"image": "ghcr.io/zhengui666/quazonai:latest"},
                        {"revision": "main"}, {"database_image": "postgres:18"}):
            with self.subTest(changes=changes), self.assertRaises(ValueError):
                manage.validate_manifest({**metadata(), **changes})

    def test_real_bundle_roundtrip(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            release.bundle("v1.2.3", "a" * 40, "sha256:" + "b" * 64, root / "assets")
            (root / "unpacked").mkdir()
            manage.unpack((root / "assets/quazonai-deploy.tar.gz").read_bytes(), root / "unpacked")
            self.assertEqual({x.name for x in (root / "unpacked").iterdir()}, manage.BUNDLE_FILES)
            self.assertEqual(manage.manifest(root / "unpacked"), metadata())

    def test_scripts_parse(self):
        for path in manage.BUNDLE.glob("*.py"):
            ast.parse(path.read_text(), filename=str(path))

    def test_existing_partial_state_is_not_reinitialized(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            state = root / "data/state"
            state.mkdir(parents=True)
            (state / "master.key").write_bytes(b"local-test-key")
            with patch.object(manage, "compose") as command, self.assertRaises(ValueError):
                manage.initialize_state({"root": str(root)})
            command.assert_not_called()
            self.assertEqual((state / "master.key").read_bytes(), b"local-test-key")

    def test_parallel_deployment_uses_one_lock(self):
        with tempfile.TemporaryDirectory() as temporary, manage.locked(Path(temporary)):
            with self.assertRaises(BlockingIOError), manage.locked(Path(temporary)):
                self.fail("Second installation lock was granted")

    def test_first_download_failure_keeps_installation_identity(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source = root / "bundle"
            source.mkdir()
            (source / "release.json").write_text(json.dumps(metadata()))
            args = argparse.Namespace(port=18081, database_port=55432, codex_home=str(root / "native"))
            with patch.object(manage, "BUNDLE", source), patch.object(manage, "preflight"), patch.object(manage, "validate_ports"), patch.object(manage, 'require_new_docker_project'), patch.object(
                manage, "prepare", side_effect=ValueError("test download failure")
            ):
                with self.assertRaises(ValueError):
                    manage.deploy(root / "installation", args)
                before = manage.configuration(root / "installation")
                with self.assertRaises(ValueError):
                    manage.deploy(root / "installation", args)
                self.assertEqual(manage.configuration(root / "installation"), before)


class InstallationInputTests(unittest.TestCase):
    def test_occupied_ports_do_not_persist_an_installation(self):
        with tempfile.TemporaryDirectory() as temporary, socket.socket() as busy, socket.socket() as free:
            root = Path(temporary)
            busy.bind(("127.0.0.1", 0))
            busy.listen()
            free.bind(("127.0.0.1", 0))
            occupied, available = busy.getsockname()[1], free.getsockname()[1]
            free.close()
            for index, pair in enumerate(((occupied, available), (available, occupied))):
                installation = root / str(index)
                args = argparse.Namespace(port=pair[0], database_port=pair[1], codex_home=str(root / "native"))
                with self.subTest(pair=pair), patch.object(manage, "manifest", return_value=metadata()), patch.object(
                    manage, "preflight"
                ), patch.object(manage, "prepare") as prepare, self.assertRaisesRegex(ValueError, "port.*unavailable"):
                    manage.deploy(installation, args)
                prepare.assert_not_called()
                self.assertFalse((installation / "installation.json").exists())
                self.assertFalse((installation / "pending.json").exists())

    def test_port_ranges_and_distinctness(self):
        for pair in ((1023, 55432), (8081, 65536), (8081, 8081), (True, 55432)):
            with self.subTest(pair=pair), self.assertRaises(ValueError):
                manage.validate_ports(*pair)
        manage.validate_ports(1024, 65535)

    def test_systemd_paths_are_literal_but_exec_is_quoted(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary) / 'installation with spaces %n $HOME'
            root.mkdir()
            config = {**metadata(), "root": str(root), "project": "release-unit-test", "password": os.urandom(16).hex(),
                      "database_port": 55432, "port": 8081, "home": str(root), "codex_home": str(root / "native"),
                      "path": "/usr/bin", "unit_directory": str(root / "units")}
            with patch.object(manage, "run"), patch.object(manage, "verify_worker"):
                manage.configure_worker(config)
            content = (root / "units/release-unit-test.service").read_text()
            self.assertIn(f"WorkingDirectory={str(root).replace('%', '%%')}/data\n", content)
            self.assertIn(f"EnvironmentFile={str(root).replace('%', '%%')}/worker.env\n", content)
            self.assertIn('ExecStart=:"', content)
            self.assertIn('PUBLIC_URL="http://localhost:8081"\n', (root / "worker.env").read_text())
            for path in (Path("relative"), Path("/bad\npath")):
                with self.assertRaises(ValueError):
                    manage.unit_path(path)


class DeploymentBoundaryTests(unittest.TestCase):
    def test_semver_precedence_handles_numeric_and_prerelease_components(self):
        ordered = ['v1.0.0-alpha', 'v1.0.0-alpha.1', 'v1.0.0-alpha.beta', 'v1.0.0-beta',
                   'v1.0.0-beta.2', 'v1.0.0-beta.11', 'v1.0.0-rc.1', 'v1.0.0',
                   'v1.9.0', 'v1.10.0', 'v2.0.0-rc.1', 'v2.0.0']
        self.assertEqual(sorted(reversed(ordered), key=manage.version_precedence), ordered)

    def test_remapped_daemons_are_rejected(self):
        manage.validate_docker_mapping(['name=seccomp,profile=builtin', 'name=apparmor', 'name=cgroupns'])
        manage.validate_docker_mapping([])
        for options in (['name=rootless'], ['name=userns'], ['rootless'], None, [1]):
            with self.subTest(options=options), self.assertRaises(ValueError):
                manage.validate_docker_mapping(options)

    def test_orphaned_project_resources_preserve_the_missing_identity(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for resource in ('container', 'volume', 'network'):
                installation = root / resource
                args = argparse.Namespace(port=18081, database_port=55432, codex_home=str(root / 'native'))
                def listing(command, **kwargs):
                    self.assertEqual(command[0], 'docker')
                    self.assertEqual(command[2], 'ls')
                    self.assertIn('--filter', command)
                    self.assertTrue(command[command.index('--filter') + 1].startswith('label=com.docker.compose.project=quazonai-'))
                    return 'preserved-resource' if command[1] == resource else ''
                with self.subTest(resource=resource), patch.object(manage, 'manifest', return_value=metadata()), patch.object(
                    manage, 'preflight'
                ), patch.object(manage, 'validate_ports'), patch.object(manage, 'run', side_effect=listing), patch.object(manage, 'prepare') as prepare:
                    with self.assertRaisesRegex(ValueError, 'Existing Docker resources'):
                        manage.deploy(installation, args)
                    prepare.assert_not_called()
                self.assertFalse((installation / 'installation.json').exists())
                self.assertFalse((installation / 'pending.json').exists())
                self.assertFalse((root / 'native').exists())


class RestartAndLayoutTests(unittest.TestCase):
    def test_codex_home_overlap_is_rejected_before_identity_or_state_creation(self):
        with tempfile.TemporaryDirectory() as temporary:
            parent = Path(temporary)
            root = parent / 'installation'
            alias = parent / 'native-alias'
            alias.symlink_to(root / 'data/state/native')
            for native in (root, root / 'data/state', root / 'data/state/native', root / 'backups/native', parent, alias):
                args = argparse.Namespace(port=18081, database_port=55432, codex_home=str(native))
                with self.subTest(native=native), patch.object(manage, 'manifest', return_value=metadata()), patch.object(
                    manage, 'preflight'
                ), patch.object(manage, 'validate_ports'), patch.object(manage, 'require_new_docker_project') as project:
                    with self.assertRaisesRegex(ValueError, 'CODEX_HOME must be separate'):
                        manage.deploy(root, args)
                    project.assert_not_called()
                self.assertFalse((root / 'installation.json').exists())
                self.assertFalse((root / 'pending.json').exists())
                self.assertFalse((root / 'data').exists())
                self.assertFalse((root / 'backups').exists())

    def test_compose_disables_restarts_for_initial_or_pending_candidates(self):
        with tempfile.TemporaryDirectory() as temporary, patch.object(manage, 'run', return_value='') as command:
            root = Path(temporary)
            config = {**metadata(), 'root': str(root), 'project': 'restart-test', 'password': os.urandom(16).hex(),
                      'port': 18081, 'database_port': 55432, 'uid': os.getuid(), 'gid': os.getgid(),
                      'codex_home': str(root.parent / 'native'), 'bundle': str(root / 'bundle')}
            manage.compose(config, 'config', '--quiet')
            self.assertEqual(command.call_args.kwargs['env']['APP_RESTART_POLICY'], 'no')
            (root / 'current').symlink_to(root / 'releases/v1.2.3')
            manage.compose(config, 'config', '--quiet')
            self.assertEqual(command.call_args.kwargs['env']['APP_RESTART_POLICY'], 'unless-stopped')
            manage.save(root / 'pending.json', {'operation': 'update'})
            manage.compose(config, 'config', '--quiet')
            self.assertEqual(command.call_args.kwargs['env']['APP_RESTART_POLICY'], 'no')

    def test_restart_policy_changes_only_the_selected_app_container(self):
        with patch.object(manage, 'compose', return_value='app-container-id') as compose, patch.object(manage, 'run') as command:
            manage.configure_app_restarts({}, False)
            command.assert_called_with(['docker', 'update', '--restart=no', 'app-container-id'], capture=True)
            manage.configure_app_restarts({}, True)
            command.assert_called_with(['docker', 'update', '--restart=unless-stopped', 'app-container-id'], capture=True)
            compose.assert_called_with({}, 'ps', '--all', '--quiet', 'app', capture=True)
            compose.return_value = ''
            with self.assertRaisesRegex(ValueError, 'exactly one'):
                manage.configure_app_restarts({}, True)


class NativeDeploymentTests(unittest.TestCase):
    def test_environment_file_path_escapes_globs_only(self):
        path = Path(r'/tmp/部署 [one]*?\part %n $HOME/worker.env')
        self.assertEqual(manage.unit_path(path), str(path).replace('%', '%%'))
        self.assertEqual(
            manage.unit_path(path, environment_file=True),
            r'/tmp/部署 \[one\]\*\?\\part %%n $HOME/worker.env',
        )

    def test_invalid_paths_fail_before_installation_identity_is_saved(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for name in ('bad\npath', 'bad\tpath', 'bad:path', 'bad"path', 'bad\\path'):
                installation = root / name
                args = argparse.Namespace(port=18081, database_port=55432, codex_home=str(root / 'native'))
                with self.subTest(name=name), patch.object(manage, 'manifest', return_value=metadata()), patch.object(
                    manage, 'preflight'
                ), patch.object(manage, 'validate_ports'), patch.object(manage, 'prepare') as prepare:
                    with self.assertRaises(ValueError):
                        manage.deploy(installation, args)
                    prepare.assert_not_called()
                self.assertFalse((installation / 'installation.json').exists())
                self.assertFalse((installation / 'pending.json').exists())
                self.assertFalse((root / 'native').exists())

    def test_host_package_checks_only_the_worker_not_a_native_codex(self):
        with tempfile.TemporaryDirectory() as temporary, patch.object(manage, 'run') as command:
            binaries = Path(temporary)
            manage.verify_native_binaries(binaries)
            self.assertEqual([call.args[0] for call in command.call_args_list], [
                [str(binaries / 'server'), '--version'],
            ])

    def test_native_unit_validation_uses_exact_future_unit_without_installing(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            config = {**metadata(), 'root': str(root / 'install [one]'), 'project': 'unit-validation-test'}
            observed = []
            def inspect(args):
                self.assertEqual(args[:3], ['systemd-analyze', '--user', 'verify'])
                path = Path(args[3])
                self.assertEqual(path.name, manage.unit(config))
                self.assertEqual(path.read_text(), manage.worker_unit_text(config))
                observed.append(path)
                raise subprocess.CalledProcessError(1, args)
            with patch.object(manage, 'run', side_effect=inspect), self.assertRaises(subprocess.CalledProcessError):
                manage.verify_worker_unit(config)
            self.assertEqual(len(observed), 1)
            self.assertFalse(observed[0].exists())
            self.assertEqual(list(root.iterdir()), [])


class DurabilityTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        self.config = {**metadata(), 'root': str(self.root), 'project': 'durability-test',
                       'password': os.urandom(16).hex(), 'database_port': 55432, 'port': 18081,
                       'home': str(self.root), 'codex_home': str(self.root.parent / 'native'),
                       'path': '/usr/bin', 'unit_directory': str(self.root / 'units/user')}

    def test_atomic_text_syncs_file_before_replace_and_directory_after(self):
        target = self.root / 'worker.env'
        target.write_text('previous complete value')
        synced = []
        fsync, replace = os.fsync, os.replace
        def synchronize(descriptor):
            synced.append('directory' if stat.S_ISDIR(os.fstat(descriptor).st_mode) else 'file')
            fsync(descriptor)
        def publish(source, destination):
            self.assertEqual(target.read_text(), 'previous complete value')
            self.assertEqual(synced, ['file'])
            synced.append('replace')
            replace(source, destination)
        with patch.object(manage.os, 'fsync', side_effect=synchronize), patch.object(manage.os, 'replace', side_effect=publish):
            manage.atomic_text(target, 'new complete value\n')
        self.assertEqual(synced, ['file', 'replace', 'directory'])
        self.assertEqual(target.read_text(), 'new complete value\n')
        self.assertEqual(stat.S_IMODE(target.stat().st_mode), 0o600)
        self.assertEqual(list(self.root.iterdir()), [target])

    def test_file_sync_failure_preserves_old_atomic_file(self):
        target = self.root / 'worker.env'
        target.write_text('old complete value')
        with patch.object(manage.os, 'fsync', side_effect=OSError('file sync failed')), self.assertRaises(OSError):
            manage.atomic_text(target, 'replacement must not be published')
        self.assertEqual(target.read_text(), 'old complete value')
        self.assertEqual(list(self.root.iterdir()), [target])

    def test_worker_files_are_complete_and_synced_before_manager_reload(self):
        published = []
        write = manage.atomic_text
        def durable(path, text):
            write(path, text)
            published.append(path)
        def manager(args):
            self.assertEqual(args, ['systemctl', '--user', 'daemon-reload'])
            self.assertEqual(published, [self.root / 'worker.env', Path(self.config['unit_directory']) / manage.unit(self.config)])
            self.assertEqual(published[1].read_text(), manage.worker_unit_text(self.config))
            self.assertTrue((self.root / 'worker.env').read_text().endswith('\n'))
        with patch.object(manage, 'atomic_text', side_effect=durable), patch.object(manage, 'run', side_effect=manager) as command:
            manage.configure_worker(self.config)
        self.assertEqual(command.call_count, 1)

    def test_unit_sync_failure_never_truncates_or_reloads_previous_unit(self):
        units = Path(self.config['unit_directory'])
        units.mkdir(parents=True)
        target = units / manage.unit(self.config)
        target.write_text('previous complete unit')
        fsync = os.fsync
        def fail_unit(descriptor):
            path = Path(os.readlink(f'/proc/self/fd/{descriptor}'))
            if stat.S_ISREG(os.fstat(descriptor).st_mode) and path.parent == units:
                raise OSError('unit sync failed')
            fsync(descriptor)
        with patch.object(manage.os, 'fsync', side_effect=fail_unit), patch.object(manage, 'run') as command, self.assertRaises(OSError):
            manage.configure_worker(self.config)
        self.assertEqual(target.read_text(), 'previous complete unit')
        self.assertEqual(list(units.iterdir()), [target])
        command.assert_not_called()

    def test_backup_syncs_all_artifacts_and_directory_before_returning(self):
        state = self.root / 'data/state'
        state.mkdir(parents=True)
        (state / 'master.key').write_bytes(os.urandom(32))
        (state / 'session-key.ref').write_text('test-only reference')
        synced = []
        fsync = os.fsync
        def synchronize(descriptor):
            synced.append(Path(os.readlink(f'/proc/self/fd/{descriptor}')))
            fsync(descriptor)
        def dump(config, *args, output=None, **kwargs):
            self.assertIn('pg_dump', args)
            # Filesystem ordering only; real pg_dump/pg_restore runs in Container CI.
            output.write(b'test-only dump bytes')
        with patch.object(manage, 'compose', side_effect=dump), patch.object(manage.os, 'fsync', side_effect=synchronize):
            recovery = manage.backup(self.config)
        for name in ('database.dump', 'master.key', 'data.tar.gz', 'installation.json'):
            self.assertIn(recovery / name, synced)
            self.assertLess(synced.index(recovery / name), len(synced) - 1)
        self.assertEqual(synced[-2:], [recovery, recovery.parent])
        self.assertIn(self.root, synced)
        with tarfile.open(recovery / 'data.tar.gz') as archive:
            self.assertNotIn('data/state/master.key', archive.getnames())
            self.assertIn('data/state/session-key.ref', archive.getnames())

    def test_backup_artifact_sync_failure_does_not_return_a_recovery_point(self):
        state = self.root / 'data/state'
        state.mkdir(parents=True)
        original = os.urandom(32)
        (state / 'master.key').write_bytes(original)
        fsync = os.fsync
        def fail_key(descriptor):
            path = Path(os.readlink(f'/proc/self/fd/{descriptor}'))
            if path.name == 'master.key' and path.parent.parent.name == 'backups':
                raise OSError('recovery file sync failed')
            fsync(descriptor)
        def dump(config, *args, output=None, **kwargs):
            output.write(b'test-only dump bytes')
        with patch.object(manage, 'compose', side_effect=dump), patch.object(manage.os, 'fsync', side_effect=fail_key):
            with self.assertRaisesRegex(OSError, 'recovery file sync failed'):
                manage.backup(self.config)
        self.assertEqual((state / 'master.key').read_bytes(), original)
        self.assertFalse((self.root / 'pending.json').exists())

    def test_legacy_install_guard_rejects_containers_or_active_worker(self):
        active = {'state': {'Status': 'running', 'Running': True}, 'restart': 'no'}
        with patch.object(manage, 'compose', return_value='existing-container'), patch.object(
            manage, 'run', return_value=json.dumps(active)
        ) as command:
            with self.assertRaises(ValueError):
                manage.require_install_stopped(self.config)
            self.assertEqual(command.call_count, 1)
        created = {'state': {'Status': 'created', 'Running': False, 'Restarting': False}, 'restart': 'no'}
        with patch.object(manage, 'compose', return_value='created-container'), patch.object(
            manage, 'run', side_effect=[json.dumps(created), 'LoadState=not-found']
        ):
            manage.require_install_stopped(self.config)
        observations = ['LoadState=loaded\nActiveState=active\nUnitFileState=disabled',
                        'LoadState=loaded\nActiveState=inactive\nUnitFileState=enabled', '']
        with patch.object(manage, 'compose', return_value=''):
            for value in observations:
                with self.subTest(value=value), patch.object(manage, 'run', return_value=value), self.assertRaises(ValueError):
                    manage.require_install_stopped(self.config)
            for value in ('LoadState=not-found', 'LoadState=loaded\nActiveState=inactive\nUnitFileState=disabled'):
                with patch.object(manage, 'run', return_value=value):
                    manage.require_install_stopped(self.config)


class GitSelectionTests(unittest.TestCase):
    def test_lightweight_annotated_and_both_push_orders(self):
        previous = Path.cwd()
        with tempfile.TemporaryDirectory() as temporary:
            os.chdir(temporary)
            try:
                def git(*args):
                    return manage.run(["git", *args], capture=True)
                git("init", "-b", "main")
                git("config", "user.email", "release-test@example.invalid")
                git("config", "user.name", "Release test")
                Path("deploy/docker").mkdir(parents=True)
                Path("deploy/docker/release.py").write_text("# release-capable test commit\n")
                git("add", ".")
                git("commit", "-m", "first")
                first = git("rev-parse", "HEAD")
                git("tag", "v1.0.0")
                git("update-ref", "refs/remotes/origin/main", first)
                git("switch", "-c", "feature")
                Path("change").write_text("second")
                git("add", ".")
                git("commit", "-m", "second")
                second = git("rev-parse", "HEAD")
                git("tag", "-a", "v1.0.1", "-m", "annotated")
                with patch.object(release, "releases", return_value={}), patch.object(release, "ci_ready", return_value=True):
                    self.assertEqual(release.select(None), [{"version": "v1.0.0", "revision": first}])
                    git("switch", "main")
                    git("merge", "--ff-only", "feature")
                    git("update-ref", "refs/remotes/origin/main", second)
                    self.assertEqual(release.select(None), [{"version": "v1.0.0", "revision": first},
                                                           {"version": "v1.0.1", "revision": second}])
                    self.assertEqual(release.tag_revision("v1.0.1"), second)
                existing = {"v1.0.0": {"tag_name": "v1.0.0", "target_commitish": first, "draft": False,
                                        "assets": [{"name": x} for x in release.ASSETS]}}
                with patch.object(release, "releases", return_value=existing), patch.object(release, "ci_ready", return_value=True):
                    self.assertEqual(release.select(None), [{"version": "v1.0.1", "revision": second}])
            finally:
                os.chdir(previous)

    def test_old_success_cannot_hide_latest_failed_ci(self):
        runs = [{"id": index, "path": path, "head_sha": "a" * 40, "status": "completed", "conclusion": "success"}
                for index, path in enumerate(sorted(release.CI_PATHS), 1)]
        with patch.object(release, "api", return_value=[{"workflow_runs": runs}]):
            self.assertTrue(release.ci_ready("a" * 40))
            self.assertFalse(release.ci_ready("b" * 40))
        newer = {**runs[0], "id": 99, "conclusion": "failure"}
        with patch.object(release, "api", return_value=[{"workflow_runs": [*runs, newer]}]):
            self.assertFalse(release.ci_ready("a" * 40))

    def test_published_version_cannot_move(self):
        existing = {"tag_name": "v1.0.0", "target_commitish": "a" * 40, "draft": False,
                    "assets": [{"name": x} for x in release.ASSETS]}
        self.assertTrue(release.completed_release(existing, "a" * 40))
        with self.assertRaises(ValueError):
            release.completed_release(existing, "b" * 40)
        with self.assertRaises(ValueError):
            release.completed_release({**existing, "assets": []}, "a" * 40)


class UpdateTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        self.old = {**metadata("v1.0.0"), "root": str(self.root), "uid": os.getuid(),
                    "project": "release-unit-test", "port": 18081, "bundle": str(self.root / "old")}
        self.new = {**self.old, "version": "v1.0.1", "bundle": str(self.root / "new")}
        manage.save(self.root / "installation.json", self.old)
        self.backup = self.root / "backup"
        self.backup.mkdir()
        self.events = []
        self.fail_migration = False
        self.stack = contextlib.ExitStack()
        self.addCleanup(self.stack.close)
        self.stack.enter_context(patch.object(manage, "preflight"))
        self.stack.enter_context(patch.object(manage, "prepare", return_value=self.new))
        self.stack.enter_context(patch.object(manage, "manifest", return_value=metadata("v1.0.1")))
        self.stack.enter_context(patch.object(manage, "verify_console"))
        self.stack.enter_context(patch.object(manage, "verify_worker"))
        self.stack.enter_context(patch.object(manage, "configure_worker", side_effect=lambda c: self.events.append("worker-configure")))
        self.stack.enter_context(patch.object(manage, 'configure_app_restarts', side_effect=lambda c, enabled: self.events.append(('app-restarts', c['version'], enabled))))
        self.stack.enter_context(patch.object(manage, "run", side_effect=lambda *a, **k: self.events.append(tuple(a[0]))))
        self.stack.enter_context(patch.object(manage.subprocess, "run"))
        self.stack.enter_context(patch.object(manage, "compose", side_effect=self.compose))
        self.idle = self.stack.enter_context(patch.object(manage, "require_idle"))
        self.backup_call = self.stack.enter_context(patch.object(manage, "backup", return_value=self.backup))

    def compose(self, config, *args, **kwargs):
        self.events.append((config["version"], *args))
        if "migrate" in args and self.fail_migration:
            raise subprocess.CalledProcessError(1, ["test-migrate"])
        return ""

    def test_active_runs_prevent_stopping_any_service(self):
        self.idle.side_effect = ValueError("active run")
        with self.assertRaises(ValueError):
            manage.apply_update(self.root)
        self.assertEqual(self.events, [])
        self.backup_call.assert_not_called()

    def test_failed_backup_restores_original_processes_without_migrating(self):
        self.backup_call.side_effect = OSError("test backup failure")
        with self.assertRaises(OSError):
            manage.apply_update(self.root)
        self.assertTrue(any("up" in event for event in self.events))
        self.assertFalse(any("migrate" in event for event in self.events))
        self.assertFalse((self.root / "pending.json").exists())
        self.assertIn(('systemctl', '--user', 'disable', '--now', manage.unit(self.old)), self.events)
        self.assertIn(('systemctl', '--user', 'enable', '--now', manage.unit(self.old)), self.events)

    def test_worker_shutdown_failure_restores_old_services_before_migration(self):
        def command(args, **kwargs):
            self.events.append(tuple(args))
            if args[2:4] == ['disable', '--now']:
                raise subprocess.CalledProcessError(1, args)
        with patch.object(manage, 'run', side_effect=command), self.assertRaises(subprocess.CalledProcessError):
            manage.apply_update(self.root)
        self.assertIn((self.old['version'], 'up', '-d', '--no-recreate', '--wait', '--wait-timeout', '120', 'app'), self.events)
        self.assertIn(('systemctl', '--user', 'enable', '--now', manage.unit(self.old)), self.events)
        self.assertIn(('app-restarts', self.old['version'], True), self.events)
        self.backup_call.assert_not_called()
        self.assertFalse(any('migrate' in event for event in self.events))
        self.assertFalse((self.root / 'pending.json').exists())
        self.assertEqual(manage.configuration(self.root), self.old)

    def test_app_shutdown_failure_also_restores_both_old_services(self):
        def operation(config, *args, **kwargs):
            if args == ('stop', 'app'):
                raise subprocess.CalledProcessError(1, ['docker', 'compose', 'stop', 'app'])
            return self.compose(config, *args, **kwargs)
        with patch.object(manage, 'compose', side_effect=operation), self.assertRaises(subprocess.CalledProcessError):
            manage.apply_update(self.root)
        self.assertIn(('systemctl', '--user', 'enable', '--now', manage.unit(self.old)), self.events)
        self.assertIn(('app-restarts', self.old['version'], True), self.events)
        self.assertFalse(any('migrate' in event for event in self.events))
        self.backup_call.assert_not_called()

    def test_pending_shutdown_failure_never_restarts_the_old_release(self):
        manage.save(self.root / 'pending.json', {'operation': 'update', 'previous': self.old,
                                               'target': self.new, 'backup': str(self.backup)})
        with patch.object(manage, 'run', side_effect=subprocess.CalledProcessError(1, ['systemctl', 'disable'])), self.assertRaises(subprocess.CalledProcessError):
            manage.apply_update(self.root)
        self.assertFalse(any('up' in event or 'worker-configure' == event for event in self.events))
        self.assertNotIn(('app-restarts', self.old['version'], True), self.events)
        self.assertTrue((self.root / 'pending.json').exists())
        self.backup_call.assert_not_called()

    def test_broken_success_output_does_not_stop_an_activated_release(self):
        with patch.object(manage, 'print', side_effect=BrokenPipeError(), create=True), patch.object(manage.sys, 'stdout', io.StringIO()):
            try:
                manage.apply_update(self.root)
            finally:
                manage.sys.stdout.close()
        self.assertEqual(manage.configuration(self.root), self.new)
        self.assertFalse((self.root / 'pending.json').exists())
        self.assertEqual(self.events[-1], ('app-restarts', self.new['version'], True))
        self.assertIn(('systemctl', '--user', 'enable', manage.unit(self.new)), self.events)
        manage.subprocess.run.assert_not_called()

    def test_downgrade_targets_leave_the_active_installation_untouched(self):
        for target in ('v0.9.9', 'v1.0.0-rc.9', 'v1.0.0-alpha'):
            with self.subTest(target=target), patch.object(manage, 'manifest', return_value=metadata(target)), patch.object(manage, 'prepare') as prepare:
                with self.assertRaisesRegex(ValueError, 'cannot downgrade'):
                    manage.apply_update(self.root)
                prepare.assert_not_called()
            with patch.object(manage.urllib.request, 'urlopen') as download:
                with self.assertRaisesRegex(ValueError, 'cannot downgrade'):
                    manage.download_update(self.root, target)
                download.assert_not_called()
            self.assertEqual(self.events, [])
            self.assertEqual(manage.configuration(self.root), self.old)
            self.assertFalse((self.root / 'pending.json').exists())
        self.backup_call.assert_not_called()

    def test_prepare_failure_keeps_existing_services_and_backup_untouched(self):
        with patch.object(manage, 'prepare', side_effect=ValueError('invalid future user unit')):
            with self.assertRaises(ValueError):
                manage.apply_update(self.root)
        self.assertEqual(self.events, [])
        self.backup_call.assert_not_called()
        self.assertEqual(manage.configuration(self.root), self.old)

    def test_migration_failure_stays_stopped_and_retry_keeps_original_backup(self):
        self.fail_migration = True
        with self.assertRaises(subprocess.CalledProcessError):
            manage.apply_update(self.root)
        self.assertNotIn("worker-configure", self.events)
        self.assertFalse(any("up" in event for event in self.events))
        self.assertEqual(manage.configuration(self.root), self.old)
        self.assertTrue((self.root / "pending.json").exists())
        disabled = ('systemctl', '--user', 'disable', '--now', manage.unit(self.old))
        self.assertLess(self.events.index(disabled), next(i for i, event in enumerate(self.events) if 'migrate' in event))
        self.assertFalse(any('enable' in event for event in self.events))
        self.fail_migration = False
        manage.apply_update(self.root)
        self.assertEqual(manage.configuration(self.root), self.new)
        self.assertFalse((self.root / "pending.json").exists())
        self.assertEqual(self.backup_call.call_count, 1)
        self.assertIn("worker-configure", self.events)
        self.assertGreater(self.events.index(('systemctl', '--user', 'enable', manage.unit(self.new))), self.events.index('worker-configure'))

    def test_activation_enables_worker_before_app_boot_recovery(self):
        manage.apply_update(self.root)
        worker = ('systemctl', '--user', 'enable', manage.unit(self.new))
        app = ('app-restarts', self.new['version'], True)
        self.assertLess(self.events.index(worker), self.events.index(app))
        self.assertFalse((self.root / 'pending.json').exists())

    def test_activation_enable_failure_keeps_app_non_restarting(self):
        def command(args, **kwargs):
            self.events.append(tuple(args))
            if args[2] == 'enable' and '--now' not in args:
                raise subprocess.CalledProcessError(1, args)
        with patch.object(manage, 'run', side_effect=command), self.assertRaises(subprocess.CalledProcessError):
            manage.apply_update(self.root)
        self.assertNotIn(('app-restarts', self.new['version'], True), self.events)
        pending = json.loads((self.root / 'pending.json').read_text())
        self.assertEqual(pending['phase'], 'starting')
        self.assertEqual(pending['previous'], self.old)
        started = self.events.index(('systemctl', '--user', 'start', manage.unit(self.new)))
        self.assertFalse(any('stop' in event or 'disable' in event for event in self.events[started:]))

    def test_shutdown_intent_survives_abrupt_exit_and_retry(self):
        def crash(config, enabled):
            self.assertFalse(enabled)
            intent = json.loads((self.root / 'pending.json').read_text())
            self.assertEqual(intent['phase'], 'preparing')
            self.assertIsNone(intent['backup'])
            self.assertEqual(intent['previous'], self.old)
            self.assertEqual(intent['target'], self.new)
            raise KeyboardInterrupt('deployer stopped without exception cleanup')
        with patch.object(manage, 'configure_app_restarts', side_effect=crash), self.assertRaises(KeyboardInterrupt):
            manage.apply_update(self.root)
        self.assertEqual(self.events, [])
        self.assertEqual(manage.configuration(self.root), self.old)
        self.backup_call.assert_not_called()
        manage.apply_update(self.root)
        self.assertEqual(manage.configuration(self.root), self.new)
        self.assertEqual(self.backup_call.call_count, 1)
        self.assertFalse((self.root / 'pending.json').exists())

    def test_shutdown_intent_write_failure_does_not_change_services(self):
        with patch.object(manage, 'save', side_effect=OSError('intent write failed')), self.assertRaises(OSError):
            manage.apply_update(self.root)
        self.assertEqual(self.events, [])
        self.backup_call.assert_not_called()
        self.assertEqual(manage.configuration(self.root), self.old)
        self.assertFalse((self.root / 'pending.json').exists())

    def test_failed_preparing_restore_retains_recovery_marker(self):
        self.backup_call.side_effect = OSError('backup failed')
        def command(args, **kwargs):
            self.events.append(tuple(args))
            if args[2:4] == ['enable', '--now']:
                raise subprocess.CalledProcessError(1, args)
        with patch.object(manage, 'run', side_effect=command), self.assertRaises(subprocess.CalledProcessError):
            manage.apply_update(self.root)
        self.assertEqual(json.loads((self.root / 'pending.json').read_text())['phase'], 'preparing')
        self.assertNotIn(('app-restarts', self.old['version'], True), self.events)
        self.assertFalse(any('migrate' in event for event in self.events))
        self.backup_call.side_effect = None
        manage.apply_update(self.root)
        self.assertEqual(manage.configuration(self.root), self.new)
        self.assertFalse((self.root / 'pending.json').exists())

    def test_migration_intent_write_failure_stays_stopped_without_ddl(self):
        save = manage.save
        def fail_transition(path, value):
            if path.name == 'pending.json' and value.get('phase') == 'migrating':
                raise OSError('migration intent not durable')
            save(path, value)
        with patch.object(manage, 'save', side_effect=fail_transition), self.assertRaises(OSError):
            manage.apply_update(self.root)
        self.assertEqual(json.loads((self.root / 'pending.json').read_text())['phase'], 'preparing')
        self.assertFalse(any('migrate' in event or 'up' in event for event in self.events))
        self.assertFalse(any('enable' in event for event in self.events))
        self.assertEqual(manage.configuration(self.root), self.old)
        manage.apply_update(self.root)
        self.assertEqual(manage.configuration(self.root), self.new)

    def test_pending_active_runs_leave_working_services_untouched(self):
        pending = {'operation': 'update', 'phase': 'migrating', 'previous': self.old,
                   'target': self.new, 'backup': str(self.backup)}
        manage.save(self.root / 'pending.json', pending)
        self.idle.side_effect = ValueError('candidate is processing a run')
        with self.assertRaises(ValueError):
            manage.apply_update(self.root)
        self.assertEqual(self.events, [])
        self.assertEqual(json.loads((self.root / 'pending.json').read_text()), pending)
        self.backup_call.assert_not_called()

    def test_preparing_retry_with_racing_run_does_not_stop_processors(self):
        pending = {'operation': 'update', 'phase': 'preparing',
                   'previous': self.old, 'target': self.new, 'backup': None}
        manage.save(self.root / 'pending.json', pending)
        self.idle.side_effect = ValueError('old run admitted before interrupted shutdown')
        with self.assertRaises(ValueError):
            manage.apply_update(self.root)
        self.assertIn(('systemctl', '--user', 'enable', '--now', manage.unit(self.old)), self.events)
        self.assertIn((self.old['version'], 'up', '-d', '--no-recreate', '--wait', '--wait-timeout', '120', 'app'), self.events)
        self.assertIn(('app-restarts', self.old['version'], True), self.events)
        self.assertFalse(any('migrate' in event or 'stop' in event or 'disable' in event for event in self.events))
        self.assertNotIn(('app-restarts', self.old['version'], False), self.events)
        self.assertEqual(json.loads((self.root / 'pending.json').read_text()), pending)
        self.backup_call.assert_not_called()

    def test_activated_install_marker_is_finalized_without_migration(self):
        manage.save(self.root / 'pending.json', {'operation': 'install', 'version': self.old['version']})
        (self.root / 'current').symlink_to(self.root / 'releases' / self.old['version'], target_is_directory=True)
        with patch.object(manage, 'manifest', return_value=metadata(self.old['version'])), patch.object(
            manage, 'verify_worker'
        ), patch.object(manage, 'initialize_state') as initialize:
            manage.deploy(self.root, argparse.Namespace())
        initialize.assert_not_called()
        manage.prepare.assert_not_called()
        self.idle.assert_not_called()
        self.assertNotIn('worker-configure', self.events)
        self.assertFalse(any('migrate' in event or 'stop' in event or 'disable' in event for event in self.events))
        self.assertIn((self.old['version'], 'up', '-d', '--no-recreate', '--wait', '--wait-timeout', '120', 'app'), self.events)
        self.assertEqual(manage.configuration(self.root), self.old)
        self.assertFalse((self.root / 'pending.json').exists())

    def test_activated_install_recovery_failure_keeps_marker_without_ddl(self):
        pending = {'operation': 'install', 'version': self.old['version']}
        manage.save(self.root / 'pending.json', pending)
        (self.root / 'current').symlink_to(self.root / 'releases' / self.old['version'], target_is_directory=True)
        with patch.object(manage, 'manifest', return_value=metadata(self.old['version'])), patch.object(
            manage, 'verify_worker', side_effect=ValueError('original Worker not healthy')
        ), self.assertRaises(ValueError):
            manage.deploy(self.root, argparse.Namespace())
        manage.prepare.assert_not_called()
        self.assertFalse(any('migrate' in event or 'stop' in event or 'disable' in event for event in self.events))
        self.assertNotIn(('app-restarts', self.old['version'], True), self.events)
        self.assertEqual(json.loads((self.root / 'pending.json').read_text()), pending)
        self.assertEqual(manage.configuration(self.root), self.old)

    def test_every_container_build_base_has_an_exact_digest(self):
        bases = [line.split()[1] for name in ('Dockerfile', 'Codex.Dockerfile')
                 for line in (manage.BUNDLE / name).read_text().splitlines() if line.startswith('FROM ')]
        self.assertEqual(len(bases), 6)
        for image in bases:
            self.assertRegex(image, r'^[^ ]+@sha256:[a-f0-9]{64}\Z')
        self.assertEqual(bases[0], bases[4])

    def test_run_admitted_during_app_shutdown_keeps_worker_running(self):
        self.idle.side_effect = [None, ValueError('run admitted while API was closing')]
        with self.assertRaises(ValueError):
            manage.apply_update(self.root)
        self.assertIn((self.old['version'], 'stop', 'app'), self.events)
        self.assertNotIn(('systemctl', '--user', 'disable', '--now', manage.unit(self.old)), self.events)
        self.assertIn(('systemctl', '--user', 'enable', '--now', manage.unit(self.old)), self.events)
        self.assertFalse(any('migrate' in event for event in self.events))
        self.backup_call.assert_not_called()
        self.assertFalse((self.root / 'pending.json').exists())

    def test_initial_pre_current_interruption_retries_without_ddl_or_worker_rewrite(self):
        manage.prepare.return_value = self.old
        with patch.object(manage, 'manifest', return_value=metadata(self.old['version'])), patch.object(
            manage, 'require_install_stopped'
        ) as stopped, patch.object(manage, 'initialize_state') as initialize:
            with patch.object(manage, 'activate', side_effect=KeyboardInterrupt('stopped before current')), self.assertRaises(KeyboardInterrupt):
                manage.deploy(self.root, argparse.Namespace())
            pending = json.loads((self.root / 'pending.json').read_text())
            self.assertEqual(pending['phase'], 'starting')
            self.assertFalse((self.root / 'current').exists())
            self.assertEqual(sum('migrate' in event for event in self.events), 1)
            self.assertEqual(self.events.count('worker-configure'), 1)
            self.events.clear()
            manage.deploy(self.root, argparse.Namespace())
            self.assertFalse(any('migrate' in event or 'disable' in event or 'stop' in event for event in self.events))
            self.assertNotIn('worker-configure', self.events)
            initialize.assert_called_once()
            stopped.assert_called_once()
            manage.prepare.assert_called_once()
        self.assertFalse((self.root / 'pending.json').exists())
        self.assertEqual(manage.configuration(self.root), self.old)

    def test_legacy_initial_processors_block_ddl_before_initialization(self):
        manage.prepare.return_value = self.old
        with patch.object(manage, 'manifest', return_value=metadata(self.old['version'])), patch.object(
            manage, 'require_install_stopped', side_effect=ValueError('legacy Worker is live')
        ), patch.object(manage, 'initialize_state') as initialize, self.assertRaises(ValueError):
            manage.deploy(self.root, argparse.Namespace())
        initialize.assert_not_called()
        self.assertFalse(any('migrate' in event or 'stop' in event or 'disable' in event for event in self.events))
        self.assertNotIn('worker-configure', self.events)
        self.assertTrue((self.root / 'pending.json').exists())

    def test_starting_intent_is_durable_before_any_candidate_start(self):
        save = manage.save
        def fail_starting(path, value):
            if path.name == 'pending.json' and value.get('phase') == 'starting':
                raise OSError('starting intent not durable')
            save(path, value)
        with patch.object(manage, 'save', side_effect=fail_starting), self.assertRaises(OSError):
            manage.apply_update(self.root)
        pending = json.loads((self.root / 'pending.json').read_text())
        self.assertEqual(pending['phase'], 'migrating')
        self.assertEqual(pending['backup'], str(self.backup))
        self.assertFalse(any('start' in event or 'enable' in event or ('up' in event and '--no-start' not in event)
                             for event in self.events))
        self.assertIn((self.new['version'], 'up', '--no-start', '--no-deps', 'app'), self.events)
        self.assertIn('worker-configure', self.events)
        self.assertEqual(manage.configuration(self.root), self.old)

    def test_candidate_configuration_failure_stays_stopped_with_original_backup(self):
        with patch.object(manage, 'configure_worker', side_effect=OSError('unit file sync failed')), self.assertRaises(OSError):
            manage.apply_update(self.root)
        pending = json.loads((self.root / 'pending.json').read_text())
        self.assertEqual(pending['phase'], 'migrating')
        self.assertEqual(pending['backup'], str(self.backup))
        self.assertFalse(any('start' in event or 'up' in event or 'enable' in event for event in self.events))
        manage.apply_update(self.root)
        self.backup_call.assert_called_once()
        self.assertEqual(manage.configuration(self.root), self.new)

    def test_starting_update_retry_preserves_candidate_processors_without_ddl(self):
        with patch.object(manage, 'activate', side_effect=KeyboardInterrupt('before current switch')), self.assertRaises(KeyboardInterrupt):
            manage.apply_update(self.root)
        pending = json.loads((self.root / 'pending.json').read_text())
        self.assertEqual(pending['phase'], 'starting')
        self.assertEqual(pending['backup'], str(self.backup))
        self.assertEqual(pending['target'], self.new)
        self.assertFalse((self.root / 'current').exists())
        self.events.clear()
        self.idle.reset_mock()
        self.idle.side_effect = ValueError('candidate is already processing work')
        manage.apply_update(self.root)
        self.idle.assert_not_called()
        self.assertFalse(any('migrate' in event or 'stop' in event or 'disable' in event for event in self.events))
        self.assertNotIn('worker-configure', self.events)
        self.backup_call.assert_called_once()
        self.assertEqual(manage.configuration(self.root), self.new)
        self.assertFalse((self.root / 'pending.json').exists())


if __name__ == "__main__":
    unittest.main()
