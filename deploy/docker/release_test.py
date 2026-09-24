"""Focused release/upgrade regressions; service acceptance lives in smoke.py."""
import argparse
import ast
import contextlib
import json
import os
from pathlib import Path
import socket
import subprocess
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
            with patch.object(manage, "BUNDLE", source), patch.object(manage, "preflight"), patch.object(manage, "validate_ports"), patch.object(
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
                manage.start_worker(config)
            content = (root / "units/release-unit-test.service").read_text()
            self.assertIn(f"WorkingDirectory={str(root).replace('%', '%%')}/data\n", content)
            self.assertIn(f"EnvironmentFile={str(root).replace('%', '%%')}/worker.env\n", content)
            self.assertIn('ExecStart=:"', content)
            self.assertIn('PUBLIC_URL="http://localhost:8081"\n', (root / "worker.env").read_text())
            for path in (Path("relative"), Path("/bad\npath")):
                with self.assertRaises(ValueError):
                    manage.unit_path(path)


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

    def test_native_package_requires_executable_sandbox_resource(self):
        with tempfile.TemporaryDirectory() as temporary, patch.object(manage, 'run') as command:
            binaries = Path(temporary)
            helper = binaries / 'codex-resources/bwrap'
            with self.assertRaisesRegex(ValueError, 'sandbox resource'):
                manage.verify_native_binaries(binaries)
            command.assert_not_called()
            helper.parent.mkdir()
            helper.write_bytes(b'test-only package entry; never executed')
            helper.chmod(0o600)
            with self.assertRaisesRegex(ValueError, 'sandbox resource'):
                manage.verify_native_binaries(binaries)
            command.assert_not_called()
            helper.chmod(0o755)
            manage.verify_native_binaries(binaries)
            self.assertEqual([call.args[0] for call in command.call_args_list], [
                [str(binaries / 'server'), '--version'], [str(binaries / 'codex'), '--version'],
                [str(helper), '--version'],
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
        self.stack.enter_context(patch.object(manage, "start_worker", side_effect=lambda c: self.events.append("worker-start")))
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
        self.assertNotIn("worker-start", self.events)
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
        self.assertIn("worker-start", self.events)
        self.assertGreater(self.events.index(('systemctl', '--user', 'enable', manage.unit(self.new))), self.events.index('worker-start'))


if __name__ == "__main__":
    unittest.main()
