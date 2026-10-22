#!/usr/bin/env python3
"""Package JS/TypeScript + official prebuilt Node; no SEA, compilation or checksums."""
from __future__ import annotations
import argparse
import json
import os
from pathlib import Path, PurePosixPath
import re
import shutil
import subprocess
import tarfile
import tempfile
import zipfile

ROOT = Path(__file__).resolve().parent.parent
REPOSITORY = 'zhengui666/QuaZonai'
NODE_VERSION = '22.19.0'
CLI_ARCHIVES = {platform: f'quazonai-cli-{platform}.{suffix}' for platform, suffix in (
    ('linux-x86_64', 'tar.gz'), ('macos-x86_64', 'tar.gz'),
    ('macos-aarch64', 'tar.gz'), ('windows-x86_64', 'zip'))}
IMAGE_ARCHIVES = {name: f'quazonai-image-{name}.tar.gz' for name in ('application', 'runtime', 'database')}
NOTICES = {'LICENSE', 'NOTICE', 'THIRD_PARTY_NOTICES.md'}
DEPLOY_FILES = frozenset({'manage.sh', 'json.awk', 'deploy.sh', 'update.sh', 'compose.yaml', 'release.json', 'README.md', '.env.example'})
REQUIRED_ASSETS = frozenset({'release.json', 'quazonai-deploy.tar.gz', 'README.md', 'install.sh', 'install.ps1'} | set(CLI_ARCHIVES.values()) | set(IMAGE_ARCHIVES.values()))
MARKER = '@QUAZONAI_VERSION@'

def identity(version: str, revision: str) -> None:
    if not re.fullmatch(r'v[0-9]+\.[0-9]+\.[0-9]+(?:-[0-9A-Za-z.-]+)?', version) or not re.fullmatch(r'[0-9a-f]{40}', revision):
        raise ValueError('Expected an exact release tag and source revision.')

def regular(path: Path) -> None:
    if not path.is_file() or path.stat().st_size == 0:
        raise ValueError(f'Missing or empty asset: {path.name}')

def safe_name(name: str) -> bool:
    value = PurePosixPath(name)
    return bool(name) and not value.is_absolute() and '..' not in value.parts and '\\' not in name

def cli_members(platform: str) -> set[str]:
    return NOTICES | {'distribution.json', 'runtime/LICENSE',
        'runtime/node.exe' if platform.startswith('windows-') else 'runtime/node',
        'quazonai.cmd' if platform.startswith('windows-') else 'quazonai',
        'app/launch.mjs', 'app/contracts/generated/domain-v1.openapi.json',
        'app/package.json', 'app/apps/cli/package.json', 'app/apps/cli/src/main.ts', 'app/packages/contracts/src/index.ts'}

def check_cli_archive(path: Path, platform: str) -> None:
    regular(path)
    if path.suffix == '.zip':
        with zipfile.ZipFile(path) as archive:
            names = [item.filename for item in archive.infolist()]
    else:
        with tarfile.open(path, 'r:gz') as archive:
            members = archive.getmembers()
            if any(not item.isfile() for item in members):
                raise ValueError('CLI archives contain only regular files.')
            names = [item.name for item in members]
    allowed = lambda name: name in cli_members(platform) or name.startswith(('app/apps/cli/', 'app/packages/contracts/', 'app/contracts/generated/', 'licenses/'))
    if len(names) != len(set(names)) or any(not safe_name(name) or not allowed(name) for name in names) or not cli_members(platform).issubset(names):
        raise ValueError('Incomplete JS/Node CLI distribution or invalid archive path.')

