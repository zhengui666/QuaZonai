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
import subprocess
import sys
import tarfile
import tempfile
import time
import urllib.request

REPOSITORY = "zhengui666/QuaZonai"
DATABASE_IMAGE = "ghcr.io/pgmq/pg18-pgmq@sha256:bfb3537068ce453609744518ece92b178ac89dff53747d47ca6fab91c2fc66a6"
BUNDLE = Path(__file__).resolve().parent
BUNDLE_FILES = {"manage.py", "deploy.sh", "update.sh", "compose.yaml", "release.json", "README.md"}
SEMVER = re.compile(r"v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(?:-([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?")


def version(value: str) -> str:
    match = SEMVER.fullmatch(value)
    if not match or len(value) > 128:
        raise ValueError("Expected vMAJOR.MINOR.PATCH[-prerelease], without build metadata.")
    if match[4] and any(x.isdigit() and len(x) > 1 and x[0] == "0" for x in match[4].split(".")):
        raise ValueError("Numeric prerelease identifiers cannot have leading zeroes.")
    return value


def validate_manifest(value: dict) -> dict:
    if value.get("schema_version") != 1:
        raise ValueError("Unsupported deployment manifest schema.")
    version(value["version"])
    if not re.fullmatch(r"[0-9a-f]{40}", value["revision"]):
        raise ValueError("A release must identify its exact source revision.")
    # Bare content-addressed image IDs let the same installer verify locally built
    # images in CI. Published manifests always use the GHCR repository digest.
    if not re.fullmatch(r"(?:ghcr\.io/zhengui666/quazonai@)?sha256:[0-9a-f]{64}", value["image"]):
        raise ValueError("Application image must use a digest, not a mutable tag.")
    if value["database_image"] != DATABASE_IMAGE:
        raise ValueError("This installer requires the release's supported PostgreSQL/PGMQ image.")
    return value


def manifest(bundle: Path) -> dict:
    return validate_manifest(json.loads((bundle / "release.json").read_text()))


def run(args: list[str], *, capture: bool = False, env=None, output=None) -> str:
    result = subprocess.run(args, check=True, env=env,
                            stdout=subprocess.PIPE if capture else output, text=capture)
    return result.stdout.strip() if capture else ""


def save(path: Path, value: dict) -> None:
    with tempfile.NamedTemporaryFile(mode="w", dir=path.parent, delete=False) as stream:
        temporary = Path(stream.name)
        os.chmod(temporary, 0o600)
        json.dump(value, stream, indent=2)
        stream.write("\n")
        stream.flush()
        os.fsync(stream.fileno())
    os.replace(temporary, path)
    fd = os.open(path.parent, os.O_RDONLY | os.O_DIRECTORY)
    try:
        os.fsync(fd)
    finally:
        os.close(fd)


@contextlib.contextmanager
def locked(root: Path):
    root.mkdir(mode=0o700, parents=True, exist_ok=True)
    with (root / ".deployment.lock").open("a") as stream:
        fcntl.flock(stream, fcntl.LOCK_EX | fcntl.LOCK_NB)
        yield


def configuration(root: Path) -> dict:
    value = json.loads((root / "installation.json").read_text())
    if value["uid"] != os.getuid() or value["root"] != str(root):
        raise ValueError("Manage this installation as its original user at its original path.")
    return value


