#!/usr/bin/env python3
"""Exercise real deployment in a new disposable directory; never use an existing installation."""
from __future__ import annotations

import argparse
import contextlib
import datetime
import hashlib
import http.cookiejar
import json
import os
from pathlib import Path
import re
import signal
import socket
import secrets
import shutil
import sqlite3
import stat
import time
import subprocess
import sys
import tarfile
import tempfile
import urllib.error
import urllib.request
import urllib.parse
import uuid
import zipfile

import codex
import manage
import release

# One disposable password per smoke process, retained across its upgrades.
# Never write it into the installation manifest, output or model context.
SMOKE_PASSWORD = secrets.token_urlsafe(32)


def ports() -> tuple[int, int]:
    with socket.socket() as web, socket.socket() as database:
        web.bind(("127.0.0.1", 0))
        database.bind(("127.0.0.1", 0))
        return web.getsockname()[1], database.getsockname()[1]


def make_bundle(root: Path, tag: str, revision: str, images: dict) -> Path:
    assets, bundle = root / (tag + "-assets"), root / tag
    release.bundle(tag, revision, images["image"], assets,
                   runtime_image=images["runtime_image"], codex_version=images["codex_version"],
                   codex_image=images["codex_image"])
    bundle.mkdir()
    manage.unpack((assets / "quazonai-deploy.tar.gz").read_bytes(), bundle)
    assert not (bundle / "Codex.Dockerfile").exists()
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
    assert (binaries / 'quazonai').samefile(binaries / 'server')
    for args in (['client', '--help'], ['client', 'forward', 'weights', 'submit', '--help'],
                 ['client', 'forward', 'messages', 'submit', '--help']):
        help_text = manage.run([str(binaries / 'quazonai'), *args], capture=True)
        assert 'Usage: quazonai' in help_text
        container_help = manage.compose(config, 'exec', '-T', 'app', 'quazonai', *args, capture=True)
        assert 'Usage: quazonai' in container_help
    assert not (binaries / 'codex').exists()
    manage.compose(config, 'exec', '-T', 'app', '/bin/sh', '-c', 'test ! -e /opt/quazonai/bin/codex')
    server = str(binaries / 'server')
    codex.docker(config, 'run', '--rm', '--network', 'none', '--read-only', '--cap-drop', 'ALL',
                 '--security-opt', 'no-new-privileges:true', '--user', f'{config["uid"]}:{config["gid"]}',
                 '--volume', server + ':' + server + ':ro', '--entrypoint', server,
                 codex.image_tag(config), '--version')
    # The actual running API must launch and initialize the separate container.
    # An unavailable deployment/transport is a failure, not equivalent to no login.
    origin = f'http://localhost:{config["port"]}'
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}),
                                        urllib.request.HTTPCookieProcessor(http.cookiejar.CookieJar()))
    with opener.open(origin + '/api/v2/auth/status', timeout=10) as response:
        setup = json.load(response)['setup_required']
    login = urllib.request.Request(origin + ('/api/v2/auth/setup' if setup else '/api/v2/auth/login'),
                                   data=json.dumps({'schema_version': 1, 'password': SMOKE_PASSWORD,
                                                    'remember_device': False}).encode(),
                                   headers={'Content-Type': 'application/json', 'Origin': origin})
    with opener.open(login, timeout=10) as response:
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


def verify_codex_update(config: dict, selected: tuple[str, str], previous: tuple[str, str]) -> None:
    root, home = Path(config['root']), Path(config['codex_home'])
    sentinel = home / 'container-deployment-smoke.txt'
    sentinel.write_text('persistent native directory, not an authentication fixture\n')
    before = sentinel.read_bytes()
    original = codex.image_id(config)
    assert codex.read_env(root / '.env')[0] == previous[0]
    codex.update(root, selected[0], reference=selected[1])
    if selected != previous:
        assert codex.image_id(config) != original
    assert codex.read_env(root / '.env')[0] == selected[0]
    assert sentinel.read_bytes() == before
    verify_container_codex(config)
    container = codex.docker(config, 'run', '-d', '--rm', '--label',
                             'io.quazonai.codex.image=' + codex.image_tag(config),
                             '--entrypoint', '/usr/bin/sleep', codex.image_tag(config), '120', capture=True)
    try:
        identity = codex.image_id(config)
        try:
            codex.update(root, previous[0], reference=previous[1])
            raise AssertionError('An active Codex container did not block its version update')
        except ValueError as error:
            assert 'Codex sessions still exist' in str(error), str(error)
        assert codex.image_id(config) == identity
        assert codex.read_env(root / '.env')[0] == selected[0]
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


def exercise(root: Path, images: dict, revision: str, previous: tuple[str, str]) -> None:
    installation = root / "installation with spaces [native] %n $HOME"
    installation.mkdir()
    (installation / '.env').write_text('CODEX_VERSION=' + previous[0] + '\n')
    web_port, database_port = ports()
    original_images = {**images, "codex_version": previous[0], "codex_image": previous[1]}
    one = make_bundle(root, "v0.0.0-ci.1", revision, original_images)
    two = make_bundle(root, "v0.0.0-ci.2", revision, images)
    three = make_bundle(root, "v0.0.0-ci.3", revision, images)
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
        verify_codex_update(original, (images["codex_version"], images["codex_image"]), previous)
        verify_runtime(original)
        verify_installed_sources(original)
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
        verify_runtime(latest)
        verify_installed_sources(latest)
        print(f"Real install/update/failure-retry/restart/PG-restore passed: {revision}")
    finally:
        cleanup_installation(installation)


def cleanup_installation(installation: Path) -> None:
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
                codex.docker(config, 'image', 'rm', codex.image_tag(config), capture=True)


@contextlib.contextmanager
def installation_tools(directory: Path):
    """Fail, rather than silently succeed, if the installer tries to build anything."""
    directory.mkdir()
    previous = os.environ["PATH"]
    docker = shutil.which("docker")
    assert docker
    log = directory / "commands.jsonl"
    forbidden = directory / "forbidden"
    wrapper = """#!/usr/bin/python3
import json, os, sys
from pathlib import Path
args = sys.argv[1:]
name = Path(sys.argv[0]).name
blocked = name != "docker" or not args or args[0] in ("build", "buildx", "builder") or args[:2] == ["image", "build"] or "--build" in args
with open({log!r}, "a") as stream:
    stream.write(json.dumps({{"tool": name, "operation": args[0] if args else "", "blocked": blocked}}) + "\\n")
if blocked:
    Path({forbidden!r}).write_text("deployment attempted a build")
    sys.exit(97)
os.execv({docker!r}, [{docker!r}, *args])
""".format(log=str(log), forbidden=str(forbidden), docker=docker)
    for name in ("docker", "cargo", "rustup", "rustc", "npm", "npx", "node", "make", "cmake", "gcc", "cc", "clang"):
        path = directory / name
        path.write_text(wrapper)
        path.chmod(0o755)
    os.environ["PATH"] = str(directory) + os.pathsep + previous
    try:
        yield log
        assert not forbidden.exists(), "Deployment tried to compile or build an image"
    finally:
        os.environ["PATH"] = previous


