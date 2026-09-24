#!/usr/bin/env python3
"""Exercise real deployment in a new disposable directory; never use an existing installation."""
from __future__ import annotations

import argparse
import contextlib
import hashlib
import json
import os
from pathlib import Path
import socket
import subprocess
import sys
import tarfile
import tempfile
import urllib.error
import urllib.request

import manage
import release


def ports() -> tuple[int, int]:
    with socket.socket() as web, socket.socket() as database:
        web.bind(("127.0.0.1", 0))
        database.bind(("127.0.0.1", 0))
        return web.getsockname()[1], database.getsockname()[1]


def make_bundle(root: Path, tag: str, revision: str, image: str) -> Path:
    assets, bundle = root / (tag + "-assets"), root / tag
    release.bundle(tag, revision, image, assets)
    bundle.mkdir()
    manage.unpack((assets / "quazonai-deploy.tar.gz").read_bytes(), bundle)
    return bundle


def invoke(bundle: Path, command: str, root: Path, *args: str, succeeds: bool = True) -> None:
    result = subprocess.run([sys.executable, "-B", str(bundle / "manage.py"), command,
                             "--directory", str(root), *args], check=False)
    if (result.returncode == 0) != succeeds:
        raise AssertionError(f"Unexpected {command} exit status: {result.returncode}")


def fingerprint(root: Path) -> str:
    return hashlib.sha256((root / "data/state/master.key").read_bytes()).hexdigest()


def verify_native_sandbox(root: Path, config: dict) -> None:
    binaries = Path(config['root']) / 'releases' / config['version'] / 'bin'
    with tempfile.TemporaryDirectory(prefix='sandbox-probe-', dir=root) as temporary:
        home = Path(temporary)
        (home / 'codex-home').mkdir()
        # An empty native home and a PATH without system bwrap force the packaged
        # helper to be exercised. This executes no model and uses no real account.
        environment = {
            'HOME': str(home), 'CODEX_HOME': str(home / 'codex-home'),
            'PATH': str(binaries), 'LANG': 'C.UTF-8',
        }
        result = subprocess.run(
            [str(binaries / 'codex'), '--disable', 'use_legacy_landlock',
             '-c', 'sandbox_mode="read-only"', 'sandbox', '--', '/usr/bin/true'],
            cwd=home, env=environment, capture_output=True, text=True, timeout=60, check=False,
        )
        if result.returncode != 0:
            print(result.stdout[-8000:], file=sys.stderr)
            print(result.stderr[-8000:], file=sys.stderr)
            raise AssertionError(f'Packaged Codex sandbox failed: {result.returncode}')


def verify_orphaned_volume(root: Path, bundle: Path) -> None:
    installation = root / 'orphaned-installation'
    project = 'quazonai-' + hashlib.sha256(str(installation).encode()).hexdigest()[:12]
    volume = project + '_postgres'
    web, database = ports()
    manage.run(['docker', 'volume', 'create', '--label', 'com.docker.compose.project=' + project, volume], capture=True)
    try:
        invoke(bundle, 'deploy', installation, '--port', str(web), '--database-port', str(database),
               '--codex-home', str(root / 'orphan-native-home'), succeeds=False)
        assert not (installation / 'installation.json').exists()
        assert not (installation / 'data').exists()
        manage.run(['docker', 'volume', 'inspect', volume], capture=True)
    finally:
        # Only the unused volume created by this test is removed.
        manage.run(['docker', 'volume', 'rm', volume], capture=True)


def verify_app_restart(config: dict, expected: str) -> None:
    container = manage.compose(config, 'ps', '--all', '--quiet', 'app', capture=True)
    assert container and '\n' not in container
    policy = manage.run(['docker', 'inspect', '--format', '{{.HostConfig.RestartPolicy.Name}}', container], capture=True)
    assert policy == expected, (policy, expected)