def preflight() -> None:
    if sys.platform != "linux" or os.getuid() == 0:
        raise ValueError("Run as the Linux user who owns Codex, not with sudo.")
    if not Path("/sys/fs/cgroup/cgroup.controllers").is_file():
        raise ValueError("Native Missions require cgroup v2.")
    for binary in ("docker", "systemctl", "systemd-run", "prlimit", "git", "rg"):
        if not shutil.which(binary):
            raise ValueError(f"Required executable is missing: {binary}")
    host = os.environ.get("DOCKER_HOST")
    endpoint = host or run(["docker", "context", "inspect", "--format", "{{.Endpoints.docker.Host}}"], capture=True)
    if not endpoint.startswith("unix:"):
        raise ValueError("Use a local Unix-socket Docker daemon; remote hosts cannot share these paths.")
    architecture = run(["docker", "info", "--format", "{{.OSType}}/{{.Architecture}}"], capture=True)
    if architecture not in {"linux/x86_64", "linux/amd64"}:
        raise ValueError("This native release supports Linux x86_64.")
    compose_version = run(["docker", "compose", "version", "--short"], capture=True)
    match = re.match(r"v?(\d+)\.(\d+)", compose_version)
    if not match or tuple(map(int, match.groups())) < (2, 20):
        raise ValueError("Docker Compose 2.20 or newer is required.")
    run(["systemctl", "--user", "show-environment"], capture=True)
    if run(["loginctl", "show-user", str(os.getuid()), "--property=Linger", "--value"], capture=True) != "yes":
        raise ValueError('Enable persistent user services first: loginctl enable-linger "$USER"')
    run(["systemd-run", "--user", "--scope", "--quiet", "--collect", "/usr/bin/true"])


def compose(config: dict, *args: str, capture: bool = False, output=None) -> str:
    env = {k: v for k, v in os.environ.items() if not k.startswith("COMPOSE_")}
    env.update({
        "APP_IMAGE": config["image"], "DATABASE_IMAGE": config["database_image"],
        "DATABASE_PASSWORD": config["password"], "INSTALL_DIR": config["root"],
        "WEB_PORT": str(config["port"]), "DATABASE_PORT": str(config["database_port"]),
        "HOST_UID": str(config["uid"]), "HOST_GID": str(config["gid"]),
        "NATIVE_CODEX_HOME": config["codex_home"],
        "RUNTIME_TARGETS": json.dumps(config.get("runtime_targets", [])),
        "DOWNSTREAM_TARGETS": json.dumps(config.get("downstream_targets", [])),
    })
    command = ["docker", "compose", "--project-name", config["project"], "--env-file", "/dev/null",
               "-f", str(Path(config["bundle"]) / "compose.yaml")]
    override = Path(config["root"]) / "compose.override.yaml"
    if override.is_file():
        command += ["-f", str(override)]
    return run(command + list(args), capture=capture, env=env, output=output)


def sql(config: dict, statement: str) -> str:
    return compose(config, "exec", "-T", "database", "psql", "-X", "-U", "quazonai", "-d", "quazonai",
                   "-v", "ON_ERROR_STOP=1", "-Atc", statement, capture=True)


def require_idle(config: dict) -> None:
    if sql(config, "SELECT count(*) FROM app.runs WHERE finished_at IS NULL") != "0":
        raise ValueError("Unfinished Runs exist. Finish or cancel/reconcile them before updating; no task records were changed.")


def prepare(bundle: Path, config: dict) -> dict:
    release = manifest(bundle)
    destination = Path(config["root"]) / "releases" / release["version"]
    stored = destination / "deployment"
    if (stored / "release.json").exists() and manifest(stored) != release:
        raise ValueError("An installed release version cannot be replaced with different content.")
    destination.mkdir(mode=0o700, parents=True, exist_ok=True)
    if stored.resolve() != bundle.resolve():
        stored.mkdir(mode=0o700, exist_ok=True)
        # The manifest is copied last, so interrupted bundle copies can be retried.
        for name in sorted(BUNDLE_FILES - {"release.json"}) + ["release.json"]:
            shutil.copyfile(bundle / name, stored / name)
    image = release["image"]
    if not image.startswith("sha256:"):
        run(["docker", "pull", image])
    revision = run(["docker", "image", "inspect", "--format",
                    '{{index .Config.Labels "org.opencontainers.image.revision"}}', image], capture=True)
    if revision != release["revision"]:
        raise ValueError("Image source revision does not match the deployment manifest.")
    binaries = destination / "bin"
    if not binaries.exists():
        temporary = destination / "bin.partial"
        if temporary.exists():
            shutil.rmtree(temporary)
        temporary.mkdir(mode=0o700)
        container = run(["docker", "create", image], capture=True)
        try:
            run(["docker", "cp", f"{container}:/opt/quazonai/bin/.", str(temporary)])
            # Run before stopping the old release: host shared-library incompatibility
            # must not take an existing installation offline.
            run([str(temporary / "server"), "--version"])
            run([str(temporary / "codex"), "--version"])
            temporary.rename(binaries)
        finally:
            run(["docker", "rm", container], capture=True)
    return {**config, **release, "bundle": str(stored)}