def fixture_id() -> str:
    # UUIDv7 test identities; independent state and cleanup retain these exact IDs.
    value = bytearray(int(time.time() * 1000).to_bytes(6, "big") + os.urandom(10))
    value[6] = (value[6] & 0x0f) | 0x70
    value[8] = (value[8] & 0x3f) | 0x80
    return str(uuid.UUID(bytes=bytes(value)))


def source_fixture(root: Path, plan: dict) -> None:
    """Synthetic provider-shaped bytes only; native tools still do the conversion."""
    acquired = root / 'acquired'
    (acquired / 'raw').mkdir(parents=True)
    encoded = lambda value: (json.dumps(value, sort_keys=True, separators=(',', ':'), ensure_ascii=False) + '\n').encode()
    record = lambda name, body: {'path': name, 'size': len(body), 'sha256': hashlib.sha256(body).hexdigest()}
    observed = '2024-01-02T00:00:01Z'
    values = ['42000.01', '42001.02', '41999.00', '42000.99', '0.10000001']
    raw = ('[' + ','.join(f'[{index * 60},{values[2]},{values[1]},{values[0]},{values[3]},{values[4]}]'
                         for index in range(3)) + ']').encode()
    rows = [{'selection_time_seconds': index * 60, 'event_time_seconds': (index + 1) * 60,
             'open': values[0], 'high': values[1], 'low': values[2], 'close': values[3], 'volume': values[4],
             'instrument': 'BTC-USD', 'kind': 'OHLCV_CANDLE', 'interval_seconds': 60,
             'observed_at': observed, 'historical_available_at': None,
             'source_response': 'raw/0000.json', 'source_row_index': index} for index in range(3)]
    records = b''.join(encoded(row) for row in rows)
    terms = b'Synthetic provider terms for deterministic installed native tests only.'
    for name, body in [('raw/0000.json', raw), ('records.jsonl', records), ('source-terms.bin', terms)]:
        (acquired / name).write_bytes(body)
    manifest = {**plan, 'created_at': '2024-01-02T00:00:02Z',
                'source_terms': {'file': record('source-terms.bin', terms),
                                 'reference': plan['provider']['terms_reference'],
                                 'evidence_status': 'OPERATOR_SUPPLIED_NOT_INDEPENDENTLY_VERIFIED'},
                'responses': [{'request': plan['requests'][0], 'observation': {
                    'status': 200, 'headers': {'Content-Type': 'application/json'},
                    'request_started_at': '2024-01-02T00:00:00Z', 'retrieved_at': observed},
                    'file': record('raw/0000.json', raw),
                    'counts': {'source_rows': 3, 'selected_rows': 3, 'outside_request_window': 0}}],
                'records': record('records.jsonl', records), 'record_count': 3,
                'raw_response_bytes': len(raw), 'observation_status': 'OBSERVED'}
    (acquired / 'acquisition.json').write_bytes(encoded(manifest))
    # Native CurrencyPair Serde shape from the existing candle_acquisition fixture.
    # Every value here is explicitly a test fixture, never a historical definition.
    instrument = {'id': 'BTC-USD.COINBASE', 'raw_symbol': 'BTC-USD', 'base_currency': 'BTC',
                  'quote_currency': 'USD', 'price_precision': 2, 'size_precision': 8,
                  'price_increment': '0.01', 'size_increment': '0.00000001', 'multiplier': '1',
                  'lot_size': None, 'margin_init': '0', 'margin_maint': '0',
                  'maker_fee': '0.004', 'taker_fee': '0.006', 'max_quantity': None,
                  'min_quantity': None, 'max_notional': None, 'min_notional': None,
                  'max_price': None, 'min_price': None, 'tick_scheme': None, 'info': None,
                  'ts_event': 0, 'ts_init': 0}
    (root / 'instruments.json').write_bytes(encoded([{'CurrencyPair': instrument}]))
    selection = {'dataset_revision_id': fixture_id(), 'settlements': [], 'selection': {
        'schema_version': 1, 'bar_types': ['BTC-USD.COINBASE-1-MINUTE-LAST-EXTERNAL'],
        'event_start_ns': '60000000000', 'event_end_ns': '240000000000',
        'decision_cutoff_ns': '1704153601000000000', 'maximum_rows': 3}}
    (root / 'selection.json').write_bytes(encoded(selection))
    declaration = {'schema_version': 1, 'registered_ref': 'synthetic-installed-candles',
        'native_snapshot_ref': 'synthetic-installed-candles-discovery', 'storage_version': 'fixture-v1',
        'provider_kind': 'NAUTILUS_CATALOG', 'data_kind': 'BAR', 'partition': 'DISCOVERY',
        'event_start': '1970-01-01T00:01:00Z', 'event_end': '1970-01-01T00:04:00Z',
        'available_through': observed, 'origin': 'FIXTURE', 'pit_status': 'UNVERIFIED',
        'revision_policy': 'UNKNOWN', 'provenance_reference': 'SYNTHETIC_INSTALLED_SOURCE_TEST',
        'availability_provenance': 'Synthetic late REST batch; no historical publication evidence',
        'universe': {'name': 'Synthetic installed BTC-USD fixture', 'calendar_ref': 'synthetic-utc',
            'calendar_version': '1', 'selection_asof': '1970-01-01T00:00:00Z',
            'has_historical_membership': False, 'coverage_start': '1970-01-01T00:00:00Z',
            'coverage_end': '2024-01-03T00:00:00Z', 'membership': [{
                'instrument_id': 'BTC-USD.COINBASE', 'valid_from': '1970-01-01T00:00:00Z',
                'valid_until': None, 'available_at': '1970-01-01T00:00:00Z', 'groups': None}]}}
    (root / 'declaration.json').write_bytes(encoded(declaration))
    history_fixture(root, declaration, selection)


def history_fixture(root: Path, candle_declaration: dict, candle_selection: dict) -> None:
    """Original synthetic native records for offline packaged import and native readback."""
    identity = 'fixture-event-101.POLYMARKET'
    kind = identity + '-1-MINUTE-LAST-EXTERNAL'
    instrument = {'id': identity, 'raw_symbol': '101', 'asset_class': 'ALTERNATIVE',
        'currency': 'pUSD', 'activation_ns': 0, 'expiration_ns': 60000000000000,
        'price_precision': 4, 'size_precision': 6, 'price_increment': '0.0001',
        'size_increment': '0.000001', 'margin_init': '0', 'margin_maint': '0',
        'maker_fee': '0', 'taker_fee': '0', 'outcome': None, 'description': None,
        'max_quantity': None, 'min_quantity': None, 'max_notional': None, 'min_notional': None,
        'max_price': None, 'min_price': None, 'tick_scheme': None,
        'info': {'condition_id': 'fixture-event', 'token_id': '101',
                 'fee_schedule': {'rate': 0.0, 'exponent': 1, 'rebateRate': 0.2, 'takerOnly': True},
                 'source_reference': 'SYNTHETIC_INSTALLED_HISTORY_TEST'}, 'ts_event': 0, 'ts_init': 0}
    bars = [{'type': 'Bar', 'bar_type': kind, 'open': '0.4200', 'high': '0.4300',
             'low': '0.4100', 'close': '0.4250', 'volume': '2.000000',
             'ts_event': step * 60000000000, 'ts_init': step * 60000000000 + 1000}
            for step in range(1, 4)]
    archive = {'schema_version': 1, 'source_reference': 'FIXTURE: installed history import',
               'source_observed_at': '2024-01-02T00:00:01Z', 'source_metadata': {'origin': 'FIXTURE'},
               'instruments': [{'BinaryOption': instrument}], 'bars': bars}
    declaration = json.loads(json.dumps(candle_declaration))
    declaration.update(registered_ref='synthetic-installed-history',
                       native_snapshot_ref='synthetic-installed-history-discovery',
                       provenance_reference='SYNTHETIC_INSTALLED_HISTORY_TEST',
                       availability_provenance='Synthetic native clocks, not historical availability evidence')
    declaration['universe']['name'] = 'Synthetic installed binary-option fixture'
    declaration['universe']['membership'][0]['instrument_id'] = identity
    selection = json.loads(json.dumps(candle_selection))
    selection['selection']['bar_types'] = [kind]
    for name, value in [('history.json', archive), ('history-declaration.json', declaration),
                        ('history-selection.json', selection)]:
        (root / name).write_text(json.dumps(value) + '\n')


