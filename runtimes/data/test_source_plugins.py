"""Local source dispatch tests; converter stubs do not establish scientific admission."""

import contextlib
from decimal import Decimal
import io
import json
import os
from pathlib import Path
from types import SimpleNamespace
import tempfile
import unittest
from unittest.mock import patch

import acquire
import cryptohftdata
import providers
import source_plugins as plugins


class RegistryTest(unittest.TestCase):
    def test_only_hyperliquid_lifecycle_is_exposed(self):
        with contextlib.redirect_stdout(io.StringIO()) as stdout:
            self.assertEqual(plugins.main(["plugins"]), 0)
        result = json.loads(stdout.getvalue())
        self.assertEqual([item["id"] for item in result], [cryptohftdata.PLUGIN])
        self.assertEqual(result[0]["capabilities"], ["convert", "inspect", "plan"])
        self.assertEqual(result[0]["public_network_operations"], [])
        self.assertEqual(result[0]["authentication"], "NONE")
        self.assertFalse(result[0]["admission"]["research_qualified"])
        self.assertFalse(result[0]["admission"]["registered_in_quazonai"])

    def test_removed_lifecycle_operations_are_not_accepted(self):
        for operation in ("freeze", "verify", "prepare", "export-dataset"):
            with self.subTest(operation=operation), contextlib.redirect_stderr(io.StringIO()), \
                    self.assertRaises(SystemExit) as error:
                plugins.parser().parse_args([operation, cryptohftdata.PLUGIN])
            self.assertEqual(error.exception.code, 2)

    def test_original_json_decimals_and_duplicate_fields_are_preserved(self):
        self.assertEqual(providers.read_json(b'{"quantity":123456789.123456789}')['quantity'],
                         Decimal('123456789.123456789'))
        for value in (b'{"quantity":1,"quantity":2}', b'{"quantity":NaN}', b'{"quantity":Infinity}'):
            with self.subTest(value=value), self.assertRaises(ValueError):
                providers.read_json(value)

    def test_packaged_binary_is_used_without_path_fallback(self):
        with tempfile.TemporaryDirectory() as directory:
            args = SimpleNamespace(native_bin=None, output=Path(directory) / 'new-output')
            with patch.object(plugins.os, 'access', return_value=True), \
                    patch.object(Path, 'is_file', return_value=True), \
                    patch.object(plugins.subprocess, 'run', return_value=SimpleNamespace(returncode=0)) as run:
                self.assertEqual(plugins.run_native(args, ['import', '--native-precision'], 'market-history'),
                                 args.output)
            argv = run.call_args.args[0]
            self.assertEqual(argv, [str(Path(plugins.__file__).parent / 'bin' / 'market-history'),
                'import', '--native-precision', '--output', str(args.output)])
            self.assertFalse(run.call_args.kwargs['shell'])

    def test_missing_packaged_binary_and_installed_override_never_launch(self):
        with tempfile.TemporaryDirectory() as directory:
            args = SimpleNamespace(native_bin=None, output=Path(directory) / 'new-output')
            with patch.object(Path, 'is_file', return_value=False), \
                    patch.object(plugins.subprocess, 'run') as run, self.assertRaises(ValueError):
                plugins.run_native(args, ['import'], 'market-history')
            run.assert_not_called()
            args.native_bin = Path(directory) / 'replacement'
            with patch.dict(os.environ, {'QZ_OPERATOR_INSTALLED': '1'}), \
                    patch.object(plugins.subprocess, 'run') as run, self.assertRaises(ValueError):
                plugins.run_native(args, ['import'], 'market-history')
            run.assert_not_called()

    def test_original_local_bytes_are_read_and_source_symlinks_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            path = root / 'source.json'
            path.write_bytes(b'original source bytes')
            self.assertEqual(acquire.local_bytes(path), b'original source bytes')
            link = root / 'linked.json'
            link.symlink_to(path)
            with self.assertRaises(ValueError):
                acquire.local_bytes(link)


if __name__ == '__main__':
    unittest.main()
