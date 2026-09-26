#!/usr/bin/env python3
"""Versioned Linux installation using Docker Compose and the native user Worker."""
from __future__ import annotations

import argparse
import contextlib
import fcntl
import hashlib
import io
import json
import os
from pathlib import Path
import re
import secrets
import shutil
import socket
import stat
import subprocess
import sys
import tarfile
import tempfile
import time
import urllib.request

REPOSITORY = "zhengui666/QuaZonai"
DATABASE_IMAGE = "ghcr.io/pgmq/pg18-pgmq@sha256:bfb3537068ce453609744518ece92b178ac89dff53747d47ca6fab91c2fc66a6"
BUNDLE = Path(__file__).resolve().parent
BUNDLE_FILES = {"manage.py", "deploy.sh", "update.sh", "compose.yaml", "release.json", "README.md",
                "codex.py", "codex-update.sh", "codex-login.sh", "runtime.sh", "codex.apparmor", ".env.example"}
SEMVER = re.compile(r"v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(?:-([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?")


def version(value: str) -> str:
    match = SEMVER.fullmatch(value)
    if not match or len(value) > 128:
        raise ValueError("Expected vMAJOR.MINOR.PATCH[-prerelease], without build metadata.")
    if match[4] and any(x.isdigit() and len(x) > 1 and x[0] == "0" for x in match[4].split(".")):
        raise ValueError("Numeric prerelease identifiers cannot have leading zeroes.")
    return value


def version_precedence(value: str) -> tuple:
    match = SEMVER.fullmatch(version(value))
    assert match is not None
    core = tuple(int(match[index]) for index in (1, 2, 3))
    prerelease = match[4]
    # SemVer 2.0: numeric identifiers sort below text; a release sorts above
    # its prereleases. The existing parser already rejects build metadata.
    identifiers = tuple((0, int(part)) if part.isdigit() else (1, part)
                        for part in prerelease.split('.')) if prerelease else ()
    return (*core, 1 if prerelease is None else 0, identifiers)


def validate_manifest(value: dict, *, published: bool = False) -> dict:
    if not isinstance(value, dict) or type(value.get("schema_version")) is not int or value["schema_version"] != 2:
        raise ValueError("This installer requires a version-2 deployment bundle with prebuilt images.")
    version(value["version"])
    if not re.fullmatch(r"[0-9a-f]{40}", value["revision"]):
        raise ValueError("A release must identify its exact source revision.")
    for field, repository in (("image", "quazonai"), ("runtime_image", "quazonai-runtime"),
                              ("codex_image", "quazonai-codex")):
        reference = value.get(field)
        # The pre-publication suite uses Docker's content-addressed IDs for its
        # already-built images. Public bundles contain only GHCR digests.
        pattern = rf"ghcr\.io/zhengui666/{repository}@sha256:[0-9a-f]{{64}}"
        if not isinstance(reference, str) or not (
            re.fullmatch(pattern, reference)
            or (not published and re.fullmatch(r"sha256:[0-9a-f]{64}", reference))
        ):
            raise ValueError(f"{field} must identify the prebuilt {repository} image by digest.")
    codex_version = value.get("codex_version")
    if not isinstance(codex_version, str) or codex_version.startswith("v"):
        raise ValueError("codex_version must be an exact published version.")
    version("v" + codex_version)
    if value["database_image"] != DATABASE_IMAGE:
        raise ValueError("This installer requires the release's supported PostgreSQL/PGMQ image.")
    return value


def manifest(bundle: Path) -> dict:
    return validate_manifest(json.loads((bundle / "release.json").read_text()))


def run(args: list[str], *, capture: bool = False, env=None, output=None) -> str:
    result = subprocess.run(args, check=True, env=env,
                            stdout=subprocess.PIPE if capture else output, text=capture)
    return result.stdout.strip() if capture else ""


def sync_directory(path: Path) -> None:
    descriptor = os.open(path, os.O_RDONLY | os.O_DIRECTORY)
    try:
        os.fsync(descriptor)
    finally:
        os.close(descriptor)


def durable_directory(path: Path, *, exist_ok: bool = True) -> None:
    if not path.parent.exists():
        durable_directory(path.parent)
    path.mkdir(mode=0o700, exist_ok=exist_ok)
    sync_directory(path)
    sync_directory(path.parent)