def quote(value: str, *, specifiers: bool = True) -> str:
    if any(x in value for x in "\n\r\x00"):
        raise ValueError("Installation paths and environment values must be single-line strings.")
    value = value.replace("\\", "\\\\").replace('"', '\\"')
    if specifiers:
        value = value.replace("%", "%%")
    return '"' + value + '"'


def unit(config: dict) -> str:
    return config["project"] + ".service"


def start_worker(config: dict) -> None:
    root = Path(config["root"])
    binary = root / "releases" / config["version"] / "bin/server"
    environment = {
        "DATABASE_URL": f'postgresql://quazonai:{config["password"]}@127.0.0.1:{config["database_port"]}/quazonai',
        "STATE_DIR": str(root / "data/state"), "HOME": config["home"],
        "CODEX_HOME": config["codex_home"], "PUBLIC_URL": f'http://localhost:{config["port"]}',
        "DEVELOPMENT_HTTP": "true", "PATH": f'{binary.parent}:{config["path"]}',
        "RUNTIME_TARGETS": json.dumps(config.get("runtime_targets", [])),
        "DOWNSTREAM_TARGETS": json.dumps(config.get("downstream_targets", [])),
    }
    with (root / "worker.env").open("w") as stream:
        os.chmod(stream.name, 0o600)
        for name, value in environment.items():
            stream.write(name + "=" + quote(value, specifiers=False) + "\n")
    units = Path(config["unit_directory"])
    units.mkdir(parents=True, exist_ok=True)
    (units / unit(config)).write_text(
        "[Unit]\nDescription=QuaZonai native Worker for the container deployment\n"
        "[Service]\nType=simple\nUMask=0077\n"
        f"WorkingDirectory={quote(str(root / 'data'))}\n"
        f"EnvironmentFile={quote(str(root / 'worker.env'))}\n"
        f"ExecStart=:{quote(str(binary))} worker\n"
        "Restart=on-failure\nRestartSec=3\nTimeoutStopSec=90\n"
        "[Install]\nWantedBy=default.target\n"
    )
    run(["systemctl", "--user", "daemon-reload"])
    run(["systemctl", "--user", "enable", "--now", unit(config)])
    verify_worker(config)


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


def activate(root: Path, config: dict) -> None:
    verify_console(config)
    save(root / "installation.json", config)
    link = root / "current.next"
    link.unlink(missing_ok=True)
    link.symlink_to(root / "releases" / config["version"], target_is_directory=True)
    os.replace(link, root / "current")
    (root / "pending.json").unlink(missing_ok=True)
    print(f'Active release {config["version"]}: http://localhost:{config["port"]}')