def archive_source_fixture(root: Path, template: Path, plan: dict) -> None:
    """Independent synthetic ZIP inputs; the installed freeze tool publishes the bundle."""
    assert plan['selection']['day'] == '2024-01-01' and plan['source_timestamp_unit'] == 'ms'
    start = int(plan['start_ns']) // 1_000_000
    rows = [[str(start + index * 60000), '42000.01', '42001.02', '41999.00', '42000.99',
             '0.10000001', str(start + (index + 1) * 60000 - 1), '4200.10', '3', '0.05', '2100.00', '0']
            for index in range(3)]
    body = ('\n'.join(','.join(row) for row in rows) + '\n').encode('ascii')
    archive = root / plan['archive_name']
    with zipfile.ZipFile(archive, 'w', compression=zipfile.ZIP_DEFLATED) as writer:
        writer.writestr(plan['member_name'], body)
    (root / (plan['archive_name'] + '.CHECKSUM')).write_text(
        f"{hashlib.sha256(archive.read_bytes()).hexdigest()}  {plan['archive_name']}\n")
    provenance = {'kind': 'SYNTHETIC', 'retrieval': {
        'checksum': {'started_at': '2024-01-02T00:00:00Z', 'completed_at': '2024-01-02T00:00:00Z'},
        'archive': {'started_at': '2024-01-02T00:00:00Z', 'completed_at': '2024-01-02T00:00:01Z'}}}
    (root / 'provenance.json').write_text(json.dumps(provenance))
    instruments = json.loads((template / 'instruments.json').read_bytes())
    instruments[0]['CurrencyPair'].update(id='BTCUSDT.BINANCE', raw_symbol='BTCUSDT', quote_currency='USDT')
    (root / 'instruments.json').write_text(json.dumps(instruments))
    selection = json.loads((template / 'selection.json').read_bytes())
    selection['dataset_revision_id'] = fixture_id()
    selection['selection'].update(bar_types=['BTCUSDT.BINANCE-1-MINUTE-LAST-EXTERNAL'],
        event_start_ns=str(int(plan['start_ns']) + 60_000_000_000),
        event_end_ns=str(int(plan['start_ns']) + 240_000_000_000))
    (root / 'selection.json').write_text(json.dumps(selection))
    declaration = json.loads((template / 'declaration.json').read_bytes())
    declaration.update(registered_ref='synthetic-installed-archive', native_snapshot_ref='synthetic-installed-archive-discovery',
        event_start='2024-01-01T00:01:00Z', event_end='2024-01-01T00:04:00Z',
        provenance_reference='SYNTHETIC_INSTALLED_ARCHIVE_TEST',
        availability_provenance='Explicit synthetic archive receipt; not an attested market observation')
    declaration['universe']['name'] = 'Synthetic installed BTCUSDT archive fixture'
    declaration['universe']['membership'][0]['instrument_id'] = 'BTCUSDT.BINANCE'
    (root / 'declaration.json').write_text(json.dumps(declaration))


def source_container_ids(config: dict, invocation: str) -> list[str]:
    ids = manage.run(['docker', 'ps', '--all', '--quiet', '--no-trunc',
                      '--filter', 'label=io.quazonai.source.invocation=' + invocation,
                      '--filter', 'label=io.quazonai.source.installation=' + config['project'],
                      '--filter', 'label=io.quazonai.source.owner=' + str(config['uid'])], capture=True).splitlines()
    if len(ids) > 2 or len(ids) != len(set(ids)) or any(not re.fullmatch(r'[0-9a-f]{64}', item) for item in ids):
        raise ValueError('Source invocation container identity is ambiguous.')
    return ids


def reconcile_source_invocation(config: dict, invocation: str, *, uncertain: bool) -> bool:
    """Confirm actual owned-container removal before any input/output cleanup."""
    try:
        ids = source_container_ids(config, invocation)
        identified = False
        for container in ids:
            try:
                inspected = json.loads(manage.run(['docker', 'inspect', '--format', '{{json .}}', container], capture=True))
            except subprocess.CalledProcessError:
                if container not in source_container_ids(config, invocation):
                    # It was selected by all three ownership labels and has now
                    # actually disappeared, normally through --rm completion.
                    identified = True
                    continue
                return False
            if not isinstance(inspected, dict) or not isinstance(inspected.get('Config'), dict):
                return False
            labels = inspected['Config'].get('Labels', {})
            if (not isinstance(labels, dict) or inspected.get('Id') != container
                    or inspected.get('Name') not in ('/quazonai-source-' + invocation,
                                                     '/quazonai-source-' + invocation + '-inventory')
                    or inspected['Config'].get('Image') != config['image']
                    or labels.get('io.quazonai.source.invocation') != invocation
                    or labels.get('io.quazonai.source.installation') != config['project']
                    or labels.get('io.quazonai.source.owner') != str(config['uid'])):
                return False
            identified = True
            # Stop only this positively identified invocation. A CLI timeout or
            # exit is never taken as evidence that its native container stopped.
            stopped = subprocess.run(['docker', 'stop', '--time', '10', container],
                                     capture_output=True, text=True, timeout=20, check=False)
            remaining = source_container_ids(config, invocation)
            if container in remaining:
                if stopped.returncode != 0:
                    return False
                removed = subprocess.run(['docker', 'rm', container], capture_output=True,
                                         text=True, timeout=20, check=False)
                if removed.returncode != 0 and container in source_container_ids(config, invocation):
                    return False
        return not source_container_ids(config, invocation) and (identified or not uncertain)
    except (OSError, ValueError, KeyError, TypeError, subprocess.SubprocessError):
        return False


def invoke_installed_source(config: dict, root: Path, arguments: list[str], inputs=(), output=None):
    def command_for(invocation):
        command = [sys.executable, '-B', str(Path(config['bundle']) / 'manage.py'), 'source',
                   '--directory', config['root'], '--invocation-id', invocation]
        for path in inputs:
            command += ['--read-only', str(path)]
        if output:
            command += ['--output-parent', str(output)]
        return command + ['--', *arguments]
    return invoke_source_process(config, root, arguments[0], command_for)