def atomic_text(path: Path, text: str) -> None:
    temporary = None
    try:
        with tempfile.NamedTemporaryFile(mode='w', encoding='utf-8', dir=path.parent,
                                         prefix='.' + path.name + '.', delete=False) as stream:
            temporary = Path(stream.name)
            os.fchmod(stream.fileno(), 0o600)
            stream.write(text)
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(temporary, path)
        sync_directory(path.parent)
    finally:
        if temporary is not None:
            with contextlib.suppress(OSError):
                temporary.unlink(missing_ok=True)


def save(path: Path, value: dict) -> None:
    atomic_text(path, json.dumps(value, indent=2) + '\n')


def sync_tree(path: Path) -> None:
    # Only our prepared release, initialized state or recovery directory is used;
    # never walk the native Codex home or follow links outside that tree.
    def fail(error):
        raise error
    for directory, _, files in os.walk(path, topdown=False, onerror=fail, followlinks=False):
        for name in files:
            child = Path(directory) / name
            if child.is_symlink():
                continue
            if not child.is_file():
                raise ValueError('A deployment recovery file is not a regular file.')
            with child.open('rb') as stream:
                os.fsync(stream.fileno())
        sync_directory(Path(directory))
    sync_directory(path.parent)


@contextlib.contextmanager
def locked(root: Path):
    durable_directory(root)
    with (root / ".deployment.lock").open("a") as stream:
        fcntl.flock(stream, fcntl.LOCK_EX | fcntl.LOCK_NB)
        yield


def configuration(root: Path) -> dict:
    value = json.loads((root / "installation.json").read_text())
    if value["uid"] != os.getuid() or value["root"] != str(root):
        raise ValueError("Manage this installation as its original user at its original path.")
    return value


def validate_docker_mapping(options: object) -> None:
    if not isinstance(options, list) or any(not isinstance(item, str) for item in options):
        raise ValueError('Docker did not report its user namespace configuration.')
    names = {part.removeprefix('name=') for item in options for part in item.split(',')}
    if names & {'rootless', 'userns'}:
        raise ValueError('This deployment needs a rootful Docker daemon without userns-remap so bind mounts retain the host UID/GID.')


def require_new_docker_project(project: str) -> None:
    # Compose labels identify only this installation. Never attach an old volume
    # to a new password/master key after its host-side manifest was removed.
    label = 'label=com.docker.compose.project=' + project
    for resource in ('container', 'volume', 'network'):
        args = ['docker', resource, 'ls', '--quiet', '--filter', label]
        if resource == 'container':
            args.append('--all')
        if run(args, capture=True):
            raise ValueError('Existing Docker resources belong to this installation path. Restore its original manifest and keys; no new identity was created.')


def announce(message: str) -> None:
    try:
        print(message, flush=True)
    except (OSError, ValueError):
        # A closed output consumer cannot roll back an already activated release.
        # Replace the broken stream so interpreter shutdown does not flush it again.
        with contextlib.suppress(OSError):
            sys.stdout = open(os.devnull, 'w')


def preflight() -> None:
    if sys.platform != "linux" or os.getuid() == 0:
        raise ValueError("Run as the Linux user who owns Codex, not with sudo.")
    if not Path("/sys/fs/cgroup/cgroup.controllers").is_file():
        raise ValueError("Native Missions require cgroup v2.")
    for binary in ("docker", "systemctl", "systemd-analyze", "git"):
        if not shutil.which(binary):
            raise ValueError(f"Required executable is missing: {binary}")
    host = os.environ.get("DOCKER_HOST")
    endpoint = host or run(["docker", "context", "inspect", "--format", "{{.Endpoints.docker.Host}}"], capture=True)
    if not endpoint.startswith("unix:"):
        raise ValueError("Use a local Unix-socket Docker daemon; remote hosts cannot share these paths.")
    architecture = run(["docker", "info", "--format", "{{.OSType}}/{{.Architecture}}"], capture=True)
    if architecture not in {"linux/x86_64", "linux/amd64"}:
        raise ValueError("This native release supports Linux x86_64.")
    validate_docker_mapping(json.loads(run(['docker', 'info', '--format', '{{json .SecurityOptions}}'], capture=True)))
    compose_version = run(["docker", "compose", "version", "--short"], capture=True)
    match = re.match(r"v?(\d+)\.(\d+)", compose_version)
    if not match or tuple(map(int, match.groups())) < (2, 20):
        raise ValueError("Docker Compose 2.20 or newer is required.")
    run(["systemctl", "--user", "show-environment"], capture=True)
    if run(["loginctl", "show-user", str(os.getuid()), "--property=Linger", "--value"], capture=True) != "yes":
        raise ValueError('Enable persistent user services first: loginctl enable-linger "$USER"')


