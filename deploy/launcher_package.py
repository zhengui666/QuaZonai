"""Distribute the existing offline owner-environment builder; no Nautilus binaries."""
import json
import shutil
import tarfile
import tempfile
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
PLATFORMS = {'linux-x86_64': 'tar.gz', 'macos-x86_64': 'tar.gz', 'macos-aarch64': 'tar.gz', 'windows-x86_64': 'zip'}
ARCHIVES = {p: f'quazonai-launcher-builder-{p}.{suffix}' for p, suffix in PLATFORMS.items()}
MODULES = ('rust-environment.ts', 'rust-launcher-build.ts', 'rust-native-dependencies.ts')
GLUE = ('Cargo.toml', 'Cargo.lock', 'apps/job/Cargo.toml', 'apps/job/build.rs', 'apps/job/build_identity.rs', 'apps/job/src', 'apps/job/examples', 'apps/job/tests', 'crates/contracts/Cargo.toml', 'crates/contracts/src', 'crates/domain/Cargo.toml', 'crates/domain/src')

def verify(path, platform, version=None, revision=None):
    with zipfile.ZipFile(path) if platform.startswith('windows-') else tarfile.open(path, 'r:gz') as archive:
        if platform.startswith('windows-'):
            names = archive.namelist()
            read = archive.read
        else:
            if any(not item.isfile() for item in archive.getmembers()):
                raise ValueError('Builder archive must contain regular files only')
            names = archive.getnames()
            read = lambda name: archive.extractfile(name).read()
        required = {'distribution.json', 'runtime/LICENSE', 'app/package.json', 'app/launcher-build.mjs', *('app/' + x for x in MODULES), 'glue/apps/job/src/native_runner.rs', 'runtime/node.exe' if platform.startswith('windows-') else 'runtime/node', 'quazonai-launcher-builder.cmd' if platform.startswith('windows-') else 'quazonai-launcher-builder'}
        for item in GLUE:
            source = ROOT/item
            if source.is_dir():
                required.update('glue/' + p.relative_to(ROOT).as_posix() for p in source.rglob('*') if p.is_file() and not p.is_symlink())
            else:
                required.add('glue/' + item)
        allowed = lambda n: n in required or any(n == 'glue/' + x or n.startswith('glue/' + x + '/') for x in GLUE)
        if len(set(names)) != len(names) or not required.issubset(names) or any(not allowed(n) or '..' in Path(n).parts or '\\' in n for n in names):
            raise ValueError('Incomplete or invalid launcher builder archive')
        identity = json.loads(read('distribution.json'))
        if identity['platform'] != platform or identity['kind'] != 'owner-launcher-builder' or identity['bundlesNautilus'] is not False:
            raise ValueError('Builder identity mismatch')
        if version is not None and (identity['version'] != version or identity['revision'] != revision):
            raise ValueError('Builder release mismatch')

def pack(runtime, platform, output, version, revision):
    output.mkdir(parents=True, exist_ok=True)
    windows = platform.startswith('windows-')
    with tempfile.TemporaryDirectory(prefix='qz-launcher-builder-') as tmp:
        root = Path(tmp)
        (root/'app').mkdir(); (root/'runtime').mkdir()
        for name in MODULES:
            shutil.copy2(ROOT/'apps/nautilus/src'/name, root/'app'/name)
        shutil.copy2(ROOT/'deploy/launcher-build.mjs', root/'app/launcher-build.mjs')
        (root/'app/package.json').write_text('{"type":"module"}\n')
        for name in GLUE:
            source, target = ROOT/name, root/'glue'/name
            target.parent.mkdir(parents=True, exist_ok=True)
            if source.is_dir(): shutil.copytree(source, target)
            else: shutil.copy2(source, target)
        shutil.copy2(runtime/('node.exe' if windows else 'bin/node'), root/'runtime'/('node.exe' if windows else 'node'))
        shutil.copy2(runtime/'LICENSE', root/'runtime/LICENSE')
        (root/'distribution.json').write_text(json.dumps(dict(version=version, revision=revision, platform=platform, kind='owner-launcher-builder', bundlesNautilus=False))+'\n')
        if windows:
            (root/'quazonai-launcher-builder.cmd').write_text('@echo off\n"%~dp0runtime\\node.exe" --experimental-strip-types "%~dp0app\\launcher-build.mjs" %*\nexit /b %errorlevel%\n')
        else:
            entry = root/'quazonai-launcher-builder'
            entry.write_text('#!/bin/sh\nroot=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)\nexec "$root/runtime/node" --experimental-strip-types "$root/app/launcher-build.mjs" "$@"\n')
            entry.chmod(0o755)
        path = output/ARCHIVES[platform]
        files = [p for p in root.rglob('*') if p.is_file() and not p.is_symlink()]
        with zipfile.ZipFile(path, 'w', compression=zipfile.ZIP_DEFLATED) if windows else tarfile.open(path, 'w:gz') as archive:
            for file in files:
                name = file.relative_to(root).as_posix()
                if windows: archive.write(file, name)
                else: archive.add(file, arcname=name, recursive=False)
    verify(path, platform, version, revision)
    return path
