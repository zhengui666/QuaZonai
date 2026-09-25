#!/usr/bin/env python3
"""Exercise real deployment in a new disposable directory; never use an existing installation."""
from __future__ import annotations

import argparse
import contextlib
import hashlib
import http.cookiejar
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
import uuid

import codex
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


def verify_container_codex(config: dict) -> None:
    binaries = Path(config['root']) / 'releases' / config['version'] / 'bin'
    assert not (binaries / 'codex').exists()
    manage.compose(config, 'exec', '-T', 'app', '/bin/sh', '-c', 'test ! -e /opt/quazonai/bin/codex')
    server = str(binaries / 'server')
    codex.docker(config, 'run', '--rm', '--network', 'none', '--read-only', '--cap-drop', 'ALL',
                 '--security-opt', 'no-new-privileges:true', '--user', f'{config["uid"]}:{config["gid"]}',
                 '--volume', server + ':' + server + ':ro', '--entrypoint', server,
                 config['codex_image'], '--version')
    # The actual running API must launch and initialize the separate container.
    # An unavailable deployment/transport is a failure, not equivalent to no login.
    origin = f'http://localhost:{config["port"]}'
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}),
                                        urllib.request.HTTPCookieProcessor(http.cookiejar.CookieJar()))
    with opener.open(origin + '/api/v2/auth/session', timeout=10) as response:
        assert response.status == 200
    with opener.open(origin + '/api/v2/settings/codex', timeout=10) as response:
        profiles = json.load(response)['items']
    assert profiles
    profile = profiles[0]
    body = json.dumps({'schema_version': 1, 'profile_id': profile['id'],
                       'expected_revision': profile['revision']}).encode()
    request = urllib.request.Request(origin + '/api/v2/codex/probe', data=body, headers={
        'Content-Type': 'application/json', 'Origin': origin, 'Idempotency-Key': str(uuid.uuid4()),
    })
    with opener.open(request, timeout=40) as response:
        outcome = json.load(response)['resource']['outcome']
    assert outcome == {'status': 'UNAVAILABLE', 'reason': 'AUTHENTICATION_REQUIRED'}, outcome
    codex.require_stopped(config)


def verify_codex_update(config: dict) -> None:
    root = Path(config['root'])
    home = Path(config['codex_home'])
    sentinel = home / 'container-deployment-smoke.txt'
    sentinel.write_text('persistent native directory, not an authentication fixture\n')
    before = sentinel.read_bytes()
    original = codex.image_id(config)
    assert codex.read_env(root / '.env')[0] == '0.156.1'
    target = codex.read_env(Path(config['bundle']) / '.env.example')[0]
    codex.update(root, target)
    assert codex.image_id(config) != original
    assert codex.read_env(root / '.env')[0] == target
    assert sentinel.read_bytes() == before
    verify_container_codex(config)
    # A real running container blocks image replacement even when no QZ Run owns
    # it, as happens with account operations. Only this test identity is stopped.
    container = codex.docker(config, 'run', '-d', '--rm', '--label',
                             'io.quazonai.codex.image=' + config['codex_image'],
                             '--entrypoint', '/usr/bin/sleep', config['codex_image'], '120', capture=True)
    try:
        selected = codex.image_id(config)
        try:
            codex.update(root, '0.156.1')
            raise AssertionError('An active Codex container did not block its version update')
        except ValueError as error:
            assert 'Codex sessions still exist' in str(error), str(error)
        assert codex.image_id(config) == selected
        assert codex.read_env(root / '.env')[0] == target
    finally:
        codex.docker(config, 'container', 'rm', '--force', container, capture=True)
    assert sentinel.read_bytes() == before


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


def verify_interrupted_shutdown(bundle: Path, installation: Path) -> None:
    # Terminate a separate installer process after the real Docker policy change,
    # without exception cleanup. Its durable pre-migration intent must survive.
    program = '''
import os
import sys
from pathlib import Path
sys.path.insert(0, sys.argv[1])
import manage
change_policy = manage.configure_app_restarts
def interrupt(config, enabled):
    change_policy(config, enabled)
    if not enabled:
        os._exit(99)
manage.configure_app_restarts = interrupt
manage.apply_update(Path(sys.argv[2]))
'''
    before = fingerprint(installation)
    result = subprocess.run([sys.executable, '-B', '-c', program, str(bundle), str(installation)],
                            check=False, timeout=120)
    assert result.returncode == 99, result.returncode
    pending = json.loads((installation / 'pending.json').read_text())
    assert pending['phase'] == 'preparing' and pending['backup'] is None
    assert pending['previous']['version'] == 'v0.0.0-ci.1'
    assert pending['target']['version'] == 'v0.0.0-ci.2'
    assert manage.configuration(installation)['version'] == 'v0.0.0-ci.1'
    verify_app_restart(pending['previous'], 'no')
    assert fingerprint(installation) == before