def pack_cli(runtime: Path, platform: str, destination: Path, version: str, revision: str) -> Path:
    identity(version, revision)
    windows = platform.startswith('windows-')
    node = runtime / ('node.exe' if windows else 'bin/node')
    regular(node); regular(runtime / 'LICENSE')
    destination.mkdir(parents=True, exist_ok=True)
    output = destination / CLI_ARCHIVES[platform]
    with tempfile.TemporaryDirectory(prefix='quazonai-cli-', dir=os.environ.get('TMPDIR')) as temporary:
        directory = Path(temporary)
        cli = directory / 'app/apps/cli'
        cli.mkdir(parents=True)
        shutil.copytree(ROOT / 'apps/cli/src', cli / 'src')
        for name in ('package.json', 'package-lock.json'):
            shutil.copy2(ROOT / 'apps/cli' / name, cli / name)
        package = json.loads((cli / 'package.json').read_text())
        package['version'] = version.removeprefix('v')
        (cli / 'package.json').write_text(json.dumps(package, indent=2) + '\n')
        subprocess.run(['npm.cmd' if os.name == 'nt' else 'npm', 'ci', '--prefix', str(cli), '--omit=dev', '--ignore-scripts', '--no-audit', '--no-fund'], check=True)
        shutil.copytree(ROOT / 'packages/contracts/src', directory / 'app/packages/contracts/src')
        (directory / 'app/package.json').write_text('{"type":"module"}\n')
        shutil.copytree(ROOT / 'contracts/generated', directory / 'app/contracts/generated')
        shutil.copy2(ROOT / 'deploy/cli-launch.mjs', directory / 'app/launch.mjs')
        (directory / 'runtime').mkdir()
        shutil.copy2(node, directory / ('runtime/node.exe' if windows else 'runtime/node'))
        shutil.copy2(runtime / 'LICENSE', directory / 'runtime/LICENSE')
        for name in NOTICES:
            shutil.copy2(ROOT / name, directory / name)
        shutil.copytree(ROOT / 'licenses', directory / 'licenses')
        (directory / 'distribution.json').write_text(json.dumps({'version': version, 'revision': revision, 'platform': platform, 'node': NODE_VERSION}) + '\n')
        if windows:
            (directory / 'quazonai.cmd').write_text('@echo off\n"%~dp0runtime\\node.exe" --experimental-strip-types "%~dp0app\\launch.mjs" %*\nexit /b %errorlevel%\n')
        else:
            launcher = directory / 'quazonai'
            launcher.write_text('#!/bin/sh\nroot=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)\nexec "$root/runtime/node" --experimental-strip-types "$root/app/launch.mjs" "$@"\n')
            launcher.chmod(0o755)
        # npm executable links are irrelevant to CLI runtime; do not ship them.
        files = [file for file in directory.rglob('*') if file.is_file() and not file.is_symlink()]
        if windows:
            with zipfile.ZipFile(output, 'w', compression=zipfile.ZIP_DEFLATED) as archive:
                for file in files:
                    archive.write(file, file.relative_to(directory).as_posix())
        else:
            with tarfile.open(output, 'w:gz') as archive:
                for file in files:
                    archive.add(file, arcname=file.relative_to(directory).as_posix(), recursive=False)
    check_cli_archive(output, platform)
    return output

def smoke_cli(path: Path, platform: str, version: str) -> None:
    check_cli_archive(path, platform)
    with tempfile.TemporaryDirectory(prefix='quazonai-cli-check-', dir=os.environ.get('TMPDIR')) as temporary:
        directory = Path(temporary)
        if path.suffix == '.zip':
            with zipfile.ZipFile(path) as archive:
                archive.extractall(directory)
        else:
            with tarfile.open(path, 'r:gz') as archive:
                archive.extractall(directory, filter='data')
        node = directory / ('runtime/node.exe' if platform.startswith('windows-') else 'runtime/node')
        command = [str(node), '--experimental-strip-types', str(directory / 'app/launch.mjs')]
        env = {**os.environ, 'QUAZONAI_VERSION': version, 'QZ_CLI_VERSION': version}
        def run(*args):
            return subprocess.run([*command, *args], env=env, check=True, capture_output=True, text=True).stdout.strip()
        if run('--version') != f'quazonai {version}':
            raise ValueError('Extracted CLI version differs from the release tag.')
        help_text = run('--help')
        if not all(name in help_text for name in ('research', 'portfolio', 'observation', 'settings', 'extensions', 'pm list', 'campaign', 'strategy', 'maintenance', 'pm clarify-intent')):
            raise ValueError('Extracted CLI lacks workbench commands.')
        schemas = json.loads(run('schema', 'list'))
        for name in ('PmIntentClarification', 'PmExecutionEvidence', 'PmExecutionQualification', 'AssetTarget'):
            if name not in schemas.get('names', []):
                raise ValueError(f'Extracted CLI lacks the {name} contract.')
            contract = json.loads(run('schema', 'show', name))
            if contract.get('entry') != name or f'interface {name}' not in contract.get('source', ''):
                raise ValueError(f'Extracted CLI cannot read its original {name} contract.')
            if name == 'AssetTarget' and not re.search(r'export interface AssetTarget\s*\{[^}]*\bexecutionEstimate\?:', contract.get('source', '')):
                raise ValueError('Extracted CLI lacks the separate rounded execution estimate contract.')
        if 'DecisionTrigger' not in schemas.get('names', []):
            raise ValueError('Extracted CLI lacks the DecisionTrigger contract.')
        trigger = json.loads(run('schema', 'show', 'DecisionTrigger'))
        definition = re.search(r'export type DecisionTrigger\s*=([^;]+);', trigger.get('source', ''))
        if trigger.get('entry') != 'DecisionTrigger' or not definition or "'SOURCE_RECOVERED'" not in definition.group(1):
            raise ValueError('Extracted CLI lacks the source recovery decision trigger contract.')
        domain = json.loads(run('schema', 'list', '--domain'))
        for name in ('NativeObservedSpotBuildRequestV1', 'NativeObservedSpotBuildResultV1', 'AllocatorSettingsV1', 'AllocationResultV1'):
            if name not in domain.get('names', []):
                raise ValueError(f'Extracted CLI lacks the native {name} contract.')
            contract = json.loads(run('schema', 'show', name, '--domain'))
            native = contract.get('components', {}).get('schemas', {})
            if name not in native:
                raise ValueError(f'Extracted CLI cannot resolve its native {name} contract.')
            if name == 'AllocatorSettingsV1':
                protocol = 'NativeCashNumericalOutputProtocolV1'
                field = native[name].get('properties', {}).get('cash_numerical_output', {})
                if f'#/components/schemas/{protocol}' not in json.dumps(field) or 'VALIDATED_FEASIBLE_SEGMENT_V1' not in native.get(protocol, {}).get('enum', []):
                    raise ValueError('Extracted CLI lacks the registered CASH numerical output protocol.')
            if name == 'AllocationResultV1':
                trace = 'NativeCashNumericalOutputV1'
                field = native[name].get('properties', {}).get('numerical_output', {})
                exact = native.get(trace, {}).get('properties', {}).get('objective_delta_exact', {})
                value_type = exact.get('type')
                if f'#/components/schemas/{trace}' not in json.dumps(field) or not (value_type == 'string' or isinstance(value_type, list) and 'string' in value_type):
                    raise ValueError('Extracted CLI lacks the exact numerical objective trace contract.')

