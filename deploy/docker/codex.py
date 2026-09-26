#!/usr/bin/env python3
"""Pull and switch a published Codex image without touching its native home."""
from __future__ import annotations

import argparse
import contextlib
import csv
import io
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import uuid

import manage


def exact_version(value: str) -> str:
    # npm selectors, URLs, paths and ranges are never image build inputs.
    if value.startswith('v') or len(value) > 128:
        raise ValueError('Codex requires an exact npm version, such as 0.157.0.')
    manage.version('v' + value)
    return value


def requested_version(value: str) -> str:
    return "latest" if value == "latest" else exact_version(value)


def read_env(path: Path) -> tuple[str, str]:
    text = path.read_text(encoding='utf-8')
    values = []
    for line in text.splitlines():
        if not line.strip() or line.lstrip().startswith('#'):
            continue
        match = re.fullmatch(r'\s*CODEX_VERSION\s*=\s*([^\s#]+)\s*(?:#.*)?', line)
        if not match:
            raise ValueError('.env accepts only CODEX_VERSION=<exact npm version> and comments.')
        values.append(exact_version(match[1]))
    if len(values) != 1:
        raise ValueError('.env must contain exactly one CODEX_VERSION assignment.')
    return values[0], text


def docker(config: dict, *args: str, capture: bool = False) -> str:
    environment = dict(os.environ, DOCKER_HOST='unix://' + config['docker_socket'])
    environment.pop('DOCKER_CONTEXT', None)
    return manage.run(['docker', *args], capture=capture, env=environment)


def image_tag(config: dict) -> str:
    return config.get("codex_runtime_image") or config["codex_image"]


def image_id(config: dict) -> str | None:
    # `image ls` distinguishes an absent tag from an unavailable Docker daemon.
    value = docker(config, 'image', 'ls', '--quiet', '--no-trunc', image_tag(config), capture=True)
    if not value:
        return None
    if not re.fullmatch(r'sha256:[0-9a-f]{64}', value):
        raise ValueError('Codex image tag does not identify one local image.')
    return value


def require_stopped(config: dict, *, recover_created: bool = False) -> None:
    containers = docker(config, 'container', 'ls', '--all', '--quiet', '--no-trunc', '--filter',
                        'label=io.quazonai.codex.image=' + image_tag(config), capture=True).splitlines()
    for container in containers:
        state = docker(config, 'inspect', '--format', '{{.State.Status}}', container, capture=True)
        if state == 'created' and recover_created:
            # Only callers holding the launchers' exclusive deployment lock may
            # remove a never-started orphan. Docker still rejects a running ID.
            docker(config, 'container', 'rm', container, capture=True)
            continue
        if state not in {'exited', 'dead'}:
            raise ValueError('Codex sessions still exist. Finish or cancel/reconcile them before updating.')


def verify_candidate(config: dict, target: str, candidate: str) -> None:
    actual = docker(config, 'run', '--rm', '--network', 'none', '--read-only', '--cap-drop', 'ALL',
                    '--security-opt', 'no-new-privileges:true', candidate, '--version', capture=True)
    if actual != 'codex-cli ' + target:
        raise ValueError('The pulled Codex executable does not match the requested version.')
    uid, gid = config.get('uid', 1000), config.get('gid', 1000)
    try:
        docker(config, 'run', '--rm', '--network', 'none', '--read-only', '--cap-drop', 'ALL',
               '--security-opt', 'no-new-privileges:true', '--security-opt', 'seccomp=unconfined',
               '--security-opt', 'apparmor=unconfined', '--user', f'{uid}:{gid}',
               '--pids-limit', '128', '--memory', '512m', '--memory-swap', '512m',
               '--tmpfs', f'/home/codex:rw,nosuid,nodev,mode=700,uid={uid},gid={gid},size=64m',
               '--tmpfs', '/tmp:rw,nosuid,nodev,size=64m', '--env', 'CODEX_HOME=/home/codex',
               '--entrypoint', '/usr/bin/timeout', candidate, '--signal=KILL', '30',
               '/opt/codex/bin/codex', '--disable', 'use_legacy_landlock',
               '-c', 'sandbox_mode="read-only"', 'sandbox', '--', '/usr/bin/true')
    except subprocess.CalledProcessError as error:
        raise ValueError('Candidate Codex sandbox failed; the installed image/version was not changed. '
                         'On hosts restricting user namespaces with AppArmor, have an administrator install '
                         'and load this bundle\'s codex.apparmor as described in README.md, then retry. '
                         'Do not disable host-wide user-namespace policy or the Codex sandbox.') from error


def pull_candidate(config: dict, target: str, *, reference: str | None = None) -> tuple[str, str]:
    target = requested_version(target)
    selected = reference or 'ghcr.io/zhengui666/quazonai-codex:' + target
    if reference is not None and not re.fullmatch(
        r'(?:ghcr\.io/zhengui666/quazonai-codex@)?sha256:[0-9a-f]{64}', reference
    ):
        raise ValueError('A selected Codex image must be an immutable repository digest.')
    if not selected.startswith('sha256:'):
        docker(config, 'pull', selected)
    metadata = json.loads(docker(config, 'image', 'inspect', selected, capture=True))[0]
    candidate = metadata['Id']
    actual_version = exact_version(metadata['Config']['Labels']['org.opencontainers.image.version'])
    if (not re.fullmatch(r'sha256:[0-9a-f]{64}', candidate)
            or metadata['Os'] != 'linux' or metadata['Architecture'] != 'amd64'):
        raise ValueError('The published Codex image is not a Linux x86_64 image.')
    if target != 'latest' and target != actual_version:
        raise ValueError('The published Codex image does not match the requested version.')
    verify_candidate(config, actual_version, candidate)
    return actual_version, candidate


