"""Packaging contracts and real CLI pipe decoding; Windows release runs in cli.yml."""
import io
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tarfile
import tempfile
import unittest
import zipfile
import package

class PackageTests(unittest.TestCase):
    def test_node_members_and_retired_runtime(self):
        self.assertIn('runtime/node', package.cli_members('linux-x86_64'))
        self.assertIn('quazonai.cmd', package.cli_members('windows-x86_64'))
        self.assertIn('app/apps/cli/src/main.ts', package.cli_members('linux-x86_64'))
        self.assertNotIn('runtime.sh', package.DEPLOY_FILES)
        self.assertNotIn('SHA256SUMS', package.REQUIRED_ASSETS)

    def test_incomplete_cli_is_not_a_distribution(self):
        with tempfile.TemporaryDirectory(dir=os.environ.get('TMPDIR')) as temporary:
            archive = Path(temporary) / 'old.tar.gz'
            with tarfile.open(archive, 'w:gz') as out:
                entry = tarfile.TarInfo('quazonai'); entry.size = 3
                out.addfile(entry, io.BytesIO(b'old'))
            with self.assertRaisesRegex(ValueError, 'Incomplete JS/Node'):
                package.check_cli_archive(archive, 'linux-x86_64')


class CliEncodingTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        # CI already installs Node and the CLI dependencies. No network/build,
        # external release archive, schema mirror, or subprocess success mock.
        node = shutil.which('node')
        if node is None:
            raise RuntimeError('CLI decoding tests require the project Node runtime on PATH.')
        cls.node = Path(node).resolve()
        cls.temporary = tempfile.TemporaryDirectory(prefix='quazonai-encoding-', dir=os.environ.get('TMPDIR'))
        cls.addClassCleanup(cls.temporary.cleanup)
        directory = Path(cls.temporary.name)
        cls.platform = 'windows-x86_64' if os.name == 'nt' else 'linux-x86_64'
        cls.archive = directory / package.CLI_ARCHIVES[cls.platform]
        cls.version = 'v0.0.0-encoding-test'
        root = package.ROOT
        dependencies = root / 'apps/cli/node_modules'
        if not dependencies.is_dir():
            raise RuntimeError('Run npm ci --prefix apps/cli before the CLI decoding tests.')
        files = {name: root / name for name in package.NOTICES}
        files.update({
            'runtime/node.exe' if os.name == 'nt' else 'runtime/node': cls.node,
            'runtime/LICENSE': cls.node.parent / 'LICENSE' if os.name == 'nt' else cls.node.parent.parent / 'LICENSE',
            'app/launch.mjs': root / 'deploy/cli-launch.mjs',
            'app/apps/cli/package.json': root / 'apps/cli/package.json',
            'app/contracts/generated/domain-v1.openapi.json': root / 'contracts/generated/domain-v1.openapi.json',
        })
        for source in (root / 'apps/cli/src', dependencies, root / 'packages/contracts/src'):
            files.update({f'app/{path.relative_to(root).as_posix()}': path
                          for path in source.rglob('*') if path.is_file() and not path.is_symlink()})
        inline = {
            'app/package.json': b'{"type":"module"}\n',
            'distribution.json': json.dumps({'version': cls.version}).encode('utf-8'),
            # smoke_cli invokes the actual bundled Node/launch.mjs directly.
            'quazonai.cmd' if os.name == 'nt' else 'quazonai': b'encoding-test archive\n',
        }
        if os.name == 'nt':
            with zipfile.ZipFile(cls.archive, 'w', compression=zipfile.ZIP_DEFLATED, compresslevel=1) as archive:
                for name, source in files.items():
                    archive.write(source, name)
                for name, data in inline.items():
                    archive.writestr(name, data)
        else:
            with tarfile.open(cls.archive, 'w:gz', compresslevel=1) as archive:
                for name, source in files.items():
                    archive.add(source, arcname=name, recursive=False)
                for name, data in inline.items():
                    entry = tarfile.TarInfo(name); entry.size = len(data)
                    archive.addfile(entry, io.BytesIO(data))

    def smoke_with_cp1252(self, mode='source'):
        # A new interpreter explicitly disables Python's UTF-8 mode, so this
        # also exercises the locale-dependent old path on UTF-8-mode hosts.
        program = r'''
import importlib.util
import locale
import os
from pathlib import Path
import sys
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('tested_package', sys.argv[1])
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
mode = sys.argv[5]
if mode == 'stderr':
    os.environ['NODE_OPTIONS'] = '--import=data:text/javascript,process.stderr.write(Buffer.from([0xe4,0xb8,0x8d]))'
elif mode == 'invalid':
    os.environ['NODE_OPTIONS'] = '--import=data:text/javascript,process.stdout.write(Buffer.from([0xff]))'
else:
    os.environ.pop('NODE_OPTIONS', None)
assert not sys.flags.utf8_mode
with patch.object(locale, 'getencoding', return_value='cp1252'):
    try:
        module.smoke_cli(Path(sys.argv[2]), sys.argv[3], sys.argv[4])
    except UnicodeDecodeError as error:
        if mode != 'invalid' or error.encoding != 'utf-8':
            raise
    except AttributeError as error:
        # Windows subprocess reader threads can report the strict decode error
        # on stderr and leave stdout=None; the smoke must still reject it.
        if mode != 'invalid' or os.name != 'nt' or str(error) != "'NoneType' object has no attribute 'strip'":
            raise
    else:
        if mode == 'invalid':
            raise AssertionError('Malformed UTF-8 output was accepted.')
'''
        result = subprocess.run([sys.executable, '-X', 'utf8=0', '-c', program,
                                 str(Path(package.__file__).resolve()), str(self.archive), self.platform, self.version, mode],
                                capture_output=True, encoding='utf-8', errors='strict')
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_original_schema_utf8_stdout_with_cp1252_default(self):
        # The problematic bytes come from current schema source, not a canned
        # JSON fixture. The installed CLI reads this same source in smoke_cli.
        raw = subprocess.check_output([str(self.node), '--experimental-strip-types',
                                       str(package.ROOT / 'apps/cli/src/main.ts'), 'schema', 'show', 'AssetTarget'])
        self.assertIn((package.ROOT / 'packages/contracts/src/pm.ts').read_text(encoding='utf-8'),
                      json.loads(raw)['source'])
        with self.assertRaises(UnicodeDecodeError):
            raw.decode('cp1252')
        self.smoke_with_cp1252()

    def test_utf8_stderr_with_cp1252_default(self):
        self.smoke_with_cp1252('stderr')

    def test_invalid_utf8_stdout_is_rejected(self):
        self.smoke_with_cp1252('invalid')

if __name__ == '__main__': unittest.main()