def invoke_source_process(config: dict, root: Path, operation: str, command_for):
    """Track actual container ownership for registry and direct packaged smoke commands."""
    invocation = secrets.token_hex(16)
    diagnostics = root / 'diagnostics'
    diagnostics.mkdir(exist_ok=True)
    record = diagnostics / (invocation + '.json')
    metadata = {'invocation': invocation, 'operation': operation, 'state': 'starting',
                'installation': config['project'], 'owner_uid': config['uid'], 'image': config['image'],
                'container_names': ['quazonai-source-' + invocation, 'quazonai-source-' + invocation + '-inventory']}
    record.write_text(json.dumps(metadata, indent=2) + '\n')
    timed_out, local_finished, stdout, stderr, returncode = False, False, '', '', None
    try:
        with subprocess.Popen(command_for(invocation), stdout=subprocess.PIPE,
                              stderr=subprocess.PIPE, text=True, start_new_session=True) as process:
            try:
                stdout, stderr = process.communicate(timeout=180)
            except subprocess.TimeoutExpired:
                timed_out = True
                # This is only local process cleanup. The separate Docker
                # reconciliation below owns the actual container outcome.
                with contextlib.suppress(ProcessLookupError):
                    os.killpg(process.pid, signal.SIGTERM)
                try:
                    stdout, stderr = process.communicate(timeout=10)
                except subprocess.TimeoutExpired:
                    with contextlib.suppress(ProcessLookupError):
                        os.killpg(process.pid, signal.SIGKILL)
                    stdout, stderr = process.communicate(timeout=10)
            returncode = process.returncode
            local_finished = True
    finally:
        confirmed = reconcile_source_invocation(config, invocation, uncertain=not local_finished or timed_out or returncode != 0)
        (diagnostics / (invocation + '.stdout')).write_text(stdout)
        (diagnostics / (invocation + '.stderr')).write_text(stderr)
        metadata.update(state='terminal' if confirmed else 'container_outcome_uncertain',
                        cli_returncode=returncode, local_timeout=timed_out, container_cleanup_confirmed=confirmed)
        record.write_text(json.dumps(metadata, indent=2) + '\n')
    if not confirmed:
        raise RuntimeError(f'Source container outcome is uncertain; invocation {invocation}; artifacts retained at {root}')
    if timed_out or returncode != 0:
        raise RuntimeError(f'Installed source operation failed; invocation {invocation}; artifacts retained at {root}')
    return json.loads(stdout)



def invoke_packaged_history(config: dict, root: Path, inputs: Path, output: Path):
    """Exercise the shipped history launcher, then read its catalog through native preparation."""
    release_identity = manage.manifest(Path(config['bundle']))
    assert all(release_identity[key] == config[key] for key in ('image', 'version', 'revision'))
    source = manage.source_path(inputs, config)
    target = manage.source_path(output, config, output=True)
    assert source != target and source not in target.parents and target not in source.parents
    program = r'''
import json, os, shutil, stat, subprocess, sys
from pathlib import Path
inputs, output = map(Path, sys.argv[1:])
binroot = Path('/opt/quazonai/operator/bin')
assert not shutil.which('cargo') and not shutil.which('rustc')
assert not Path('/build/Cargo.toml').exists()
elf = binroot / 'source-tools'
assert stat.S_ISREG(elf.lstat().st_mode) and os.access(elf, os.X_OK)
with elf.open('rb') as stream:
    assert stream.read(4) == b'\x7fELF'
for name, modes in [('catalog-prepare', ['', 'ingest-candles', 'ingest-archive-candles']),
                    ('polymarket-history', ['', 'fetch', 'import', 'archive', 'chain', 'capture'])]:
    launcher = binroot / name
    assert stat.S_ISREG(launcher.lstat().st_mode) and os.access(launcher, os.X_OK)
    assert launcher.read_text() == '#!/bin/sh\nexec /opt/quazonai/operator/bin/source-tools ' + name + ' "$@"\n'
    for mode in modes:
        args = ([mode] if mode else []) + ['--help']
        direct = subprocess.run([str(elf), name, *args], capture_output=True, timeout=30)
        wrapped = subprocess.run([str(launcher), *args], capture_output=True, timeout=30)
        assert direct.returncode == wrapped.returncode == 0
        assert direct.stdout == wrapped.stdout and direct.stderr == wrapped.stderr
native = output / 'history import [行情]'
prepared = output / 'history readback [行情]'
command = [str(binroot / 'polymarket-history'), 'import', '--input', str(inputs / 'history.json'), '--output', str(native)]
result = subprocess.run(command, capture_output=True, timeout=90)
assert result.returncode == 0, result.stderr.decode(errors='replace')
report = json.loads(result.stdout)
assert report == json.loads((native / 'import-report.json').read_text())
assert report['bars'] == 3 and report['instruments'] == report['instrument_versions'] == 1
assert all(report[key] == 0 for key in ('trades', 'quotes', 'deltas', 'closes'))
assert report['coverage'] == 'UNPROVEN' and report['historical_availability'] == 'UNVERIFIED'
assert report['registered_in_quazonai'] is False
original = json.loads((inputs / 'history.json').read_text())
evidence = json.loads((native / 'source-evidence.json').read_text())
assert all(evidence[key] == original[key] for key in original)
reused = subprocess.run(command, capture_output=True, timeout=30)
assert reused.returncode == 1 and reused.stderr == b'QZ_POLYMARKET_HISTORY_FAILED\n' and not reused.stdout
readback = subprocess.run([str(binroot / 'catalog-prepare'), '--catalog', str(native / 'catalog'),
    '--declaration', str(inputs / 'history-declaration.json'), '--selection', str(inputs / 'history-selection.json'),
    '--output', str(prepared)], capture_output=True, timeout=90)
assert readback.returncode == 0, readback.stderr.decode(errors='replace')
metadata = json.loads(readback.stdout)
assert metadata == json.loads((prepared / 'catalog-metadata.json').read_text())
assert metadata['origin'] == 'FIXTURE' and metadata['pit_status'] == 'UNVERIFIED' and metadata['row_count'] == '3'
assert metadata['universe']['instrument_definitions'] == original['instruments']
print(json.dumps({'history_import_bars': report['bars'], 'native_readback_rows': metadata['row_count'],
                  'origin': metadata['origin'], 'pit_status': metadata['pit_status']}))
'''
    def command_for(invocation):
        command = manage.source_container(config, release_identity, 'none', invocation)
        command += ['--mount', f'type=bind,source={source},target={source},readonly',
                    '--mount', f'type=bind,source={target},target={target}']
        return command + [release_identity['image'], '-E', '-s', '-B', '-c', program, str(source), str(target)]
    return invoke_source_process(config, root, 'packaged_history_import_readback', command_for)