def exercise(root: Path, image: str, revision: str) -> None:
    installation = root / "installation with spaces [native] %n $HOME"
    web_port, database_port = ports()
    one = make_bundle(root, "v0.0.0-ci.1", revision, image)
    two = make_bundle(root, "v0.0.0-ci.2", revision, image)
    three = make_bundle(root, "v0.0.0-ci.3", revision, image)
    verify_orphaned_volume(root, one)
    overlapping = root / 'overlapping-installation'
    invoke(one, 'deploy', overlapping, '--port', str(web_port), '--database-port', str(database_port),
           '--codex-home', str(overlapping / 'data/state/native'), succeeds=False)
    assert not (overlapping / 'installation.json').exists()
    assert not (overlapping / 'data').exists()
    try:
        invoke(one, "deploy", installation, "--port", str(web_port), "--database-port", str(database_port),
               "--codex-home", str(root / "native-home"))
        original = manage.configuration(installation)
        verify_app_restart(original, 'unless-stopped')
        verify_native_sandbox(root, original)
        key = fingerprint(installation)
        assert manage.sql(original, "SELECT extversion FROM pg_extension WHERE extname='pgmq'") == "1.10.0"
        manage.sql(original, "CREATE TABLE public.container_release_smoke (value text PRIMARY KEY); "
                             "INSERT INTO public.container_release_smoke VALUES ('persisted')")
        request = urllib.request.Request(f"http://localhost:{web_port}/assets/does-not-exist.js",
                                         headers={"Accept": "text/html"})
        try:
            urllib.request.build_opener(urllib.request.ProxyHandler({})).open(request, timeout=10)
            raise AssertionError("Missing assets must not fall back to index.html")
        except urllib.error.HTTPError as error:
            assert error.code == 404
        invoke(one, "deploy", installation)
        assert manage.configuration(installation)["password"] == original["password"]
        assert fingerprint(installation) == key

        invoke(two, "apply-update", installation)
        updated = manage.configuration(installation)
        assert updated["version"] == "v0.0.0-ci.2"
        assert updated["password"] == original["password"]
        assert updated["project"] == original["project"]
        assert fingerprint(installation) == key
        assert manage.sql(updated, "SELECT value FROM public.container_release_smoke") == "persisted"
        backups = list((installation / "backups").iterdir())
        assert len(backups) == 1
        pid = manage.run(['systemctl', '--user', 'show', manage.unit(updated), '--property=MainPID', '--value'], capture=True)
        invoke(one, 'apply-update', installation, succeeds=False)
        assert manage.configuration(installation)['version'] == 'v0.0.0-ci.2'
        assert not (installation / 'pending.json').exists()
        assert len(list((installation / 'backups').iterdir())) == 1
        assert manage.run(['systemctl', '--user', 'show', manage.unit(updated), '--property=MainPID', '--value'], capture=True) == pid
        manage.verify_console(updated)
        with tarfile.open(backups[0] / "data.tar.gz") as archive:
            assert "data/state/master.key" not in archive.getnames()
            assert "data/state/session-key.ref" in archive.getnames()
        assert (backups[0] / "database.dump").read_bytes().startswith(b"PGDMP")

        # Cause a real PostgreSQL connection/migration failure after a recovery
        # point, then remove only this test override and retry the same target.
        override = installation / "compose.override.yaml"
        override.write_text("services:\n  app:\n    environment:\n      DATABASE_URL: postgresql://quazonai:unusable@database:5432/missing\n")
        invoke(three, "apply-update", installation, succeeds=False)
        pending = json.loads((installation / "pending.json").read_text())
        recovery = pending["backup"]
        assert manage.configuration(installation)["version"] == "v0.0.0-ci.2"
        assert fingerprint(installation) == key
        assert manage.compose(updated, "ps", "--status", "running", "-q", "app", capture=True) == ""
        active = subprocess.run(["systemctl", "--user", "is-active", "--quiet", manage.unit(updated)], check=False)
        assert active.returncode != 0
        enabled = subprocess.run(['systemctl', '--user', 'is-enabled', manage.unit(updated)], capture_output=True, text=True, check=False)
        assert enabled.returncode != 0 and enabled.stdout.strip() == 'disabled'
        verify_app_restart(updated, 'no')
        override.unlink()
        # Reproduce the window after a candidate starts but before activation.
        # It must not gain daemon-start/automatic restart behavior while pending.
        manage.compose(pending['target'], 'up', '-d', '--wait', '--wait-timeout', '120', 'app')
        verify_app_restart(pending['target'], 'no')
        manage.compose(pending['target'], 'stop', 'app')
        invoke(three, "apply-update", installation)
        latest = manage.configuration(installation)
        verify_app_restart(latest, 'unless-stopped')
        assert latest["version"] == "v0.0.0-ci.3"
        assert not (installation / "pending.json").exists()
        assert len(list((installation / "backups").iterdir())) == 2
        assert Path(recovery).is_dir()
        assert fingerprint(installation) == key

        manage.run(["systemctl", "--user", "stop", manage.unit(latest)])
        manage.compose(latest, "restart", "database", "app")
        manage.compose(latest, "up", "-d", "--wait", "--wait-timeout", "120")
        manage.run(["systemctl", "--user", "start", manage.unit(latest)])
        manage.verify_worker(latest)
        manage.verify_console(latest)
        assert fingerprint(installation) == key
        assert manage.sql(latest, "SELECT value FROM public.container_release_smoke") == "persisted"

        # Actually restore the saved dump into another database in the disposable
        # PG instance. The active application's database is not overwritten.
        manage.compose(latest, "exec", "-T", "database", "createdb", "-U", "quazonai", "release_restore")
        database = manage.compose(latest, "ps", "-q", "database", capture=True)
        with (backups[0] / "database.dump").open("rb") as stream:
            subprocess.run(["docker", "exec", "-i", database, "pg_restore", "--exit-on-error", "-U", "quazonai",
                            "-d", "release_restore"], stdin=stream, check=True)
        restored = manage.compose(latest, "exec", "-T", "database", "psql", "-X", "-U", "quazonai", "-d", "release_restore",
                                   "-Atc", "SELECT value FROM public.container_release_smoke", capture=True)
        assert restored == "persisted"
        print(f"Real install/update/failure-retry/restart/PG-restore passed: {revision}")
    finally:
        if (installation / "installation.json").exists():
            config = manage.configuration(installation)
            # Cleanup identities come only from the new temporary installation.
            override = installation / "compose.override.yaml"
            override.unlink(missing_ok=True)
            if sys.exc_info()[0] is not None:
                with contextlib.suppress(subprocess.CalledProcessError):
                    manage.compose(config, "logs", "--no-color", "--tail", "100")
                subprocess.run(["journalctl", "--user-unit", manage.unit(config), "--no-pager", "-n", "50"], check=False)
            subprocess.run(["systemctl", "--user", "disable", "--now", manage.unit(config)], check=False)
            (Path(config["unit_directory"]) / manage.unit(config)).unlink(missing_ok=True)
            manage.run(["systemctl", "--user", "daemon-reload"])
            manage.compose(config, "down", "--volumes", "--remove-orphans")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--image", required=True)
    parser.add_argument("--revision", required=True)
    args = parser.parse_args()
    os.umask(0o077)
    image = manage.run(["docker", "image", "inspect", "--format", "{{.Id}}", args.image], capture=True)
    with tempfile.TemporaryDirectory(prefix="quazonai-container-", dir=os.environ.get("RUNNER_TEMP")) as temporary:
        exercise(Path(temporary), image, args.revision)


if __name__ == "__main__":
    main()