def backup(config: dict) -> Path:
    root = Path(config["root"])
    destination = root / "backups" / (time.strftime("%Y%m%dT%H%M%S") + "-" + secrets.token_hex(3))
    destination.mkdir(mode=0o700, parents=True)
    with (destination / "database.dump").open("xb") as stream:
        compose(config, "exec", "-T", "database", "pg_dump", "-U", "quazonai", "-d", "quazonai", "-Fc", output=stream)
    shutil.copyfile(root / "data/state/master.key", destination / "master.key")
    with tarfile.open(destination / "data.tar.gz", "w:gz") as archive:
        archive.add(root / "data", arcname="data", filter=lambda x: None if x.name == "data/state/master.key" else x)
    save(destination / "installation.json", config)
    print(f"Recovery point: {destination}; move a copy of master.key to separate protected storage.")
    return destination


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
                save(pending, {"operation": "install", "version": release["version"]})
            if json.loads(pending.read_text())["operation"] != "install":
                raise ValueError("An update is pending; retry that target with update.sh.")
        else:
            if any((root / name).exists() for name in ("data", "current", "releases")):
                raise ValueError("Existing deployment files have no installation manifest; restore it rather than reinitialize.")
            home = Path.home()
            codex_home = Path(args.codex_home or os.environ.get("CODEX_HOME", str(home / ".codex"))).expanduser().resolve()
            codex_home.mkdir(mode=0o700, parents=True, exist_ok=True)
            config = {
                **release, "root": str(root), "uid": os.getuid(), "gid": os.getgid(),
                "home": str(home), "codex_home": str(codex_home), "path": os.environ.get("PATH", os.defpath),
                "unit_directory": str(Path(os.environ.get("XDG_CONFIG_HOME", str(home / ".config"))) / "systemd/user"),
                "port": args.port, "database_port": args.database_port, "password": secrets.token_hex(32),
                "project": "quazonai-" + hashlib.sha256(str(root).encode()).hexdigest()[:12],
            }
            # Persist ownership and credentials before downloading or creating any state.
            config["bundle"] = str(BUNDLE)
            save(root / "installation.json", config)
            save(pending, {"operation": "install", "version": release["version"]})
        config = prepare(BUNDLE, config)
        save(root / "installation.json", config)
        (root / "data").mkdir(mode=0o700, exist_ok=True)
        compose(config, "up", "-d", "--wait", "--wait-timeout", "120", "database")
        initialize_state(config)
        compose(config, "run", "--rm", "--no-deps", "app", "migrate")
        try:
            compose(config, "up", "-d", "--wait", "--wait-timeout", "120", "app")
            start_worker(config)
            activate(root, config)
        except Exception:
            # Keep a partial installation and its credentials available for retry.
            subprocess.run(["systemctl", "--user", "stop", unit(config)], check=False)
            compose(config, "stop", "app")
            raise


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
        candidate = prepare(BUNDLE, old)
        if old["database_image"] != candidate["database_image"]:
            raise ValueError("Application updates do not upgrade PostgreSQL; use a database upgrade procedure.")
        if not pending and old["version"] == candidate["version"]:
            verify_console(old)
            verify_worker(old)
            print(f'Already active: {old["version"]}')
            return
        if not pending:
            require_idle(old)
        # Stop admissions first, then the worker, then repeat the observation.
        compose(old, "stop", "app")
        run(["systemctl", "--user", "stop", unit(old)])
        if not pending:
            try:
                require_idle(old)
                recovery = backup(old)
            except Exception:
                compose(old, "up", "-d", "--wait", "--wait-timeout", "120", "app")
                run(["systemctl", "--user", "start", unit(old)])
                raise
            pending = {"operation": "update", "previous": old, "target": candidate, "backup": str(recovery)}
            save(pending_file, pending)
        else:
            if candidate != pending["target"]:
                raise ValueError("Pending target changed; preserve its original deployment bundle and configuration.")
            require_idle(old)
        try:
            compose(candidate, "run", "--rm", "--no-deps", "app", "migrate")
            compose(candidate, "up", "-d", "--wait", "--wait-timeout", "120", "app")
            start_worker(candidate)
            activate(root, candidate)
        except Exception:
            # A forward migration may have committed. Never automatically run old
            # binaries against that schema, overwrite keys, or delete data volumes.
            subprocess.run(["systemctl", "--user", "stop", unit(candidate)], check=False)
            compose(candidate, "stop", "app")
            raise


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
    configuration(root)
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


def main() -> None:
    os.umask(0o077)
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=["deploy", "update", "apply-update", "status"])
    parser.add_argument("version", nargs="?")
    parser.add_argument("--directory", type=Path, default=Path.home() / ".local/share/quazonai")
    parser.add_argument("--port", type=int, default=8081)
    parser.add_argument("--database-port", type=int, default=55432)
    parser.add_argument("--codex-home")
    args = parser.parse_args()
    root = args.directory.expanduser().resolve()
    if root in {Path("/"), Path.home().resolve()}:
        parser.error("Select a dedicated installation directory.")
    if not (1024 <= args.port <= 65535 and 1024 <= args.database_port <= 65535) or args.port == args.database_port:
        parser.error("Select distinct web/database ports between 1024 and 65535.")
    if args.command == "deploy":
        deploy(root, args)
    elif args.command == "apply-update":
        apply_update(root)
    elif args.command == "update":
        if not args.version:
            parser.error("update requires an explicit release tag")
        download_update(root, args.version)
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