def verify_source_output_reuse(config: dict, root: Path, arguments: list[str], inputs, output: Path) -> None:
    """Execute the installed preflight and prove rejection before any process launch."""
    invocation = secrets.token_hex(16)
    diagnostics = root / 'diagnostics'
    diagnostics.mkdir(exist_ok=True)
    facts = {'invocation': invocation, 'operation': 'reuse_preflight', 'state': 'starting',
             'container_cleanup_confirmed': False, 'docker_calls': 0,
             'cli_returncode': None, 'local_timeout': False}
    record = diagnostics / (invocation + '.json')
    record.write_text(json.dumps(facts, indent=2) + '\n')
    program = '''
import importlib.util, json, sys
from pathlib import Path
spec = importlib.util.spec_from_file_location('installed_source_manager', sys.argv[1])
manager = importlib.util.module_from_spec(spec)
spec.loader.exec_module(manager)
calls = []
def forbidden(*args, **kwargs):
    calls.append(True)
    raise AssertionError('Output reuse reached process launch')
manager.run = forbidden
manager.subprocess.run = forbidden
manager.subprocess.Popen = forbidden
manager.os.execv = forbidden
manager.os.execvp = forbidden
try:
    manager.source_command(Path(sys.argv[2]), [Path(value) for value in json.loads(sys.argv[3])],
                           Path(sys.argv[4]), json.loads(sys.argv[5]), sys.argv[6])
except ValueError as error:
    assert str(error) == 'Source output must be new; original and failed artifacts are retained.'
else:
    raise AssertionError('Installed preflight accepted an existing output')
assert not calls
print(json.dumps({'rejected_before_launch': True, 'docker_calls': 0}))
'''
    try:
        result = subprocess.run([sys.executable, '-B', '-c', program,
                                 str(Path(config['bundle']) / 'manage.py'), config['root'],
                                 json.dumps([str(path) for path in inputs]), str(output),
                                 json.dumps(arguments), invocation], capture_output=True, text=True, timeout=20, check=False)
        facts.update(cli_returncode=result.returncode, state='preflight_rejection_unconfirmed')
        if result.returncode == 0 and json.loads(result.stdout) == {'rejected_before_launch': True, 'docker_calls': 0}:
            facts.update(state='rejected_before_launch', container_cleanup_confirmed=True)
    except subprocess.TimeoutExpired:
        facts.update(state='preflight_timeout', local_timeout=True)
    except (OSError, ValueError):
        facts.update(state='preflight_result_invalid')
    finally:
        record.write_text(json.dumps(facts, indent=2) + '\n')
    assert facts['container_cleanup_confirmed'], 'Installed output-reuse preflight proof failed'


def verify_installed_sources(config: dict) -> None:
    """No checkout modules or compiler: actual installed manager and image payload."""
    installation = Path(config['root'])
    binary_root = installation / 'releases' / config['version'] / 'bin'
    assert not (binary_root / 'catalog-prepare').exists()
    assert not (binary_root / 'polymarket-history').exists()
    assert not (binary_root / 'source-tools').exists()
    before = processor_identity(config)
    # Outside the enclosing installation TemporaryDirectory: its cleanup must
    # never remove mounts still owned by an uncertain source invocation.
    root = Path(tempfile.mkdtemp(prefix='quazonai-source tools [行情] ', dir=os.environ.get('RUNNER_TEMP')))
    complete = False
    def invoke_source(arguments, inputs=(), output=None):
        return invoke_installed_source(config, root, arguments, inputs, output)
    try:
        inventory = invoke_source(['plugins'])
        assert 'prepare' in next(item for item in inventory if item['id'] == 'coinbase-candles')['capabilities']
        inputs, converted_parent, prepared_parent = (root / name for name in ('inputs', 'converted', 'prepared'))
        for path in (inputs, converted_parent, prepared_parent):
            path.mkdir()
        plan = invoke_source(['plan', 'coinbase-candles', '--instrument', 'BTC-USD', '--start-seconds', '0',
                              '--end-seconds', '180', '--interval-seconds', '60'])
        source_fixture(inputs, plan)
        frozen = {str(path): hashlib.sha256(path.read_bytes()).hexdigest() for path in inputs.rglob('*') if path.is_file()}
        acquisition = str(inputs / 'acquired/acquisition.json')
        verified = invoke_source(['verify', 'coinbase-candles', '--acquisition', acquisition], [inputs])
        assert verified['record_count'] == 3
        converted = converted_parent / 'native'
        report = invoke_source(['convert', 'coinbase-candles', '--acquisition', acquisition,
                                '--instruments', str(inputs / 'instruments.json'), '--output', str(converted)],
                               [inputs], converted_parent)
        assert report['native_report']['bars'] == 3
        assert report['native_report']['native_readback_verified'] is True
        assert report['admission']['research_qualified'] is False
        output = prepared_parent / 'discovery'
        arguments = ['prepare', 'coinbase-candles', '--native-output', str(converted),
                     '--declaration', str(inputs / 'declaration.json'), '--selection', str(inputs / 'selection.json'),
                     '--output', str(output)]
        handoff = invoke_source(arguments, [inputs, converted], prepared_parent)
        assert handoff['status'] == 'CATALOG_PREPARED'
        assert handoff['catalog_registration'] == {'root': str(output / 'catalog'), 'metadata_file': str(output / 'catalog-metadata.json')}
        original = (output / 'catalog-metadata.json').read_bytes()
        assert handoff['metadata_bytes'] == len(original)
        assert handoff['metadata_sha256'] == hashlib.sha256(original).hexdigest()
        metadata = json.loads(original)
        assert metadata['row_count'] == '3' and metadata['origin'] == 'FIXTURE' and metadata['pit_status'] == 'UNVERIFIED'
        assert metadata['available_through'] == '2024-01-02T00:00:01Z'
        assert handoff['identity_hints'] == {'native_catalog_ref': metadata['registered_ref'], 'native_storage_version': metadata['storage_version']}
        assert handoff['producer'] == {'version': config['version'], 'revision': config['revision'], 'image': config['image']}
        verify_source_output_reuse(config, root, arguments, [inputs, converted], prepared_parent)
        assert (output / 'catalog-metadata.json').read_bytes() == original
        assert all(hashlib.sha256(Path(path).read_bytes()).hexdigest() == digest for path, digest in frozen.items())
        archive_plugin = 'binance-vision-spot-klines'
        archive_inventory = next(item for item in inventory if item['id'] == archive_plugin)
        assert set(archive_inventory['capabilities']) == {'plan', 'inspect', 'freeze', 'verify', 'convert', 'prepare'}
        assert archive_inventory['public_network_operations'] == []
        archive_inputs, frozen_parent = root / 'archive inputs', root / 'archive frozen'
        archive_inputs.mkdir()
        frozen_parent.mkdir()
        selection_args = ['--symbol', 'BTCUSDT', '--base-asset', 'BTC', '--quote-asset', 'USDT',
                          '--day', '2024-01-01', '--interval', '1m']
        archive_plan = invoke_source(['plan', archive_plugin, *selection_args])
        archive_source_fixture(archive_inputs, inputs, archive_plan)
        original_hashes = {str(path): hashlib.sha256(path.read_bytes()).hexdigest()
                           for path in archive_inputs.iterdir() if path.is_file()}
        bundle = frozen_parent / 'bundle'
        manifest = invoke_source(['freeze', archive_plugin, *selection_args,
            '--archive', str(archive_inputs / archive_plan['archive_name']),
            '--checksum', str(archive_inputs / (archive_plan['archive_name'] + '.CHECKSUM')),
            '--provenance', str(archive_inputs / 'provenance.json'), '--output', str(bundle)],
            [archive_inputs], frozen_parent)
        assert manifest['provenance_kind'] == 'SYNTHETIC' and manifest['counts']['rows'] == '3'
        assert (bundle / 'raw/archive.zip').read_bytes() == (archive_inputs / archive_plan['archive_name']).read_bytes()
        original_bundle_hashes = {str(path): hashlib.sha256(path.read_bytes()).hexdigest()
                                  for path in bundle.rglob('*') if path.is_file()}
        verified_archive = invoke_source(['verify', archive_plugin, '--acquisition', str(bundle / 'archive.json')], [bundle])
        assert verified_archive['integrity'] == 'VERIFIED' and verified_archive['provenance_status'] == 'SYNTHETIC'
        archive_native = converted_parent / 'archive-native'
        imported = invoke_source(['convert', archive_plugin, '--acquisition', str(bundle / 'archive.json'),
            '--instruments', str(archive_inputs / 'instruments.json'), '--output', str(archive_native)],
            [bundle, archive_inputs], converted_parent)
        assert imported['native_report']['bars'] == 3 and imported['native_report']['native_readback_verified'] is True
        assert imported['native_report']['source_provenance_kind'] == 'SYNTHETIC'
        assert imported['native_report']['receipt_basis'] == {
            'kind': 'SYNTHETIC', 'source_clock': 'provenance.retrieval.archive.completed_at',
            'declared_observed_at': '2024-01-02T00:00:01Z', 'ts_init_ns': '1704153601000000000'}
        assert imported['admission']['research_qualified'] is False
        archive_prepared = prepared_parent / 'archive-discovery'
        archive_arguments = ['prepare', archive_plugin, '--native-output', str(archive_native),
            '--declaration', str(archive_inputs / 'declaration.json'), '--selection', str(archive_inputs / 'selection.json'),
            '--output', str(archive_prepared)]
        archive_handoff = invoke_source(archive_arguments, [archive_inputs, archive_native], prepared_parent)
        assert archive_handoff['status'] == 'CATALOG_PREPARED'
        assert archive_handoff['catalog_registration'] == {
            'root': str(archive_prepared / 'catalog'), 'metadata_file': str(archive_prepared / 'catalog-metadata.json')}
        archive_metadata = (archive_prepared / 'catalog-metadata.json').read_bytes()
        assert archive_handoff['metadata_bytes'] == len(archive_metadata)
        assert archive_handoff['metadata_sha256'] == hashlib.sha256(archive_metadata).hexdigest()
        archived = json.loads(archive_metadata)
        assert archived['row_count'] == '3' and archived['origin'] == 'FIXTURE' and archived['pit_status'] == 'UNVERIFIED'
        assert archived['available_through'] == '2024-01-02T00:00:01Z'
        assert archive_handoff['producer'] == {'version': config['version'], 'revision': config['revision'], 'image': config['image']}
        verify_source_output_reuse(config, root, archive_arguments, [archive_inputs, archive_native], prepared_parent)
        assert (archive_prepared / 'catalog-metadata.json').read_bytes() == archive_metadata
        assert all(hashlib.sha256(Path(path).read_bytes()).hexdigest() == digest
                   for path, digest in {**original_hashes, **original_bundle_hashes}.items())
        history = invoke_packaged_history(config, root, inputs, converted_parent)
        assert history == {'history_import_bars': 3, 'native_readback_rows': '3',
                           'origin': 'FIXTURE', 'pit_status': 'UNVERIFIED'}
        assert all(hashlib.sha256(Path(path).read_bytes()).hexdigest() == digest for path, digest in frozen.items())
        assert processor_identity(config) == before
        complete = True
    finally:
        if complete:
            shutil.rmtree(root)
        else:
            print(f'Source smoke did not complete; diagnostics and partial artifacts retained at {root}', file=sys.stderr)