def container_codex_configuration(config: dict) -> dict:
    endpoint = os.environ.get('DOCKER_HOST') or run(
        ['docker', 'context', 'inspect', '--format', '{{.Endpoints.docker.Host}}'], capture=True)
    if not endpoint.startswith('unix://'):
        raise ValueError('Codex containers require a local Docker Unix socket.')
    path = Path(endpoint.removeprefix('unix://')).resolve()
    metadata = path.stat()
    if not stat.S_ISSOCK(metadata.st_mode):
        raise ValueError('The configured Docker endpoint is not a Unix socket.')
    if config.get('docker_socket', str(path)) != str(path):
        raise ValueError('Use this installation\'s original Docker socket.')
    return {**config, 'codex_runtime_image': 'quazonai-codex:' + config['project'],
            'docker_socket': str(path), 'docker_socket_gid': metadata.st_gid}


def validate_ports(web: int, database: int, *, available: bool = False) -> None:
    if any(type(port) is not int or not 1024 <= port <= 65535 for port in (web, database)) or web == database:
        raise ValueError("Select distinct web/database ports between 1024 and 65535.")
    if available:
        # Check both sockets before persisting a new installation's port choices.
        # Docker still owns the final bind: a later race remains a visible error.
        with contextlib.ExitStack() as stack:
            for label, port in (("Web", web), ("Database", database)):
                sock = stack.enter_context(socket.socket())
                try:
                    sock.bind(("127.0.0.1", port))
                except OSError as error:
                    raise ValueError(f"{label} port {port} is unavailable; select another port before installing.") from error


def compose(config: dict, *args: str, capture: bool = False, output=None) -> str:
    env = {k: v for k, v in os.environ.items() if not k.startswith("COMPOSE_")}
    env.update({
        "APP_IMAGE": config["image"], "DATABASE_IMAGE": config["database_image"],
        "DATABASE_PASSWORD": config["password"], "INSTALL_DIR": config["root"],
        "WEB_PORT": str(config["port"]), "DATABASE_PORT": str(config["database_port"]),
        "HOST_UID": str(config["uid"]), "HOST_GID": str(config["gid"]),
        "NATIVE_CODEX_HOME": config["codex_home"],
        "CODEX_IMAGE": config.get('codex_runtime_image', 'quazonai-codex:' + config['project']),
        "CODEX_DOCKER_SOCKET": config.get('docker_socket', '/var/run/docker.sock'),
        "DOCKER_SOCKET_GID": str(config.get('docker_socket_gid', config['gid'])),
        "APP_RESTART_POLICY": 'unless-stopped' if (Path(config['root']) / 'current').is_symlink()
        and not (Path(config['root']) / 'pending.json').exists() else 'no',
        "RUNTIME_TARGETS": json.dumps(config.get("runtime_targets", [])),
        "DOWNSTREAM_TARGETS": json.dumps(config.get("downstream_targets", [])),
    })
    command = ["docker", "compose", "--project-name", config["project"], "--env-file", "/dev/null",
               "-f", str(Path(config["bundle"]) / "compose.yaml")]
    override = Path(config["root"]) / "compose.override.yaml"
    if override.is_file():
        command += ["-f", str(override)]
    return run(command + list(args), capture=capture, env=env, output=output)


def configure_app_restarts(config: dict, enabled: bool) -> None:
    # Update the actual existing container without recreating it or changing its
    # data mounts. Compose's pending-state default also protects new candidates.
    containers = compose(config, 'ps', '--all', '--quiet', 'app', capture=True).splitlines()
    if enabled and len(containers) != 1:
        raise ValueError('Activation requires exactly one application container.')
    if containers:
        policy = 'unless-stopped' if enabled else 'no'
        run(['docker', 'update', '--restart=' + policy, *containers], capture=True)


def sql(config: dict, statement: str) -> str:
    return compose(config, "exec", "-T", "database", "psql", "-X", "-U", "quazonai", "-d", "quazonai",
                   "-v", "ON_ERROR_STOP=1", "-Atc", statement, capture=True)


def require_idle(config: dict) -> None:
    if sql(config, "SELECT count(*) FROM app.runs WHERE finished_at IS NULL") != "0":
        raise ValueError("Unfinished Runs exist. Finish or cancel/reconcile them before updating; no task records were changed.")