def switch(config: dict, target: str, candidate: str, previous_text: str | None) -> None:
    root = Path(config['root'])
    previous = image_id(config)
    text = 'CODEX_VERSION=' + target + '\n'
    if previous_text is not None:
        text = re.sub(r'(?m)^\s*CODEX_VERSION\s*=.*$', 'CODEX_VERSION=' + target, previous_text)
        if not text.endswith('\n'):
            text += '\n'
    try:
        docker(config, 'image', 'tag', candidate, image_tag(config))
        manage.atomic_text(root / '.env', text)
    except BaseException:
        # Image tag and file are two native stores. Roll back both on a reported
        # failure; a killed updater is detected by the next version check.
        if previous:
            docker(config, 'image', 'tag', previous, image_tag(config))
        else:
            docker(config, 'image', 'rm', image_tag(config))
        if previous_text is not None:
            manage.atomic_text(root / '.env', previous_text)
        else:
            (root / '.env').unlink(missing_ok=True)
            manage.sync_directory(root)
        raise


def prepare(config: dict, bundle: Path) -> None:
    root = Path(config['root'])
    source = root / '.env'
    if not source.exists():
        source = bundle / '.env'
    target = read_env(source)[0] if source.exists() else exact_version(config['codex_version'])
    current = image_id(config)
    if current:
        installed = docker(config, 'image', 'inspect', '--format',
                           '{{index .Config.Labels "org.opencontainers.image.version"}}', current, capture=True)
        if installed != target:
            raise ValueError('Codex image differs from .env; run codex-update.sh with the intended version before continuing.')
        if not (root / '.env').exists():
            manage.atomic_text(root / '.env', 'CODEX_VERSION=' + target + '\n')
        return
    require_stopped(config, recover_created=True)
    reference = config['codex_image'] if config['codex_version'] == target else None
    actual, candidate = pull_candidate(config, target, reference=reference)
    switch(config, actual, candidate, (root / '.env').read_text() if (root / '.env').exists() else None)


def update(root: Path, target: str, *, reference: str | None = None) -> str:
    target = requested_version(target)
    config = manage.configuration(root)
    if not config.get('codex_runtime_image') or not config.get('docker_socket'):
        raise ValueError('Upgrade the application to a prebuilt-image deployment bundle first.')
    if (root / 'pending.json').exists():
        raise ValueError('Finish the pending application deployment before changing Codex.')
    actual, candidate = pull_candidate(config, target, reference=reference)
    with manage.locked(root):
        if manage.configuration(root) != config or (root / 'pending.json').exists():
            raise ValueError('Application deployment changed during the Codex pull; retry.')
        _, previous_text = read_env(root / '.env')
        manage.require_idle(config)
        require_stopped(config, recover_created=True)
        switch(config, actual, candidate, previous_text)
    manage.announce('Codex image updated to ' + actual + '; native authentication and history were preserved.')
    return actual


def login(root: Path, *, status: bool = False) -> None:
    config = manage.configuration(root)
    with manage.locked(root):
        if (root / 'pending.json').exists():
            raise ValueError('Finish the pending application deployment before logging in.')
        manage.require_idle(config)
        require_stopped(config, recover_created=True)
        image = image_id(config)
        if image is None:
            raise ValueError('The installation has no Codex image; finish deployment or run codex-update.sh.')
        name = config['project'] + '-login-' + uuid.uuid4().hex
        mount = io.StringIO()
        csv.writer(mount, lineterminator='').writerow(
            ['type=bind', 'source=' + config['codex_home'], 'target=' + config['codex_home']])
        command = ['run', '--rm', '--name', name, '--label', 'io.quazonai.codex.image=' + image_tag(config),
                   '--user', f'{config["uid"]}:{config["gid"]}', '--read-only', '--cap-drop', 'ALL',
                   '--security-opt', 'no-new-privileges:true', '--pids-limit', '128', '--memory', '512m',
                   '--memory-swap', '512m', '--cpus', '1', '--init',
                   '--tmpfs', f'/home/codex:rw,nosuid,nodev,mode=700,uid={config["uid"]},gid={config["gid"]},size=16m',
                   '--tmpfs', '/tmp:rw,nosuid,nodev,size=64m', '--mount', mount.getvalue(),
                   '--env', 'CODEX_HOME=' + config['codex_home'],
                   image, '-c', 'cli_auth_credentials_store="file"', 'login',
                   'status' if status else '--device-auth']
        try:
            # Native device URLs/codes go only to this operator's terminal. No
            # credential file is opened, copied or included in an image layer.
            docker(config, *command)
        finally:
            with contextlib.suppress(subprocess.CalledProcessError):
                remaining = docker(config, 'container', 'ls', '--all', '--quiet', '--no-trunc',
                                   '--filter', 'name=^/' + name + '$', capture=True)
                if remaining:
                    docker(config, 'container', 'rm', '--force', remaining, capture=True)


def main() -> None:
    os.umask(0o077)
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('version', nargs='?', default='latest')
    parser.add_argument('--login', action='store_true', help='Use native ChatGPT device-code login.')
    parser.add_argument('--status', action='store_true', help='Read native login status in a fresh container.')
    parser.add_argument('--directory', type=Path, default=Path.home() / '.local/share/quazonai')
    args = parser.parse_args()
    root = args.directory.expanduser().resolve()
    if args.login or args.status:
        if args.version != 'latest':
            parser.error('Login/status does not select a version; use codex-update.sh first.')
        login(root, status=args.status)
    else:
        update(root, args.version)


if __name__ == '__main__':
    try:
        main()
    except (ValueError, KeyError, OSError, subprocess.CalledProcessError) as error:
        print(f'Codex update failed: {error}', file=sys.stderr)
        sys.exit(1)