def runtime_native_exit_facts(directory: Path, spec: dict) -> dict:
    """Inspect only this smoke's owned SQLite exit record, including live WAL."""
    unavailable = {"native_exit": "unavailable"}
    try:
        run, attempt, external = spec["run_id"], spec["attempt_no"], spec["external_job_id"]
        if (type(run) is not str or str(uuid.UUID(run)) != run or type(attempt) is not int
                or not 1 <= attempt <= 4294967295 or external != f"{run}/{attempt}"):
            return unavailable
        # Pin every directory component and file without traversing symlinks.
        # Snapshot bounded regular files instead of allowing SQLite to follow
        # live WAL/SHM paths or write a shared-memory index in the source root.
        with contextlib.ExitStack() as handles:
            def opened(name, flags, parent=None):
                fd = os.open(name, flags | os.O_NOFOLLOW | os.O_NONBLOCK, dir_fd=parent)
                handles.callback(os.close, fd)
                return fd
            def identity(info):
                return (info.st_dev, info.st_ino, info.st_size, info.st_mtime_ns, info.st_ctime_ns)
            if not directory.is_absolute() or ".." in directory.parts:
                return unavailable
            parent = opened("/", os.O_RDONLY | os.O_DIRECTORY)
            for part in (*directory.parts[1:], "state"):
                parent = opened(part, os.O_RDONLY | os.O_DIRECTORY, parent)
            snapshots, observed = {}, {}
            for name in ("journal.sqlite", "journal.sqlite-wal"):
                try:
                    fd = opened(name, os.O_RDONLY, parent)
                except FileNotFoundError:
                    if name == "journal.sqlite":
                        raise
                    observed[name] = None
                    continue
                info = os.fstat(fd)
                if not stat.S_ISREG(info.st_mode) or info.st_nlink != 1 or info.st_size > 8 * 1024 * 1024:
                    return unavailable
                with os.fdopen(fd, "rb", closefd=False) as source:
                    snapshots[name] = source.read(8 * 1024 * 1024 + 1)
                if len(snapshots[name]) != info.st_size or identity(os.fstat(fd)) != identity(info):
                    return unavailable
                observed[name] = identity(info)
            # A single coherent observation only: never retry a changing source.
            for name, before in observed.items():
                try:
                    after = identity(os.stat(name, dir_fd=parent, follow_symlinks=False))
                except FileNotFoundError:
                    after = None
                if after != before:
                    return unavailable
        with tempfile.TemporaryDirectory(prefix="quazonai-exit-snapshot-") as temporary:
            root = Path(temporary)
            for name, content in snapshots.items():
                (root / name).write_bytes(content)
            with contextlib.closing(sqlite3.connect((root / "journal.sqlite").as_uri() + "?mode=ro", uri=True,
                                                   timeout=0.1)) as journal:
                journal.execute("PRAGMA query_only=ON")
                journal.execute("PRAGMA trusted_schema=OFF")
                deadline = time.monotonic() + 1
                journal.set_progress_handler(lambda: int(time.monotonic() >= deadline), 1000)
                for table in ("runtime_jobs", "native_exit_observations"):
                    schema = journal.execute(f"PRAGMA table_list('{table}')").fetchone()
                    if schema is None or schema[2] != "table" or schema[5] != 1:
                        return unavailable
                row = journal.execute(
                    "SELECT e.exit_code, e.oom_killed FROM native_exit_observations AS e "
                    "JOIN runtime_jobs AS j ON e.external_id=j.external_id AND e.container_id=j.container_id "
                    "AND e.started_us=j.started_us WHERE j.external_id=? AND j.run_id=? AND j.attempt_no=? "
                    "AND j.phase='TERMINAL' AND typeof(j.attempt_no)='integer' "
                    "AND typeof(e.exit_code)='integer' AND typeof(e.oom_killed)='integer' LIMIT 2",
                    (external, run, attempt)).fetchall()
                if len(row) != 1 or type(row[0][0]) is not int or type(row[0][1]) is not int or row[0][1] not in (0, 1):
                    return unavailable
                return {"native_exit": "read", "exit_code": row[0][0], "oom_killed": bool(row[0][1])}
    except Exception:
        return unavailable


