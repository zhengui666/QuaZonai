"""Packaging contracts and real CLI pipe decoding; Windows release runs in cli.yml."""
import io
import json
import os
from pathlib import Path
import shutil
import subprocess
import tarfile
import tempfile
import unittest
import package

class PackageTests(unittest.TestCase):
    def test_node_members_and_retired_runtime(self):
        self.assertIn('runtime/node', package.cli_members('linux-x86_64'))
        self.assertIn('skill/SKILL.md', package.cli_members('linux-x86_64'))
        self.assertIn('skill/release.json', package.cli_members('linux-x86_64'))
        self.assertIn('quazonai.cmd', package.cli_members('windows-x86_64'))
        self.assertIn('app/apps/cli/src/main.ts', package.cli_members('linux-x86_64'))
        self.assertNotIn('runtime.sh', package.DEPLOY_FILES)
        self.assertNotIn('SHA256SUMS', package.REQUIRED_ASSETS)
        self.assertNotIn('quazonai-image-runtime.tar.gz', package.REQUIRED_ASSETS)
        self.assertIn('quazonai-image-application.tar.gz', package.REQUIRED_ASSETS)

    def test_release_readme_documents_docker_and_preserves_cli_clients(self):
        text = package.readme({'version': 'v3.0.0-doc-test', 'revision': 'abc123'})
        for phrase in ('Docker Engine', 'Docker Compose', '/v3.0.0-doc-test/install.sh',
                       'bash install.sh --directory "$HOME/.local/share/quazonai" --port 8081',
                       'client of the deployed HTTP service', 'quazonai login --help',
                       'loopback interface', 'separate Nautilus data/account directory',
                       'retired research workflows'):
            self.assertIn(phrase, text)
        for route in ('macOS (CLI)', 'install.ps1', '--cli-only', 'npm start', 'cargo run'):
            self.assertNotIn(route, text)
        # Only deployment documentation changes; remote client assets remain.
        self.assertIn('windows-x86_64', package.CLI_ARCHIVES)
        self.assertIn('macos-aarch64', package.CLI_ARCHIVES)
        self.assertIn('install.ps1', package.REQUIRED_ASSETS)

    def test_incomplete_cli_is_not_a_distribution(self):
        with tempfile.TemporaryDirectory(dir=os.environ.get('TMPDIR')) as temporary:
            archive = Path(temporary) / 'old.tar.gz'
            with tarfile.open(archive, 'w:gz') as out:
                entry = tarfile.TarInfo('quazonai'); entry.size = 3
                out.addfile(entry, io.BytesIO(b'old'))
            with self.assertRaisesRegex(ValueError, 'Incomplete JS/Node'):
                package.check_cli_archive(archive, 'linux-x86_64')

    def test_standalone_launcher_has_separate_artifacts_without_native_owner(self):
        members = package.cli_members('linux-x86_64', True)
        self.assertIn('quazonai-launcher', members)
        self.assertNotIn('quazonai', members)
        self.assertIn('LAUNCHER.md', members)
        self.assertNotIn('skill/SKILL.md', members)
        self.assertIn('quazonai-launcher-windows-x86_64.zip', package.REQUIRED_ASSETS)
        self.assertNotIn('app/apps/nautilus/src/native_runner.rs', members)
        self.assertEqual(set(package.LAUNCHER_ARCHIVES), set(package.CLI_ARCHIVES))

    def test_standalone_entry_maps_only_existing_protocol_without_executing_an_owner(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / 'app/apps/cli/src').mkdir(parents=True)
            (root / 'app/launch.mjs').write_text((package.ROOT / 'deploy/launcher-launch.mjs').read_text())
            (root / 'distribution.json').write_text('{"version":"v0.0.0-test"}')
            (root / 'app/apps/cli/src/main.ts').write_text('process.stdout.write(JSON.stringify(process.argv.slice(2)));')
            node = shutil.which('node')
            for args, expected in [(['hello', '--file', 'fixture.json'], ['remote-account', 'hello', '--file', 'fixture.json']),
                                   (['inspect', '--file', 'fixture.json'], ['remote-owner', 'inspect', '--file', 'fixture.json']),
                                   (['remote-account', 'relay', '--socket-path', '/fixture'], ['remote-account', 'relay', '--socket-path', '/fixture'])]:
                result = subprocess.run([node, '--experimental-strip-types', str(root / 'app/launch.mjs'), *args], check=True, capture_output=True, text=True)
                self.assertEqual(json.loads(result.stdout), expected)
            denied = subprocess.run([node, '--experimental-strip-types', str(root / 'app/launch.mjs'), 'build'], capture_output=True, text=True)
            self.assertEqual(denied.returncode, 1)
            self.assertIn('launcher_command_invalid', denied.stderr)


class CliEncodingTests(unittest.TestCase):
    def test_original_schema_stdout_preserves_utf8_source(self):
        # Direct protocol assertion against current source, not an archive-wide
        # readiness probe or synthetic CLI. No help/version command sweep.
        node = shutil.which('node')
        self.assertIsNotNone(node, 'CLI schema decoding requires Node on PATH.')
        result = subprocess.run([
            node, '--experimental-strip-types',
            str(package.ROOT / 'apps/cli/src/main.ts'),
            'schema', 'show', 'AssetTarget',
        ], check=True, capture_output=True, encoding='utf-8', errors='strict')
        self.assertIn(
            (package.ROOT / 'packages/contracts/src/pm.ts').read_text(encoding='utf-8'),
            json.loads(result.stdout)['source'],
        )

if __name__ == '__main__': unittest.main()
