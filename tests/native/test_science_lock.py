"""Offline regressions for stale direct dependencies, without a dependency solver."""

import importlib.util
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location("science_lock", ROOT / "tools/lock_science_runtime.py")
assert SPEC is not None and SPEC.loader is not None
LOCK = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(LOCK)
HASH = " --hash=sha256:" + "a" * 64


class ScienceLockTests(unittest.TestCase):
    def check(self, inputs: str, entries: str, roots: str | None = None) -> None:
        if roots is None:
            roots = LOCK.roots_header(LOCK.collect_pins(["some-package==1.0"]))
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            source = base / "requirements.in"
            lock = base / "requirements.lock"
            source.write_text(inputs, encoding="utf-8")
            lock.write_text(roots + "\n" + entries, encoding="utf-8")
            before = (source.read_bytes(), lock.read_bytes())
            with patch.object(LOCK.subprocess, "run", side_effect=AssertionError("must remain offline")):
                LOCK.check_lock(source, lock)
            self.assertEqual(before, (source.read_bytes(), lock.read_bytes()))

    def test_normalizes_pep_names_and_versions_without_writing_or_subprocesses(self) -> None:
        self.check("# comment\n Some_Package == 1.0.0 \n", "some-package==1.0" + HASH)

    def test_version_drift_is_rejected(self) -> None:
        with self.assertRaisesRegex(ValueError, "requirements.in changed"):
            self.check("some-package==2.0", "some-package==1.0" + HASH)

    def test_missing_or_stale_lock_entry_is_rejected_even_with_fresh_header(self) -> None:
        for entries in ("other-package==1.0" + HASH, "some-package==2.0" + HASH):
            with self.subTest(entries=entries), self.assertRaisesRegex(ValueError, "absent or stale"):
                self.check("some-package==1.0", entries)

    def test_removed_direct_dependency_is_not_hidden_by_a_retained_transitive_entry(self) -> None:
        roots = LOCK.roots_header(LOCK.collect_pins(["some-package==1.0", "other==2.0"]))
        with self.assertRaisesRegex(ValueError, "requirements.in changed"):
            self.check("some-package==1.0", "some-package==1.0" + HASH + "\nother==2.0" + HASH, roots)

    def test_added_direct_dependency_is_rejected(self) -> None:
        with self.assertRaisesRegex(ValueError, "requirements.in changed"):
            self.check("some-package==1.0\nother==2.0", "some-package==1.0" + HASH)

    def test_missing_duplicate_or_invalid_header_is_rejected(self) -> None:
        header = LOCK.roots_header(LOCK.collect_pins(["some-package==1.0"]))
        invalid = ["", header + "\n" + header, LOCK.ROOTS_HEADER + "{}", LOCK.ROOTS_HEADER + "[1]",
                   LOCK.ROOTS_HEADER + "[]", LOCK.ROOTS_HEADER + "not-json"]
        for roots in invalid:
            with self.subTest(roots=roots), self.assertRaises(ValueError):
                self.check("some-package==1.0", "some-package==1.0" + HASH, roots)

    def test_duplicate_normalized_names_are_rejected(self) -> None:
        with self.assertRaisesRegex(ValueError, "duplicate distribution"):
            self.check("some-package==1.0\nSome_Package==1.0", "some-package==1.0" + HASH)
        with self.assertRaisesRegex(ValueError, "duplicate locked distribution"):
            self.check("some-package==1.0", "some-package==1.0" + HASH + "\nSome_Package==1.0" + HASH)

    def test_unsupported_input_syntax_is_never_silently_ignored(self) -> None:
        for requirement in ["some-package>=1", "some-package==1.*", "some-package[extra]==1.0",
                            "some-package==1.0; python_version > '3'", "some-package @ https://example.com/a.whl",
                            "-r another.in", "--extra-index-url https://example.com", "some-package===1.0"]:
            with self.subTest(requirement=requirement), self.assertRaises(ValueError):
                self.check(requirement, "some-package==1.0" + HASH)

    def test_unsupported_lock_syntax_is_never_accepted(self) -> None:
        for entry in ["some-package==1.0", "some-package==1.0 --hash=sha256:bad",
                      "some-package==1.0" + HASH + " --extra-index-url https://example.com",
                      "some-package>=1" + HASH, "-r another.lock"]:
            with self.subTest(entry=entry), self.assertRaises(ValueError):
                self.check("some-package==1.0", entry)

    def test_export_records_roots_and_refuses_incomplete_wheels(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            inputs = base / "requirements.in"
            output = base / "new.lock"
            inputs.write_text("some-package==1.0\n", encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "empty"):
                LOCK.export_lock(base, inputs, output)
            (base / "some_package-1.0-py3-none-any.whl").write_bytes(b"test fixture, not an installable wheel")
            result = subprocess.CompletedProcess([], 0, "file.whl:\n" + HASH.strip() + "\n", "")
            with patch.object(LOCK.subprocess, "run", return_value=result):
                LOCK.export_lock(base, inputs, output)
                LOCK.check_lock(inputs, output)
                with self.assertRaises(FileExistsError):
                    LOCK.export_lock(base, inputs, output)
                inputs.write_text("some-package==2.0\n", encoding="utf-8")
                with self.assertRaisesRegex(ValueError, "do not contain"):
                    LOCK.export_lock(base, inputs, base / "must-not-exist.lock")
            self.assertFalse((base / "must-not-exist.lock").exists())

    def test_repository_contract(self) -> None:
        LOCK.check_lock(ROOT / "runtimes/science/requirements.in", ROOT / "runtimes/science/requirements.lock")

    def test_cli_fails_closed_on_stale_input(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            inputs = Path(directory) / "requirements.in"
            inputs.write_text("numpy==0.0.1\n", encoding="utf-8")
            result = subprocess.run(
                [sys.executable, str(ROOT / "tools/lock_science_runtime.py"), "check", str(inputs),
                 str(ROOT / "runtimes/science/requirements.lock")], capture_output=True, text=True, timeout=10,
            )
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("requirements.in changed", result.stderr)
            self.assertNotIn("matches the committed", result.stdout)


if __name__ == "__main__":
    unittest.main()