def runtime_failure_facts(request, route: str, status: dict) -> dict:
    """Read the owned result before cleanup, exposing only closed contract codes."""
    if status.get("has_result") is not True:
        return {"result": "not_available"}
    try:
        result = request("GET", route + "/result")
        if not isinstance(result, dict) or any(
            result.get(key) != status[key]
            for key in ("run_id", "attempt_no", "external_job_id", "state")
        ):
            return {"result": "identity_mismatch"}
        error = result.get("error")
        if error is None:
            return {"result": "read", "error": None}
        if not isinstance(error, dict):
            return {"result": "invalid_error"}
        classes = ("RETRYABLE_INFRA", "PERMANENT_CONFIG", "INVALID_INPUT", "RESOURCE_LIMIT")
        codes = ("ENGINE_UNAVAILABLE", "IMAGE_UNAVAILABLE", "CONTRACT_UNSUPPORTED",
                 "INPUT_UNAVAILABLE", "INVALID_INPUT", "NATIVE_JOB_FAILED", "INVALID_OUTPUT",
                 "CPU_LIMIT", "MEMORY_LIMIT", "OUTPUT_LIMIT", "DEADLINE_EXCEEDED")
        return {"result": "read",
                "error_class": error.get("class") if error.get("class") in classes else "UNRECOGNIZED",
                "error_code": error.get("code") if error.get("code") in codes else "UNRECOGNIZED"}
    except Exception:
        # Do not replace the original failure or expose response bodies, URLs,
        # credentials, native stderr, or arbitrary exception text in CI logs.
        return {"result": "unavailable"}


def verify_runtime(config: dict) -> None:
    # Use only the image-extracted gateway and the selected job image. Compilation
    # here is the actual scientific COMPILE_MODEL operation inside its job container.
    with tempfile.TemporaryDirectory(prefix="quazonai-runtime-", dir=Path(config["root"]).parent) as temporary:
        directory = Path(temporary)
        credential = directory / "credential"
        secret = os.urandom(32).hex()
        credential.write_text(secret)
        credential.chmod(0o600)
        port, _ = ports()
        native_config = {
            "schema_version": 1, "state_dir": str(directory / "state"), "credential_file": str(credential),
            "docker_socket": config["docker_socket"], "bind": f"127.0.0.1:{port}",
            "images": [{"job_kind": "DATA_VALIDATE", "image_ref": config["runtime_image"]}],
            "catalogs": [], "max_cpu": 1, "max_memory_mib": 1024, "max_wall_seconds": 120,
            "max_output_bytes": None, "max_parallel_jobs": 2, "max_pending_jobs": 8,
            "storage_quota_bytes": 268435456,
        }
        path = directory / "runtime.json"
        path.write_text(json.dumps(native_config))
        script = Path(config["bundle"]) / "runtime.sh"
        args = ["--directory", config["root"], "--config", str(path)]
        manage.run(["bash", str(script), "doctor", *args])
        opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
        origin = f"http://127.0.0.1:{port}/runtime/v1"
        def request(method, route, body=None, *, raw=False):
            headers = {"Authorization": "Bearer " + secret}
            if body is not None:
                headers["Content-Type"] = "application/octet-stream" if raw else "application/json"
                if raw:
                    headers["X-QZ-Storage-Version"] = "1"
                else:
                    body = json.dumps(body).encode()
            value = urllib.request.Request(origin + route, data=body, headers=headers, method=method)
            with opener.open(value, timeout=15) as response:
                content = response.read(5 * 1024 * 1024)
                assert secret.encode() not in content
                return content if raw and method == "GET" else json.loads(content)
        child = None
        run_id = fixture_id()
        external = run_id + "/1"
        route = "/jobs/" + urllib.parse.quote(external, safe="")
        log = directory / "gateway.log"
        def start(*, journal_only=False):
            with log.open("ab") as output:
                process = subprocess.Popen(["bash", str(script), "serve", *args],
                                           stdin=subprocess.DEVNULL, stdout=output, stderr=output)
            deadline = time.monotonic() + 30
            while time.monotonic() < deadline:
                assert process.poll() is None, "Packaged Runtime exited before listening"
                try:
                    request("GET", route if journal_only else "/capabilities")
                    return process
                except (urllib.error.URLError, TimeoutError):
                    time.sleep(0.2)
            process.terminate()
            process.wait(timeout=15)
            raise AssertionError("Packaged Runtime did not become available")
        try:
            child = start()
            source_id, parameters_id = fixture_id(), fixture_id()
            source = b"""#![no_std]
#[panic_handler] fn panic(_: &core::panic::PanicInfo) -> ! { loop {} }
#[no_mangle] pub extern "C" fn predict(close:f64,_previous:f64,fast:f64,slow:f64,_volume:f64,_open:f64,_high:f64,_low:f64)->f64 { (fast-slow)/close }
"""
            operation = json.dumps({"operation": "COMPILE_MODEL", "schema_version": 1, "code_artifact_id": source_id}).encode()
            for identity, content in ((source_id, source), (parameters_id, operation)):
                receipt = request("PUT", "/objects/" + identity, content, raw=True)
                assert receipt["artifact_id"] == identity and int(receipt["byte_count"]) == len(content)
            spec = {
                "schema_version": 1, "run_id": run_id, "attempt_no": 1, "owner_epoch": "1",
                "external_job_id": external, "job_kind": "DATA_VALIDATE", "image_ref": config["runtime_image"],
                "input_set_id": fixture_id(), "parameters_artifact_id": parameters_id,
                "inputs": [{"kind": "ARTIFACT", "artifact_id": source_id, "storage_version": "1",
                            "byte_count": str(len(source)), "role": "CODE"}],
                "limits": {"cpu": 1, "cpu_seconds": "60", "memory_mib": 512, "wall_seconds": 60, "output_bytes": "4194304"},
                "deadline_at": (datetime.datetime.now(datetime.timezone.utc) + datetime.timedelta(seconds=80)).isoformat(),
                "requested_output_schemas": [{"name": name, "version": "1"} for name in ("qz.wasm_model", "qz.model_compilation")],
            }
            submitted = request("POST", "/jobs", spec)
            assert submitted["external_job_id"] == external
            deadline = time.monotonic() + 100
            while time.monotonic() < deadline:
                status = request("GET", route)
                if status["state"] in ("SUCCEEDED", "FAILED", "CANCELLED"):
                    break
                time.sleep(0.2)
            else:
                raise AssertionError("Native compile did not reach a terminal state")
            if status["state"] != "SUCCEEDED":
                facts = {**runtime_failure_facts(request, route, status), **runtime_native_exit_facts(directory, spec)}
                print("Native compile failure: " + json.dumps(facts, sort_keys=True), file=sys.stderr, flush=True)
            assert status["state"] == "SUCCEEDED", status
            result = request("GET", route + "/result")
            assert result["state"] == "SUCCEEDED" and result["run_id"] == run_id
            assert result["engine_versions"]["rustc"] == "1.98.1"
            model = next(item for item in result["artifacts"] if item["kind"] == "MODEL")
            wasm = request("GET", route + "/artifacts/" + model["storage_ref"], raw=True)
            assert wasm.startswith(b"\0asm\x01\0\0\0") and len(wasm) == int(model["byte_count"])
            child.terminate()
            child.wait(timeout=20)
            native_config["docker_socket"] = str(directory / "unavailable-docker.sock")
            path.write_text(json.dumps(native_config))
            child = start(journal_only=True)
            assert request("POST", "/jobs", spec) == status
            assert request("GET", route + "/result") == result
            assert request("GET", route + "/artifacts/" + model["storage_ref"], raw=True) == wasm
        finally:
            if child and child.poll() is None:
                child.terminate()
                child.wait(timeout=20)
            containers = codex.docker(config, "ps", "--all", "--quiet", "--filter", "label=io.quazonai.run=" + run_id, capture=True)
            if containers:
                codex.docker(config, "rm", "--force", *containers.splitlines(), capture=True)