def prepare(bundle: Path, config: dict) -> dict:
    validate_configuration_paths(config)
    release = manifest(bundle)
    candidate = container_codex_configuration({**config, **release})
    # Selected Codex versions survive app updates. No deployment path builds images.
    import codex
    codex.prepare(candidate, bundle)
    for field in ("image", "runtime_image"):
        image = release[field]
        if not image.startswith("sha256:"):
            run(["docker", "pull", image])
        revision = run(["docker", "image", "inspect", "--format",
                        '{{index .Config.Labels "org.opencontainers.image.revision"}}', image], capture=True)
        if revision != release["revision"]:
            raise ValueError(f"{field} source revision does not match the deployment manifest.")
    destination = Path(config["root"]) / "releases" / release["version"]
    stored = destination / "deployment"
    if (stored / "release.json").exists() and manifest(stored) != release:
        raise ValueError("An installed release version cannot be replaced with different content.")
    durable_directory(destination)
    if stored.resolve() != bundle.resolve():
        stored.mkdir(mode=0o700, exist_ok=True)
        for name in sorted(BUNDLE_FILES - {"release.json"}) + ["release.json"]:
            shutil.copyfile(bundle / name, stored / name)
    binaries = destination / "bin"
    if not binaries.exists():
        temporary = destination / "bin.partial"
        if temporary.exists():
            shutil.rmtree(temporary)
        temporary.mkdir(mode=0o700)
        container = run(["docker", "create", release["image"]], capture=True)
        try:
            run(["docker", "cp", f"{container}:/opt/quazonai/bin/.", str(temporary)])
            verify_native_binaries(temporary)
            temporary.rename(binaries)
        finally:
            run(["docker", "rm", container], capture=True)
    else:
        verify_native_binaries(binaries)
    candidate["bundle"] = str(stored)
    verify_worker_unit(candidate)
    sync_tree(destination)
    return candidate


def quote(value: str, *, specifiers: bool = True) -> str:
    if any(x in value for x in "\n\r\x00"):
        raise ValueError("Installation paths and environment values must be single-line strings.")
    value = value.replace("\\", "\\\\").replace('"', '\\"')
    if specifiers:
        value = value.replace("%", "%%")
    return '"' + value + '"'


def unit(config: dict) -> str:
    return config["project"] + ".service"


def unit_path(path: Path, *, environment_file: bool = False) -> str:
    value = str(path)
    if not path.is_absolute() or any(ord(x) < 32 or ord(x) == 127 for x in value):
        raise ValueError("systemd paths must be absolute and contain no control characters.")
    # These directives do not strip shell quotes. EnvironmentFile additionally
    # expands globs; escape metacharacters there without changing WorkingDirectory.
    value = value.replace("%", "%%")
    if environment_file:
        value = "".join("\\" + x if x in "\\*?[]" else x for x in value)
    return value


def validate_configuration_paths(config: dict) -> None:
    for name in ("root", "home", "codex_home", "unit_directory"):
        unit_path(Path(config[name]))
    if any(x in config["root"] for x in (':', '"', '\\')):
        raise ValueError("Installation directory cannot contain colon, double quote or backslash; PATH and systemd cannot represent its executable location.")
    root = Path(config['root']).resolve()
    codex_home = Path(config['codex_home']).resolve()
    if codex_home.is_relative_to(root) or root.is_relative_to(codex_home):
        raise ValueError('CODEX_HOME must be separate from the managed installation directory, including its state and backups.')
    for name in ("home", "codex_home", "path"):
        quote(config[name], specifiers=False)


def worker_unit_text(config: dict) -> str:
    root = Path(config["root"])
    binary = root / "releases" / config["version"] / "bin/server"
    return (
        "[Unit]\nDescription=QuaZonai native Worker for the container deployment\n"
        "[Service]\nType=simple\nUMask=0077\n"
        f"WorkingDirectory={unit_path(root / 'data')}\n"
        f"EnvironmentFile={unit_path(root / 'worker.env', environment_file=True)}\n"
        f"ExecStart=:{quote(str(binary))} worker\n"
        "Restart=on-failure\nRestartSec=3\nTimeoutStopSec=90\n"
        "[Install]\nWantedBy=default.target\n"
    )


def verify_worker_unit(config: dict) -> None:
    # Parse the exact future unit before any running release is stopped. This
    # writes only a temporary file; it neither installs nor starts a user service.
    with tempfile.TemporaryDirectory(prefix="quazonai-unit-") as temporary:
        path = Path(temporary) / unit(config)
        path.write_text(worker_unit_text(config))
        run(["systemd-analyze", "--user", "verify", str(path)])


def verify_native_binaries(directory: Path) -> None:
    for binary in ("server", "runtime"):
        run([str(directory / binary), "--version"])