def read_manifest(destination: Path) -> dict:
    manifest = json.loads((destination / 'release.json').read_text())
    identity(manifest['version'], manifest['revision'])
    if manifest['schema_version'] != 4:
        raise ValueError('Node deployment requires schema version 4.')
    return manifest

def readme(manifest: dict) -> str:
    version = manifest['version']
    base = f'https://github.com/{REPOSITORY}/releases/download/{version}'
    return f'''# QuaZonai {version}

Source revision: `{manifest['revision']}`. Prebuilt Node application, scientific
image and four JS CLI archives with official Node {NODE_VERSION}; no host Node/npm.

Linux x86_64 (Docker Compose) or macOS (CLI): download `{base}/install.sh`,
then run `bash install.sh`. Add `--cli-only` for a remote Linux client and
`--directory PATH` for an existing non-default installation.
Windows x86_64: download `{base}/install.ps1`, then run it with PowerShell.

`quazonai --version` prints `quazonai {version}`. Use `quazonai login --help`.
The application listens at http://localhost:8081 by default; perform initial
password setup in the browser. Configure original datasets and provider env;
installation does not perform model inference or qualify evidence.

Preserve the installation root, configuration, SQLite, keys, PostgreSQL,
modules, journals and backups. Automatic migration of legacy Rust/PG Workers
is explicitly unsupported until their active science/downstream work and auth
archive have a reviewed cutover. No old service or data is silently replaced.
See the deployment bundle README for stopped backup/restore and update commands.
'''

def prepare(destination: Path) -> None:
    manifest = read_manifest(destination)
    for name in ('install.sh', 'install.ps1'):
        (destination / name).write_text((ROOT / 'deploy' / name).read_text().replace(MARKER, manifest['version']), newline='\n')
    (destination / 'README.md').write_text(readme(manifest))
    verify(destination)

def verify(destination: Path) -> dict:
    for name in REQUIRED_ASSETS:
        regular(destination / name)
    manifest = read_manifest(destination)
    for platform, name in CLI_ARCHIVES.items():
        check_cli_archive(destination / name, platform)
    with tarfile.open(destination / 'quazonai-deploy.tar.gz', 'r:gz') as archive:
        if set(archive.getnames()) != DEPLOY_FILES or json.load(archive.extractfile('release.json')) != manifest:
            raise ValueError('Deployment bundle does not match release.json.')
    for name in ('install.sh', 'install.ps1', 'README.md'):
        content = (destination / name).read_text()
        if MARKER in content or manifest['version'] not in content:
            raise ValueError(f'Unpinned release instructions: {name}')
    return manifest

def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest='command', required=True)
    pack = commands.add_parser('pack-cli')
    pack.add_argument('--runtime', type=Path, required=True)
    pack.add_argument('--output', type=Path, required=True)
    pack.add_argument('--platform', choices=CLI_ARCHIVES, required=True)
    pack.add_argument('--version', required=True)
    pack.add_argument('--revision', required=True)
    smoke = commands.add_parser('smoke-cli')
    smoke.add_argument('--archive', type=Path, required=True)
    smoke.add_argument('--platform', choices=CLI_ARCHIVES, required=True)
    smoke.add_argument('--version', required=True)
    for name in ('prepare', 'verify'):
        commands.add_parser(name).add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    if args.command == 'pack-cli': print(pack_cli(args.runtime, args.platform, args.output, args.version, args.revision))
    elif args.command == 'smoke-cli': smoke_cli(args.archive, args.platform, args.version)
    elif args.command == 'prepare': prepare(args.output)
    else: verify(args.output)

if __name__ == '__main__': main()