def verify_published_bundle(root: Path, bundle: Path, assets: Path | None = None) -> None:
    # First consumer on the fresh runner: the exact shipped archive, including
    # its original version and default Codex digest. No synthetic manifest here.
    selected = manage.manifest(bundle)
    installation = root / "published-installation"
    web, database = ports()
    command = ["bash", str(bundle / "deploy.sh"), "--directory", str(installation),
               "--port", str(web), "--database-port", str(database)]
    env = None
    if assets:
        # The Release stays a draft until acceptance. Serve the exact downloaded
        # Actions artifacts through a curl fixture; execute the shipped installer
        # and all real deployment/CLI processes under the no-build wrappers.
        transport = root / "release-downloads"
        transport.mkdir()
        curl = transport / "curl"
        base = f"https://github.com/{manage.REPOSITORY}/releases/download/{selected['version']}/"
        curl.write_text("#!/usr/bin/python3\nimport shutil, sys\nfrom pathlib import Path\n"
                        f"base = {base!r}\nassets = Path({str(assets)!r})\n"
                        "args = sys.argv[1:]\nurl = args[-1]\n"
                        "assert url.startswith(base)\nname = url[len(base):]\n"
                        "assert name in {'SHA256SUMS', 'quazonai-cli-linux-x86_64.tar.gz', 'quazonai-deploy.tar.gz'}\n"
                        "shutil.copyfile(assets / name, args[args.index('--output') + 1])\n")
        curl.chmod(0o755)
        env = {**os.environ, "PATH": str(transport) + os.pathsep + os.environ["PATH"]}
        command = ["bash", str(assets / "install.sh"), "--directory", str(installation),
                   "--bin-dir", str(root / "client-bin"), "--port", str(web), "--database-port", str(database)]
    try:
        manage.run(command, env=env)
        config = manage.configuration(installation)
        assert all(config[name] == value for name, value in selected.items())
        assert codex.read_env(installation / ".env")[0] == selected["codex_version"]
        original_key = fingerprint(installation)
        verify_container_codex(config)
        verify_runtime(config)
        verify_installed_sources(config)
        if assets:
            binary = str(root / "client-bin/quazonai")
            assert selected["version"] in manage.run([binary, "--version"], capture=True)
            manage.run([binary, "client", "--help"], capture=True)
            assert json.loads(manage.run([binary, "openapi"], capture=True))["openapi"]
        manage.run(command, env=env)
        assert fingerprint(installation) == original_key
        assert manage.configuration(installation) == config
    finally:
        cleanup_installation(installation)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--manifest", type=Path)
    parser.add_argument("--image")
    parser.add_argument("--runtime-image")
    parser.add_argument("--codex-image")
    parser.add_argument("--codex-version")
    parser.add_argument("--previous-codex-image")
    parser.add_argument("--previous-codex-version")
    parser.add_argument("--revision")
    parser.add_argument("--assert-cold", action="store_true")
    parser.add_argument("--installer-assets", type=Path)
    parser.add_argument("--report", type=Path)
    args = parser.parse_args()
    os.umask(0o077)
    if args.manifest:
        images = manage.validate_manifest(json.loads(args.manifest.read_text()), published=True)
        revision = images["revision"]
    else:
        if not all((args.image, args.runtime_image, args.codex_image, args.codex_version, args.revision)):
            parser.error("Provide a published manifest or all prebuilt test images and their versions")
        images = {field: manage.run(["docker", "image", "inspect", "--format", "{{.Id}}", getattr(args, field)], capture=True)
                  for field in ("image", "runtime_image", "codex_image")}
        images["codex_version"] = args.codex_version
        revision = args.revision
    previous = (images["codex_version"], images["codex_image"])
    if args.previous_codex_image:
        if not args.previous_codex_version:
            parser.error("--previous-codex-image requires --previous-codex-version")
        previous = (args.previous_codex_version, manage.run(
            ["docker", "image", "inspect", "--format", "{{.Id}}", args.previous_codex_image], capture=True))
    if args.assert_cold:
        manage.run(["docker", "info"], capture=True)
        if not args.manifest:
            parser.error("--assert-cold requires a published manifest")
        for field in ("image", "runtime_image", "codex_image", "database_image"):
            result = subprocess.run(["docker", "image", "inspect", images[field]],
                                    stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=False)
            assert result.returncode != 0, "Clean installation unexpectedly found a cached image"
    with tempfile.TemporaryDirectory(prefix="quazonai-container-", dir=os.environ.get("RUNNER_TEMP")) as temporary:
        root = Path(temporary)
        with installation_tools(root / "tools") as log:
            if args.manifest:
                verify_published_bundle(root, args.manifest.resolve().parent,
                                        args.installer_assets.resolve() if args.installer_assets else None)
            exercise(root, images, revision, previous)
            commands = [json.loads(line) for line in log.read_text().splitlines()]
            assert not any(command["blocked"] for command in commands)
        if args.report:
            args.report.parent.mkdir(parents=True, exist_ok=True)
            args.report.write_text(json.dumps({
                "revision": revision, "images": images, "cold_registry_install": args.assert_cold,
                "published_bundle_install": "passed" if args.manifest else "not_run",
                "one_line_installer_and_cli": "passed" if args.installer_assets else "not_run",
                "install_update_restore": "passed", "native_runtime_compile_restart": "passed",
                "installed_source_conversion_preparation": "passed",
                "installed_archive_freeze_conversion_preparation": "passed",
                "source_successful_invocation_terminal_observation": "passed",
                "source_real_docker_timeout_cancellation": "not_run",
                "host_build_commands": 0,
            }, indent=2) + "\n")


if __name__ == "__main__":
    main()
