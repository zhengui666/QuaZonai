"""Execute the actual unpacked builder entry; never build or connect an account."""
import json
import subprocess
import sys
import tarfile
import tempfile
import zipfile
from pathlib import Path
import launcher_package

assets, platform, version = sys.argv[1:]
path = Path(assets)/launcher_package.ARCHIVES[platform]
launcher_package.verify(path, platform)
with tempfile.TemporaryDirectory(prefix='qz-builder-verify-') as tmp:
    root = Path(tmp)
    if platform.startswith('windows-'):
        with zipfile.ZipFile(path) as archive: archive.extractall(root)
    else:
        with tarfile.open(path, 'r:gz') as archive: archive.extractall(root, filter='data')
    entry = root/('quazonai-launcher-builder.cmd' if platform.startswith('windows-') else 'quazonai-launcher-builder')
    def run(*args):
        return subprocess.run([str(entry), *args], capture_output=True, text=True)
    result = run('--version')
    assert result.returncode == 0 and result.stdout.strip() == f'quazonai-launcher-builder {version}', result
    result = run('--help')
    assert result.returncode == 0 and 'COMPILED_NOT_CONNECTED' in result.stdout, result
    node = root/('runtime/node.exe' if platform.startswith('windows-') else 'runtime/node')
    code = """const environment = await import(process.argv[1]);
await import(process.argv[2]); await import(process.argv[3]);
if (!environment.renderRustLauncher({accountFactory:'account'}).includes('owner_strategy::account::strategy')) throw Error('missing owner factory');"""
    subprocess.run([str(node), '--experimental-strip-types', '--input-type=module', '-e', code,
        *( (root/'app'/name).as_uri() for name in launcher_package.MODULES)], check=True)
    # Invalid owner input must fail before attempting any compilation.
    config = root/'invalid.json'; config.write_text('{}')
    result = run('build', str(config))
    assert result.returncode == 1 and 'source requires' in result.stderr, result
