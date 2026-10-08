"""CI-only bridge to the shipped shell helpers; never included in an installation.

The acceptance suite keeps its Python assertions, but configuration validation,
Compose options, source isolation and Codex updates execute the installed Bash
implementation. JSON inputs use private temporary files, never command arguments.
"""
from __future__ import annotations

import contextlib
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile

BUNDLE = Path(__file__).resolve().parent
REPOSITORY = 'zhengui666/QuaZonai'


def command(bundle: Path, operation: str, root: Path, *args: str) -> list[str]:
    return ['bash', str(bundle / 'manage.sh'), operation, '--directory', str(root), *args]


def shell_command(bundle: Path, program: str, *args: str, initialize=True) -> list[str]:
    # Only trusted harness source is code. Bundle/root/arguments remain positional
    # data, including spaces, brackets, Unicode and literal shell metacharacters.
    prefix = '''set -euo pipefail
source "$1/manage.sh"
shift
source "$QZ_BUNDLE/codex.sh"
'''
    if initialize:
        prefix += '''QZ_WORK=$(mktemp -d "${TMPDIR:-/tmp}/quazonai-smoke-shell.XXXXXX")
trap 'rm -rf -- "$QZ_WORK"' EXIT
'''
    return ['bash', '-c', prefix + program, 'quazonai-smoke-shell', str(bundle), *map(str, args)]


def run(args: list[str], *, capture: bool = False, env=None, output=None) -> str:
    result = subprocess.run(args, check=True, env=env,
                            stdout=subprocess.PIPE if capture else output, text=capture)
    return result.stdout.strip() if capture else ''


def shell(bundle: Path, program: str, *args: str, capture=False) -> str:
    return run(shell_command(bundle, program, *args), capture=capture)


@contextlib.contextmanager
def json_file(value: dict):
    with tempfile.TemporaryDirectory(prefix='quazonai-smoke-json-') as temporary:
        path = Path(temporary) / 'input.json'
        descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
        with os.fdopen(descriptor, 'w') as stream:
            json.dump(value, stream)
        yield path


def configured(config: dict, program: str, *args: str, capture=False) -> str:
    with json_file(config) as path:
        return shell(Path(config['bundle']), program, str(path), *args, capture=capture)


def unpack(data: bytes, destination: Path) -> None:
    with tempfile.TemporaryDirectory(prefix='quazonai-smoke-archive-') as temporary:
        archive = Path(temporary) / 'deployment.tar.gz'
        archive.write_bytes(data)
        shell(BUNDLE, 'qz_unpack "$1" "$2"', str(archive), str(destination))


def configuration(root: Path) -> dict:
    # Locate the installed helper from the original state, then let that helper
    # enforce the owner/path contract before accepting its configuration.
    recorded = json.loads((root / 'installation.json').read_text())
    return json.loads(shell(Path(recorded['bundle']),
                           'qz_configuration "$1"; qz_raw "$QZ_CONFIG" ""', str(root), capture=True))


def manifest(bundle: Path) -> dict:
    return json.loads(shell(bundle, 'qz_manifest "$1"; qz_raw "$1" ""',
                           str(bundle / 'release.json'), capture=True))


def validate_manifest(value: dict, *, published: bool = False) -> dict:
    with json_file(value) as path:
        shell(BUNDLE, 'qz_manifest "$1"', str(path))
    if published:
        for field, repository in (('image', 'quazonai'), ('runtime_image', 'quazonai-runtime'),
                                  ('codex_image', 'quazonai-codex')):
            if not re.fullmatch(rf'ghcr\.io/zhengui666/{repository}@sha256:[0-9a-f]{{64}}', value[field]):
                raise ValueError(f'{field} must identify the published image by repository digest.')
    return value


def compose(config: dict, *args: str, capture=False) -> str:
    return configured(config, 'qz_compose "$@"', *args, capture=capture)


def unit(config: dict) -> str:
    return configured(config, 'qz_unit "$1"', capture=True)


def sql(config: dict, statement: str) -> str:
    return configured(config, 'qz_sql "$1" "$2"', statement, capture=True)


def save(path: Path, value: dict) -> None:
    with json_file(value) as source:
        shell(BUNDLE, 'qz_save "$1" "$2"', str(path), str(source))


def verify_console(config: dict) -> None:
    configured(config, 'qz_verify_console "$1"')


def verify_worker(config: dict) -> None:
    configured(config, 'qz_verify_worker "$1"')


def source_path(value: Path, config: dict, *, output=False) -> Path:
    value = configured(config, 'qz_source_path "$2" "$1" "$3"; printf "\\0"', str(value),
                       'true' if output else 'false', capture=True)
    if not value.endswith('\0'):
        raise ValueError('The installed source helper returned an invalid path.')
    return Path(value[:-1])