def configure_worker(config: dict) -> None:
    root = Path(config["root"])
    binary = root / "releases" / config["version"] / "bin/server"
    environment = {
        "DATABASE_URL": f'postgresql://quazonai:{config["password"]}@127.0.0.1:{config["database_port"]}/quazonai',
        "STATE_DIR": str(root / "data/state"), "HOME": config["home"],
        "CODEX_HOME": config["codex_home"], "PUBLIC_URL": f'http://localhost:{config["port"]}',
        "CODEX_IMAGE": config.get('codex_runtime_image', 'quazonai-codex:' + config['project']),
        "CODEX_DOCKER_SOCKET": config.get('docker_socket', '/var/run/docker.sock'),
        "CODEX_LOCK_FILE": str(root / '.deployment.lock'),
        "DEVELOPMENT_HTTP": "true", "PATH": f'{binary.parent}:{config["path"]}',
        "RUNTIME_TARGETS": json.dumps(config.get("runtime_targets", [])),
        "DOWNSTREAM_TARGETS": json.dumps(config.get("downstream_targets", [])),
    }
    text = ''.join(name + '=' + quote(value, specifiers=False) + '\n'
                   for name, value in environment.items())
    units = Path(config['unit_directory'])
    durable_directory(units)
    atomic_text(root / 'worker.env', text)
    atomic_text(units / unit(config), worker_unit_text(config))
    # Neither file is truncated in place. Both files and their directory entries
    # are durable before the native manager observes or starts this version.
    run(['systemctl', '--user', 'daemon-reload'])


def verify_worker(config: dict) -> None:
    before = run(["systemctl", "--user", "show", unit(config), "--property=MainPID", "--value"], capture=True)
    if before == "0" or not before.isdigit():
        raise ValueError("Native Worker did not start.")
    time.sleep(3)
    run(["systemctl", "--user", "is-active", "--quiet", unit(config)])
    after = run(["systemctl", "--user", "show", unit(config), "--property=MainPID", "--value"], capture=True)
    if before != after:
        raise ValueError("Native Worker restarted during startup; inspect its journal.")


def verify_console(config: dict) -> None:
    origin = f'http://localhost:{config["port"]}'
    # No proxy should intercept the local deployment health observation.
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
    with opener.open(origin + "/health/live", timeout=10) as response:
        if response.status != 204:
            raise ValueError("API liveness check failed.")
    request = urllib.request.Request(origin + "/", headers={"Accept": "text/html"})
    with opener.open(request, timeout=10) as response:
        if b"<!doctype html>" not in response.read(4096).lower():
            raise ValueError("Production frontend was not served.")


def initialize_state(config: dict) -> None:
    state = Path(config["root"]) / "data/state"
    if not state.exists():
        compose(config, "run", "--rm", "--no-deps", "app", "init-state")
    # Never silently regenerate a missing key inside an existing installation.
    if not all((state / name).is_file() for name in ("master.key", "session-key.ref")) or not all(
        (state / name).is_dir() for name in ("secrets", "artifacts")
    ):
        raise ValueError("State initialization is incomplete. Preserve it and recover the original keys; initialization was not repeated.")
    sync_tree(state)


def activate(root: Path, config: dict) -> None:
    verify_console(config)
    save(root / "installation.json", config)
    link = root / "current.next"
    link.unlink(missing_ok=True)
    link.symlink_to(root / "releases" / config["version"], target_is_directory=True)
    os.replace(link, root / "current")
    sync_directory(root)
    # Worker boot recovery must exist before the app can resume admissions on reboot.
    run(["systemctl", "--user", "enable", unit(config)])
    configure_app_restarts(config, True)
    (root / "pending.json").unlink(missing_ok=True)
    sync_directory(root)
    announce(f'Active release {config["version"]}: http://localhost:{config["port"]}')


def backup(config: dict) -> Path:
    root = Path(config["root"])
    destination = root / "backups" / (time.strftime("%Y%m%dT%H%M%S") + "-" + secrets.token_hex(3))
    durable_directory(destination, exist_ok=False)
    with (destination / "database.dump").open("xb") as stream:
        compose(config, "exec", "-T", "database", "pg_dump", "-U", "quazonai", "-d", "quazonai", "-Fc", output=stream)
    shutil.copyfile(root / "data/state/master.key", destination / "master.key")
    with tarfile.open(destination / "data.tar.gz", "w:gz") as archive:
        archive.add(root / "data", arcname="data", filter=lambda x: None if x.name == "data/state/master.key" else x)
    save(destination / "installation.json", config)
    # Closing pg_dump/tar output is not a durability barrier. Publish this path
    # only after every recovery file and its directory entries have been synced.
    sync_tree(destination)
    announce(f"Recovery point: {destination}; move a copy of master.key to separate protected storage.")
    return destination


