"""Exercise the real source projection/digest without Cargo, Docker or downloads."""
import json
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest


SOURCE = Path(__file__).resolve().parents[2]


class NativeInputsTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.temporary = tempfile.TemporaryDirectory(prefix='quazonai-native-input-test-')
        cls.root = Path(cls.temporary.name) / 'source with spaces'
        shutil.copytree(SOURCE, cls.root, ignore=shutil.ignore_patterns('.git', 'target', 'node_modules', '__pycache__'))

    @classmethod
    def tearDownClass(cls):
        cls.temporary.cleanup()

    def invoke(self, *args, check=True):
        return subprocess.run(['node', str(SOURCE / 'deploy/docker/native-inputs.mjs'), *map(str, args)],
                              check=check, text=True, capture_output=True)

    def identity(self):
        return json.loads(self.invoke('identity', self.root).stdout)

    def test_native_inputs_invalidate_but_packaging_edits_do_not(self):
        original = self.identity()
        native = ['apps/server/src/main.rs', 'apps/job/src/bin/catalog-prepare.rs',
                  'apps/runtime/src/journal.rs', 'crates/store/build.rs', 'Cargo.toml',
                  'Cargo.lock', 'rust-toolchain.toml',
                  'crates/store/src/historical_projection_0029.json',
                  'crates/store/src/session_schema.sql',
                  'deploy/docker/native-build.sh', 'deploy/docker/native-inputs.mjs']
        native += [str(next((self.root / 'migrations').glob('*.sql')).relative_to(self.root)),
                   str(next((self.root / 'apps/runtime/migrations').glob('*.sql')).relative_to(self.root))]
        unrelated = ['README.md', 'LICENSE', 'NOTICE', 'THIRD_PARTY_NOTICES.md',
                     'apps/web/src/main.tsx', 'runtimes/data/source_plugins.py',
                     '.github/actions/container/action.yml']
        for name in native + unrelated:
            with self.subTest(path=name):
                file = self.root / name
                before = file.read_bytes()
                try:
                    file.write_bytes(before + b'\n')
                    changed = self.identity()
                    if name in native:
                        self.assertNotEqual(changed['input_sha256'], original['input_sha256'])
                    else:
                        self.assertEqual(changed, original)
                finally:
                    file.write_bytes(before)

    def test_new_native_assets_and_removed_inputs_change_identity(self):
        original = self.identity()
        file = self.root / 'apps/job/src/new-compile-time-asset.json'
        try:
            file.write_text('{"new":true}')
            changed = self.identity()
            self.assertNotEqual(changed['input_sha256'], original['input_sha256'])
            self.assertIn(str(file.relative_to(self.root)), [entry['path'] for entry in changed['entries']])
        finally:
            file.unlink(missing_ok=True)
        self.assertEqual(self.identity(), original)

    def test_native_recipe_changes_but_final_revision_labels_do_not(self):
        file = self.root / 'deploy/docker/Dockerfile'
        before = file.read_text()
        original = self.identity()
        try:
            file.write_text(before.replace('CARGO_BUILD_JOBS=2', 'CARGO_BUILD_JOBS=3'))
            self.assertNotEqual(self.identity()['recipe_sha256'], original['recipe_sha256'])
            file.write_text(before.replace('ARG REVISION=unknown', 'ARG REVISION=another-packaging-head'))
            self.assertEqual(self.identity(), original)
        finally:
            file.write_text(before)

    def test_unsupported_workspace_config_paths_and_dynamic_includes_fail(self):
        edits = [
            ('Cargo.toml', lambda s: s.replace('resolver = "2"', 'resolver = "2"\nexclude = ["apps/elsewhere"]')),
            ('Cargo.toml', lambda s: s.replace('resolver = "2"', 'resolver = "2"\ndefault-members = ["apps/server"]')),
            ('Cargo.toml', lambda s: s.replace('"apps/cli",', '"apps/new-member", "apps/cli",')),
            ('apps/job/Cargo.toml', lambda s: s + '\n[dependencies.outside]\npath = "../../runtimes/data"\n'),
            ('apps/job/src/main.rs', lambda s: s + '\nconst X: &str = include_str!("../../../README.md");\n'),
            ('apps/job/src/main.rs', lambda s: s + '\nconst X: &str = include_str!(concat!("x", "y"));\n'),
            ('.dockerignore', lambda s: s + '\ncrates\n'),
            ('deploy/docker/Dockerfile.dockerignore', lambda s: s + '\ncrates/**\n'),
        ]
        for name, change in edits:
            with self.subTest(path=name):
                file = self.root / name
                before = file.read_text()
                try:
                    file.write_text(change(before))
                    self.assertNotEqual(self.invoke('identity', self.root, check=False).returncode, 0)
                finally:
                    file.write_text(before)
        for parent in ('', 'apps/job'):
            config = self.root / parent / '.cargo'
            try:
                config.mkdir()
                (config / 'config.toml').write_text('[build]\nrustflags=["--cfg=changed"]')
                self.assertNotEqual(self.invoke('identity', self.root, check=False).returncode, 0)
            finally:
                shutil.rmtree(config)
        self.assertNotEqual(self.invoke('identity', self.root, 'linux/arm64', check=False).returncode, 0)

    def test_required_missing_input_fails(self):
        file = self.root / 'Cargo.lock'
        before = file.read_bytes()
        try:
            file.unlink()
            self.assertNotEqual(self.invoke('identity', self.root, check=False).returncode, 0)
        finally:
            file.write_bytes(before)

    def test_context_control_files_are_admitted_and_required(self):
        rules = (self.root / 'deploy/docker/Dockerfile.dockerignore').read_text().splitlines()
        for name in ('.dockerignore', 'deploy/docker/Dockerfile.dockerignore'):
            self.assertIn('!' + name, rules)
            file = self.root / name
            before = file.read_bytes()
            try:
                file.unlink()
                self.assertNotEqual(self.invoke('prepare', self.root, self.root.parent / 'missing-control', check=False).returncode, 0)
            finally:
                file.write_bytes(before)

    def test_symlink_input_and_ancestor_fail(self):
        for name in ('apps/job/src/main.rs', 'apps'):
            file = self.root / name
            saved = file.with_name(file.name + '.saved')
            file.rename(saved)
            try:
                file.symlink_to(saved)
                self.assertNotEqual(self.invoke('identity', self.root, check=False).returncode, 0)
            finally:
                file.unlink()
                saved.rename(file)

    def test_projection_is_complete_stable_and_has_no_packaging_source(self):
        with tempfile.TemporaryDirectory(prefix='quazonai-native-output-test-') as temporary:
            output = Path(temporary) / 'native output with spaces'
            manifest = json.loads(self.invoke('prepare', self.root, output).stdout)
            self.assertEqual(manifest, self.identity())
            for entry in manifest['entries']:
                self.assertEqual((output / entry['path']).read_bytes(), (self.root / entry['path']).read_bytes())
                self.assertEqual((output / entry['path']).stat().st_mode & 0o777, entry['mode'])
            self.assertEqual((output / '.native-input.sha256').read_text().strip(), manifest['input_sha256'])
            self.assertFalse((output / 'README.md').exists())
            self.assertFalse((output / 'apps/web').exists())
            self.assertFalse((output / 'runtimes/data').exists())
            # Existing destinations are rejected instead of retaining deleted files.
            self.assertNotEqual(self.invoke('prepare', self.root, output, check=False).returncode, 0)


if __name__ == '__main__':
    unittest.main()
