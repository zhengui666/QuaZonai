"""Small packaging contracts; extracted official Node execution runs in cli.yml."""
import io
import json
import os
from pathlib import Path
import subprocess
import tarfile
import tempfile
import unittest
import package

class PackageTests(unittest.TestCase):
    def test_node_members_and_retired_runtime(self):
        self.assertIn('runtime/node', package.cli_members())
        self.assertIn('quazonai', package.cli_members())
        self.assertIn('app/apps/cli/src/main.ts', package.cli_members())
        self.assertEqual(package.CLI_ARCHIVES, {'linux-x86_64': 'quazonai-cli-linux-x86_64.tar.gz'})
        self.assertEqual(package.REQUIRED_ASSETS, {
            'install.sh', 'release.json', 'README.md', 'quazonai-deploy.tar.gz',
            'quazonai-cli-linux-x86_64.tar.gz', 'quazonai-image-application.tar.gz',
            'quazonai-image-runtime.tar.gz', 'quazonai-image-database.tar.gz',
        })
        self.assertNotIn('runtime.sh', package.DEPLOY_FILES)
        self.assertNotIn('SHA256SUMS', package.REQUIRED_ASSETS)

    def test_latest_installer_selects_complete_linux_assets(self):
        source = (package.ROOT / 'deploy/install.sh').read_text()
        parser = 'json_read() {' + source.split('json_read() {', 1)[1].split('\nfetch() {', 1)[0]
        version = 'v3.0.0-dev.20261013103500.1'
        assets = [{'name': name, 'state': 'uploaded', 'size': 1} for name in package.REQUIRED_ASSETS]
        catalog = [
            {'tag_name': version, 'draft': False, 'assets': assets},
            {'tag_name': 'v3.0.0-dev.20261013103600.1', 'draft': False,
             'assets': [asset for asset in assets if asset['name'] != 'quazonai-image-runtime.tar.gz']},
        ]
        with tempfile.TemporaryDirectory(dir=os.environ.get('TMPDIR')) as temporary:
            path = Path(temporary) / 'catalog.json'
            path.write_text(json.dumps(catalog))
            result = subprocess.run(['bash', '-c', parser + '\njson_read latest "$1"',
                                     'installer-catalog', str(path)], check=True, capture_output=True, text=True)
        self.assertEqual(result.stdout, f'2 {version}')

    def test_incomplete_cli_is_not_a_distribution(self):
        with tempfile.TemporaryDirectory(dir=os.environ.get('TMPDIR')) as temporary:
            archive = Path(temporary) / 'old.tar.gz'
            with tarfile.open(archive, 'w:gz') as out:
                entry = tarfile.TarInfo('quazonai'); entry.size = 3
                out.addfile(entry, io.BytesIO(b'old'))
            with self.assertRaisesRegex(ValueError, 'Incomplete JS/Node'):
                package.check_cli_archive(archive)

if __name__ == '__main__': unittest.main()
