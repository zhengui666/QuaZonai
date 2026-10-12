"""Small packaging contracts; extracted official Node execution runs in cli.yml."""
import io
import os
from pathlib import Path
import tarfile
import tempfile
import unittest
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

if __name__ == '__main__': unittest.main()
