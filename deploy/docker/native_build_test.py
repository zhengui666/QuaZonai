"""Check the producer command boundary without compiling or installing binaries."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


RECIPE = Path(__file__).with_name('native-build.sh')


class NativeBuildTests(unittest.TestCase):
    def invoke(self, fail_command=''):
        with tempfile.TemporaryDirectory(prefix='quazonai-native-build-test-') as temporary:
            root = Path(temporary)
            log = root / 'commands.jsonl'
            # All commands in the server branch are intercepted; /out is never
            # touched. JSON preserves the shell's actual argument boundaries.
            stub = f'#!{sys.executable}\n' + '''import json, os, sys
from pathlib import Path
name = Path(sys.argv[0]).name
with open(os.environ['QZ_TEST_LOG'], 'a') as output:
    output.write(json.dumps([name, *sys.argv[1:]]) + '\\n')
sys.exit(37 if name == os.environ['QZ_TEST_FAIL'] else 0)
'''
            for command in ('cargo', 'install', 'strip'):
                path = root / command
                path.write_text(stub)
                path.chmod(0o755)
            result = subprocess.run(
                ['/bin/sh', str(RECIPE), 'server'], cwd=root,
                env={**os.environ, 'PATH': str(root), 'QZ_TEST_LOG': str(log),
                     'QZ_TEST_FAIL': fail_command}, text=True, capture_output=True)
            calls = [json.loads(line) for line in log.read_text().splitlines()]
            return result, calls

    def test_combined_build_scopes_strip_and_preserves_installation(self):
        result, calls = self.invoke()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(calls, [
            ['cargo', 'build', '--locked', '--release', '-p', 'server', '-p', 'runtime',
             '--config', 'profile.release.package.server.strip="symbols"',
             '--config', 'profile.release.package.runtime.strip="symbols"'],
            ['install', '-Dm755', 'target/release/server', '/out/server'],
            ['install', '-Dm755', 'target/release/runtime', '/out/runtime'],
            ['strip', '/out/server', '/out/runtime'],
        ])

    def test_failed_build_cannot_install_stale_outputs(self):
        result, calls = self.invoke('cargo')
        self.assertEqual(result.returncode, 37)
        self.assertEqual([call[0] for call in calls], ['cargo'])


if __name__ == '__main__':
    unittest.main()