def processor_identity(config: dict) -> tuple[str, str]:
    pid = manage.run(['systemctl', '--user', 'show', manage.unit(config),
                      '--property=MainPID', '--value'], capture=True)
    container = manage.compose(config, 'ps', '--quiet', 'app', capture=True)
    assert pid.isdigit() and pid != '0' and container
    return pid, container


def faulted_invoke(bundle: Path, mode: str, command: str, installation: Path, *args: str) -> None:
    # Only the installer boundary is interrupted. Docker, systemd, PostgreSQL,
    # the real server and migrations remain the actual release implementations.
    program = '''
import os
import sys
sys.path.insert(0, sys.argv[1])
import manage
mode = sys.argv[2]
if mode == 'before-activation':
    manage.activate = lambda *args: os._exit(99)
elif mode == 'resume-without-ddl':
    original = manage.compose
    def compose(config, *args, **kwargs):
        assert 'migrate' not in args, 'starting retry repeated DDL'
        return original(config, *args, **kwargs)
    def configure(config):
        raise AssertionError('starting retry rewrote the Worker')
    manage.compose, manage.configure_worker = compose, configure
elif mode == 'race-after-admissions-close':
    original_idle = manage.require_idle
    calls = 0
    def idle(config):
        global calls
        original_idle(config)
        calls += 1
        if calls == 2:
            raise ValueError('test interruption at the post-admission idle boundary')
    manage.require_idle = idle
sys.argv = ['manage.py', sys.argv[3], '--directory', sys.argv[4], *sys.argv[5:]]
manage.main()
'''
    result = subprocess.run([sys.executable, '-B', '-c', program, str(bundle), mode, command,
                             str(installation), *args], check=False, timeout=240)
    expected = {'before-activation': 99, 'resume-without-ddl': 0, 'race-after-admissions-close': 1}[mode]
    assert result.returncode == expected, (mode, result.returncode)


def exercise(root: Path, image: str, revision: str) -> None:
    installation = root / "installation with spaces [native] %n $HOME"
    installation.mkdir()
    (installation / '.env').write_text('CODEX_VERSION=0.156.1\n')
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
        faulted_invoke(one, 'before-activation', 'deploy', installation,
                       '--port', str(web_port), '--database-port', str(database_port),
                       '--codex-home', str(root / 'native-home'))
        original = manage.configuration(installation)
        assert json.loads((installation / 'pending.json').read_text())['phase'] == 'starting'
        assert not (installation / 'current').is_symlink()
        initial_processors = processor_identity(original)
        faulted_invoke(one, 'resume-without-ddl', 'deploy', installation)
        assert processor_identity(original) == initial_processors
        assert not (installation / 'pending.json').exists()
        verify_app_restart(original, 'unless-stopped')
        verify_container_codex(original)
        verify_codex_update(original)
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

        # A crash can leave the initial-install marker after current and both
        # services are active. Retrying must finalize, not rerun migration or
        # replace either live processor.
        worker_pid = manage.run(['systemctl', '--user', 'show', manage.unit(original),
                                 '--property=MainPID', '--value'], capture=True)
        app_id = manage.compose(original, 'ps', '-q', 'app', capture=True)
        manage.save(installation / 'pending.json', {'operation': 'install', 'version': original['version']})
        invoke(one, 'deploy', installation)
        assert not (installation / 'pending.json').exists()
        assert manage.run(['systemctl', '--user', 'show', manage.unit(original),
                           '--property=MainPID', '--value'], capture=True) == worker_pid
        assert manage.compose(original, 'ps', '-q', 'app', capture=True) == app_id
        assert fingerprint(installation) == key

        # Inject only the boundary observation after the real API stops. The
        # existing Worker must keep its actual PID while the API is restored.
        before_race = processor_identity(original)
        faulted_invoke(two, 'race-after-admissions-close', 'apply-update', installation)
        assert processor_identity(original) == before_race
        assert not (installation / 'pending.json').exists()
        manage.verify_console(original)

        verify_interrupted_shutdown(two, installation)
        faulted_invoke(two, 'before-activation', 'apply-update', installation)
        starting = json.loads((installation / 'pending.json').read_text())
        assert starting['phase'] == 'starting' and starting['backup']
        candidate_processors = processor_identity(starting['target'])
        faulted_invoke(two, 'resume-without-ddl', 'apply-update', installation)
        assert processor_identity(starting['target']) == candidate_processors
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
        assert pending['phase'] == 'migrating'
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
            if config.get('codex_image'):
                with contextlib.suppress(subprocess.CalledProcessError):
                    codex.docker(config, 'image', 'rm', config['codex_image'], capture=True)


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