def resume_existing_services(config: dict, *, enable_boot: bool = True) -> None:
    # Recovery of an unchanged, already-migrated installation must not recreate
    # a live app container or rewrite/restart a Worker that owns active Runs.
    try:
        compose(config, 'up', '-d', '--no-recreate', '--wait', '--wait-timeout', '120', 'app')
    finally:
        action = ['enable', '--now'] if enable_boot else ['start']
        run(['systemctl', '--user', *action, unit(config)])


def start_prepared_release(root: Path, config: dict) -> None:
    # This path is entered only after the migration and complete Worker files
    # have a durable starting marker. Failures keep that marker and the same
    # processors for retry, never repeat DDL or stop an already-admitted Run.
    resume_existing_services(config, enable_boot=False)
    verify_worker(config)
    activate(root, config)


def require_install_stopped(config: dict) -> None:
    # Legacy installs have no starting phase. Do not infer DDL safety merely
    # from the absence of current when an old installation may still be live.
    container = compose(config, 'ps', '--all', '--quiet', 'app', capture=True)
    if container:
        if '\n' in container:
            raise ValueError('Ambiguous application containers prevent initial-install DDL.')
        observation = json.loads(run(['docker', 'inspect', '--format',
                                      '{"state":{{json .State}},"restart":{{json .HostConfig.RestartPolicy.Name}}}',
                                      container], capture=True))
        state = observation['state']
        if (state.get('Status') != 'created' or state.get('Running') or state.get('Restarting')
                or observation['restart'] != 'no'):
            raise ValueError('Existing application containers have no completed-migration marker; preserve and reconcile this installation before DDL.')
    observed = run(['systemctl', '--user', 'show', unit(config), '--property=LoadState',
                    '--property=ActiveState', '--property=UnitFileState'], capture=True)
    properties = dict(line.split('=', 1) for line in observed.splitlines() if '=' in line)
    if properties.get('LoadState') == 'not-found':
        return
    if (properties.get('LoadState') != 'loaded'
            or properties.get('ActiveState') not in ('inactive', 'failed')
            or properties.get('UnitFileState') != 'disabled'):
        raise ValueError('An existing Worker is not stopped and disabled; initial-install DDL was not repeated.')


def deploy(root: Path, args: argparse.Namespace) -> None:
    release = manifest(BUNDLE)
    preflight()
    with locked(root):
        pending = root / "pending.json"
        if (root / "installation.json").exists():
            config = configuration(root)
            if any(config.get(key) != value for key, value in release.items()):
                raise ValueError("Existing installation: select its update.sh to change versions.")
            if not pending.exists():
                if (root / "current").is_symlink():
                    verify_console(config)
                    verify_worker(config)
                    print(f'Already installed: {config["version"]}')
                    return
                # The first manifest may have committed immediately before a crash.
                # Resume its identity rather than generating a new password.
                save(pending, {"operation": "install", "phase": "migrating", "version": release["version"]})
            intent = json.loads(pending.read_text())
            if intent['operation'] != 'install':
                raise ValueError('An update is pending; retry that target with update.sh.')
            if intent.get('phase', 'migrating') not in ('migrating', 'starting'):
                raise ValueError('Unknown initial-install phase; preserve its recovery record.')
            if intent.get('phase') == 'starting' or (root / 'current').is_symlink():
                # starting is durable before either processor can admit work.
                # current also supports already-activated older install markers.
                start_prepared_release(root, config)
                return
        else:
            if any((root / name).exists() for name in ("data", "current", "releases")):
                raise ValueError("Existing deployment files have no installation manifest; restore it rather than reinitialize.")
            validate_ports(args.port, args.database_port, available=True)
            home = Path.home()
            codex_home = Path(args.codex_home or str(root.with_name(root.name + '-codex'))).expanduser().resolve()
            config = {
                **release, "root": str(root), "uid": os.getuid(), "gid": os.getgid(),
                "home": str(home), "codex_home": str(codex_home), "path": os.environ.get("PATH", os.defpath),
                "unit_directory": str(Path(os.environ.get("XDG_CONFIG_HOME", str(home / ".config"))) / "systemd/user"),
                "port": args.port, "database_port": args.database_port, "password": secrets.token_hex(32),
                "project": "quazonai-" + hashlib.sha256(str(root).encode()).hexdigest()[:12],
            }
            validate_configuration_paths(config)
            require_new_docker_project(config['project'])
            codex_home.mkdir(mode=0o700, parents=True, exist_ok=True)
            # Persist ownership and credentials before downloading or creating any state.
            config["bundle"] = str(BUNDLE)
            save(root / "installation.json", config)
            save(pending, {"operation": "install", "phase": "migrating", "version": release["version"]})
        config = prepare(BUNDLE, config)
        save(root / "installation.json", config)
        durable_directory(root / 'data')
        compose(config, 'up', '-d', '--wait', '--wait-timeout', '120', 'database')
        require_install_stopped(config)
        initialize_state(config)
        compose(config, 'run', '--rm', '--no-deps', 'app', 'migrate')
        configure_worker(config)
        compose(config, 'up', '--no-start', '--no-deps', 'app')
        # No application process is started until its migration and complete
        # native configuration are represented by this durable recovery phase.
        save(pending, {'operation': 'install', 'phase': 'starting', 'version': config['version']})
        start_prepared_release(root, config)