def source_container(config: dict, release: dict, network: str, invocation: str) -> list[str]:
    with json_file(release) as path:
        output = configured(config,
                            'qz_source_container "$1" "$2" "$3" "$4"; printf "%s\\0" "${QZ_SOURCE_COMMAND[@]}"',
                            str(path), network, invocation, capture=True)
    if not output.endswith('\0'):
        raise ValueError('The installed source helper returned an invalid command vector.')
    return output[:-1].split('\0')


class Codex:
    def docker(self, config: dict, *args: str, capture=False) -> str:
        return configured(config, 'qz_codex_docker "$@"', *args, capture=capture)

    def image_tag(self, config: dict) -> str:
        return configured(config, 'qz_codex_image_tag "$1"', capture=True)

    def image_id(self, config: dict) -> str | None:
        return configured(config, 'qz_codex_image_id "$1"', capture=True) or None

    def require_stopped(self, config: dict) -> None:
        configured(config, 'qz_codex_require_stopped "$1" false')

    def read_env(self, path: Path) -> tuple[str, str]:
        # The .env parser is a pure helper; the installed bundle is selected from
        # its installation identity when present, including after an upgrade.
        identity = path.parent / 'installation.json'
        bundle = Path(json.loads(identity.read_text())['bundle']) if identity.exists() else BUNDLE
        return shell(bundle, 'qz_codex_read_env "$1"', str(path), capture=True), path.read_text()

    def update(self, root: Path, target: str, *, reference: str | None = None) -> None:
        config = configuration(root)
        # CI uses already-built digest IDs; call the actual helper's immutable
        # reference argument instead of adding a user-facing test-only CLI flag.
        result = subprocess.run(shell_command(Path(config['bundle']), 'qz_codex_update "$1" "$2" "$3"',
                                              str(root), target, reference or ''),
                                capture_output=True, text=True, check=False)
        if result.returncode:
            raise ValueError(result.stderr.strip() or 'Installed Codex update failed.')
        print(result.stdout, end='')


codex = Codex()


FAULTS = {
    'after-restart-disable': '''eval "$(declare -f qz_restarts | sed '1s/qz_restarts/qz_smoke_original_restarts/')"
qz_restarts() {
    qz_smoke_original_restarts "$@"
    if [[ "$2" == false ]]; then
        # Bypass the shutdown subshell's ordinary-error recovery, as os._exit
        # did in the former manager. Only the installer's cleanup is interrupted.
        trap - EXIT
        exit 99
    fi
}
''',
    'before-activation': 'qz_activate() { exit 99; }\n',
    'resume-without-ddl': '''eval "$(declare -f qz_compose | sed '1s/qz_compose/qz_smoke_original_compose/')"
qz_compose() {
    local argument
    for argument in "$@"; do [[ "$argument" != migrate ]] || qz_fail 'starting retry repeated DDL'; done
    qz_smoke_original_compose "$@"
}
qz_configure_worker() { qz_fail 'starting retry rewrote the Worker'; }
''',
    'race-after-admissions-close': '''eval "$(declare -f qz_require_idle | sed '1s/qz_require_idle/qz_smoke_original_idle/')"
QZ_SMOKE_IDLE_CALLS=0
qz_require_idle() {
    qz_smoke_original_idle "$@"
    QZ_SMOKE_IDLE_CALLS=$((QZ_SMOKE_IDLE_CALLS + 1))
    if ((QZ_SMOKE_IDLE_CALLS == 2)); then qz_fail 'test interruption at the post-admission idle boundary'; fi
}
''',
}


def fault_command(bundle: Path, mode: str, operation: str, root: Path, *args: str) -> list[str]:
    return shell_command(bundle, FAULTS[mode] + 'qz_main "$@"',
                         operation, '--directory', str(root), *args, initialize=False)


def reuse_command(config: dict, invocation: str, inputs, output: Path,
                  arguments: list[str], forbidden: Path) -> list[str]:
    program = '''QZ_SMOKE_FORBIDDEN=$1
shift
docker() { printf 'docker\\n' >> "$QZ_SMOKE_FORBIDDEN"; return 98; }
exec() { printf 'exec\\n' >> "$QZ_SMOKE_FORBIDDEN"; return 98; }
qz_main "$@"
'''
    args = ['source', '--directory', config['root'], '--invocation-id', invocation]
    for path in inputs:
        args += ['--read-only', str(path)]
    args += ['--output-parent', str(output), '--', *arguments]
    return shell_command(Path(config['bundle']), program, str(forbidden), *args, initialize=False)
