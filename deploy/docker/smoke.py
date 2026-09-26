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
import socket
import secrets
import shutil
import time
import subprocess
import sys
import tarfile
import tempfile
import urllib.error
import urllib.request
import urllib.parse
import uuid

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
            "max_output_bytes": 67108864, "max_parallel_jobs": 2, "max_pending_jobs": 8,
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


def verify_published_bundle(root: Path, bundle: Path) -> None:
    # First consumer on the fresh runner: the exact shipped archive, including
    # its original version and default Codex digest. No synthetic manifest here.
    selected = manage.manifest(bundle)
    installation = root / "published-installation"
    web, database = ports()
    command = ["bash", str(bundle / "deploy.sh"), "--directory", str(installation),
               "--port", str(web), "--database-port", str(database)]
    try:
        manage.run(command)
        config = manage.configuration(installation)
        assert all(config[name] == value for name, value in selected.items())
        assert codex.read_env(installation / ".env")[0] == selected["codex_version"]
        original_key = fingerprint(installation)
        verify_container_codex(config)
        verify_runtime(config)
        manage.run(command)
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
                verify_published_bundle(root, args.manifest.resolve().parent)
            exercise(root, images, revision, previous)
            commands = [json.loads(line) for line in log.read_text().splitlines()]
            assert not any(command["blocked"] for command in commands)
        if args.report:
            args.report.parent.mkdir(parents=True, exist_ok=True)
            args.report.write_text(json.dumps({
                "revision": revision, "images": images, "cold_registry_install": args.assert_cold,
                "published_bundle_install": "passed" if args.manifest else "not_run",
                "install_update_restore": "passed", "native_runtime_compile_restart": "passed",
                "host_build_commands": 0,
            }, indent=2) + "\n")


if __name__ == "__main__":
    main()