def apply_update(root: Path) -> None:
    preflight()
    with locked(root):
        old = configuration(root)
        pending_file = root / "pending.json"
        pending = json.loads(pending_file.read_text()) if pending_file.exists() else None
        if pending:
            if pending["operation"] != "update" or pending["target"]["version"] != manifest(BUNDLE)["version"]:
                raise ValueError("Retry the interrupted installation or the original pending update target.")
            old = pending["previous"]
        if version_precedence(manifest(BUNDLE)['version']) < version_precedence(old['version']):
            raise ValueError('Application updates cannot downgrade a release. Use an explicit cold restore with the matching database and state.')
        candidate = prepare(BUNDLE, old)
        if old["database_image"] != candidate["database_image"]:
            raise ValueError("Application updates do not upgrade PostgreSQL; use a database upgrade procedure.")
        if not pending and old["version"] == candidate["version"]:
            verify_console(old)
            verify_worker(old)
            print(f'Already active: {old["version"]}')
            return
        if pending and candidate != pending['target']:
            raise ValueError('Pending target changed; preserve its original deployment bundle and configuration.')
        # Missing phase denotes an older recovery point which may already have
        # been migrated. Only an explicit preparing phase permits old-code recovery.
        phase = pending.get('phase', 'migrating') if pending else 'preparing'
        if phase not in ('preparing', 'migrating', 'starting'):
            raise ValueError('Unknown pending update phase; preserve its recovery record.')
        if phase == 'starting':
            start_prepared_release(root, candidate)
            return
        if pending and phase == 'preparing':
            # An interrupted shutdown may have left old processors stopped.
            # Resume, but never recreate or restart already-running processors.
            resume_existing_services(old)
            configure_app_restarts(old, True)
        # Every retry checks before shutdown, including preparing retries whose
        # old app may have admitted Runs while the installer was absent.
        require_idle(old)
        if old.get('codex_image'):
            import codex
            codex.require_stopped(old, recover_created=True)
        if not pending:
            pending = {'operation': 'update', 'phase': 'preparing', 'previous': old,
                       'target': candidate, 'backup': None}
            # save() fsyncs the file and parent before any restart policy changes.
            save(pending_file, pending)
        try:
            configure_app_restarts(old, False)
            compose(old, 'stop', 'app')
            # Closing admissions can finish a request which raced the first
            # observation. Discover its Run before stopping its Worker.
            require_idle(old)
            run(['systemctl', '--user', 'disable', '--now', unit(old)])
            require_idle(old)
            if old.get('codex_image'):
                import codex
                codex.require_stopped(old, recover_created=True)
            if phase == 'preparing':
                recovery = backup(old)
        except Exception:
            if phase == 'preparing':
                # This phase never attempts DDL. Preserve the intent if recovery
                # also fails, and enable the Worker before app boot recovery.
                resume_existing_services(old)
                configure_app_restarts(old, True)
                pending_file.unlink()
                sync_directory(root)
            raise
        if phase == 'preparing':
            # backup() synchronizes all artifacts before this durable transition.
            # Failure leaves processors stopped with the prior preparing intent.
            pending = {**pending, 'phase': 'migrating', 'backup': str(recovery)}
            save(pending_file, pending)
        # Processors remain stopped on any DDL/configuration failure. Never
        # enable old executables against a possibly forward-migrated schema.
        compose(candidate, 'run', '--rm', '--no-deps', 'app', 'migrate')
        configure_worker(candidate)
        # Replace the stopped old container with the target image/config without
        # starting it. Starting-phase retries can then preserve this exact one.
        compose(candidate, 'up', '--no-start', '--no-deps', 'app')
        save(pending_file, {**pending, 'phase': 'starting'})
        start_prepared_release(root, candidate)


def unpack(data: bytes, destination: Path) -> None:
    with tarfile.open(fileobj=io.BytesIO(data), mode="r:gz") as archive:
        members = archive.getmembers()
        if len(members) != len(BUNDLE_FILES) or {x.name for x in members} != BUNDLE_FILES:
            raise ValueError("Unexpected deployment bundle files.")
        for item in members:
            if not item.isfile() or item.size > 512_000:
                raise ValueError("Invalid deployment bundle member.")
            source = archive.extractfile(item)
            if source is None:
                raise ValueError("Missing deployment bundle member.")
            (destination / item.name).write_bytes(source.read())
    manifest(destination)


def download_update(root: Path, target: str) -> None:
    target = version(target)
    current = configuration(root)
    if version_precedence(target) < version_precedence(current['version']):
        raise ValueError('Application updates cannot downgrade a release. Use an explicit cold restore with the matching database and state.')
    url = f"https://github.com/{REPOSITORY}/releases/download/{target}/quazonai-deploy.tar.gz"
    with urllib.request.urlopen(url, timeout=60) as response:
        data = response.read(3_000_001)
    if len(data) > 3_000_000:
        raise ValueError("Deployment bundle exceeds the expected size.")
    with tempfile.TemporaryDirectory(prefix="quazonai-update-") as temporary:
        bundle = Path(temporary)
        unpack(data, bundle)
        if manifest(bundle)["version"] != target:
            raise ValueError("Downloaded bundle does not match the requested version.")
        # Apply the target's installer and Compose, not stale copies from the old version.
        run([sys.executable, str(bundle / "manage.py"), "apply-update", "--directory", str(root)])


def run_runtime(root: Path, operation: str, config_path: Path) -> None:
    config = configuration(root)
    if (root / "pending.json").exists():
        raise ValueError("Complete the recorded application update before selecting its Runtime.")
    release = manifest(Path(config["bundle"]))
    native_config = json.loads(config_path.read_text())
    registered = native_config.get("images")
    if not isinstance(registered, list) or not registered or any(
        not isinstance(item, dict) or item.get("image_ref") != release["runtime_image"]
        for item in registered
    ):
        raise ValueError("Runtime images must match this release's runtime_image digest.")
    if not release["runtime_image"].startswith("sha256:"):
        run(["docker", "pull", release["runtime_image"]])
    binary = root / "releases" / config["version"] / "bin/runtime"
    # Hand control to the native gateway; preserve signals and its real exit status.
    os.execv(str(binary), [str(binary), operation, "--config", str(config_path)])


def main() -> None:
    os.umask(0o077)
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=["deploy", "update", "apply-update", "status", "runtime"])
    parser.add_argument("version", nargs="?")
    parser.add_argument("--directory", type=Path, default=Path.home() / ".local/share/quazonai")
    parser.add_argument("--port", type=int, default=8081)
    parser.add_argument("--database-port", type=int, default=55432)
    parser.add_argument("--codex-home")
    parser.add_argument("--config", type=Path)
    args = parser.parse_args()
    root = args.directory.expanduser().resolve()
    if root in {Path("/"), Path.home().resolve()}:
        parser.error("Select a dedicated installation directory.")
    try:
        validate_ports(args.port, args.database_port)
    except ValueError as error:
        parser.error(str(error))
    if args.command == "deploy":
        deploy(root, args)
    elif args.command == "apply-update":
        apply_update(root)
    elif args.command == "update":
        if not args.version:
            parser.error("update requires an explicit release tag")
        download_update(root, args.version)
    elif args.command == "runtime":
        if args.version not in ("doctor", "serve") or args.config is None:
            parser.error("runtime requires doctor|serve and --config /absolute/runtime.json")
        run_runtime(root, args.version, args.config.expanduser().resolve())
    else:
        config = configuration(root)
        print(f'Recorded release: {config["version"]}; http://localhost:{config["port"]}')
        if (root / "pending.json").exists():
            print("An interrupted operation exists: inspect pending.json locally; it contains private deployment configuration.")
        compose(config, "ps")
        run(["systemctl", "--user", "status", "--no-pager", unit(config)])


if __name__ == "__main__":
    try:
        main()
    except (ValueError, KeyError, OSError, tarfile.TarError, subprocess.CalledProcessError) as error:
        print(f"Deployment failed: {error}", file=sys.stderr)
        sys.exit(1)
