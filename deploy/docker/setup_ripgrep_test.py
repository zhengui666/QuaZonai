"""Offline setup failures must never publish an unverified executable."""
from contextlib import redirect_stdout
import hashlib
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tarfile
import tempfile
import unittest
from unittest.mock import patch

import setup_ripgrep as setup


class RipgrepSetupTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.runner = self.root / "runner"
        self.runner.mkdir()
        self.github_path = self.root / "github-path"
        self.github_path.write_text("/previous/tool\n")
        self.payload = self.root / "payload.tar.gz"
        self.marker = self.root / "executed-existing-rg"
        self.curl_args = self.root / "curl-args"
        self.tools = self.root / "tools"
        self.tools.mkdir()
        self.script(self.tools / "rg", f"from pathlib import Path\nPath({str(self.marker)!r}).touch()\n")
        self.script(self.tools / "curl", f"""import json, os, pathlib, shutil, sys, time
if '--version' in sys.argv:
    print(os.environ.get('TEST_CURL_VERSION', 'curl 8.4.0'))
    sys.exit(0)
pathlib.Path({str(self.curl_args)!r}).write_text(json.dumps(sys.argv[1:]))
if os.environ.get('TEST_CURL_SLEEP'):
    pathlib.Path({str(self.root / 'curl-pid')!r}).write_text(str(os.getpid()))
    time.sleep(30)
shutil.copyfile({str(self.payload)!r}, sys.argv[sys.argv.index('--output') + 1])
sys.exit(int(os.environ.get('TEST_CURL_EXIT', '0')))
""")
        self.addCleanup(patch.stopall)
        patch.dict(os.environ, {"PATH": str(self.tools) + os.pathsep + os.environ["PATH"]}).start()
        patch.object(setup.platform, "system", return_value="Linux").start()
        patch.object(setup.platform, "machine", return_value="x86_64").start()
        self.stdout = io.StringIO()
        self.output = redirect_stdout(self.stdout)
        self.output.__enter__()
        self.addCleanup(self.output.__exit__, None, None, None)

    def script(self, path, body):
        path.write_text(f"#!{sys.executable}\n" + body)
        path.chmod(0o755)

    def archive(self, members=None, version="14.1.1"):
        data = f"#!/bin/sh\nprintf 'ripgrep {version} (rev 4649aa9d)\\n'\n".encode()
        if members is None:
            members = [(f"{setup.RELEASE}/rg", tarfile.REGTYPE, data)]
        with tarfile.open(self.payload, "w:gz") as archive:
            for name, kind, contents in members:
                member = tarfile.TarInfo(name)
                member.type = kind
                member.mode = 0o7777
                member.linkname = str(self.marker)
                member.size = len(contents) if kind == tarfile.REGTYPE else 0
                archive.addfile(member, io.BytesIO(contents))
        patch.object(setup, "SHA256", hashlib.sha256(self.payload.read_bytes()).hexdigest()).start()

    def install(self):
        return setup.install(self.runner, self.github_path)

    def assert_unpublished(self):
        self.assertEqual(self.github_path.read_text(), "/previous/tool\n")
        self.assertEqual(list(self.runner.iterdir()), [])
        self.assertFalse(self.marker.exists())

    def test_success_uses_owned_output_and_ignores_existing_path_binary(self):
        self.archive()
        previous_rg = (self.tools / "rg").read_bytes()
        first = self.install()
        second = self.install()
        self.assertNotEqual(first, second)
        self.assertEqual(first.parent, self.runner)
        self.assertEqual(first.stat().st_uid, os.getuid())
        self.assertEqual(first.stat().st_mode & 0o777, 0o700)
        self.assertEqual((first / "rg").stat().st_mode & 0o7777, 0o755)
        self.assertEqual([path.name for path in first.iterdir()], ["rg"])
        self.assertEqual(self.github_path.read_text(), f"/previous/tool\n{first}\n{second}\n")
        self.assertEqual((self.tools / "rg").read_bytes(), previous_rg)
        self.assertFalse(self.marker.exists())
        args = json.loads(self.curl_args.read_text())
        self.assertEqual(args[0], "--disable")
        for flag, value in (("--proto", "=https"), ("--proto-redir", "=https"),
                            ("--max-redirs", "3"), ("--connect-timeout", "10"),
                            ("--max-time", "60"), ("--retry", "0"),
                            ("--max-filesize", str(setup.MAX_ARCHIVE_BYTES))):
            self.assertEqual(args[args.index(flag) + 1], value)
        self.assertIn("--fail", args)
        self.assertEqual(args[-1], setup.URL)

    def test_hash_mismatch_is_rejected_before_archive_parsing(self):
        self.archive()
        with patch.object(setup, "SHA256", "0" * 64), patch.object(setup.tarfile, "open") as open_tar:
            with self.assertRaisesRegex(ValueError, "SHA256 mismatch"):
                self.install()
            open_tar.assert_not_called()
        self.assert_unpublished()

    def test_missing_traversal_and_duplicate_members_are_rejected(self):
        expected = f"{setup.RELEASE}/rg"
        for members in ([], [("../rg", tarfile.REGTYPE, b"no")],
                        [("/rg", tarfile.REGTYPE, b"no")],
                        [(expected, tarfile.REGTYPE, b"one"), (expected, tarfile.REGTYPE, b"two")]):
            with self.subTest(members=members):
                self.archive(members)
                with self.assertRaisesRegex(ValueError, "exactly one"):
                    self.install()
                self.assert_unpublished()

    def test_links_directories_devices_and_empty_members_are_rejected(self):
        for kind in (tarfile.SYMTYPE, tarfile.LNKTYPE, tarfile.DIRTYPE,
                     tarfile.CHRTYPE, tarfile.FIFOTYPE, tarfile.REGTYPE):
            with self.subTest(kind=kind):
                self.archive([(f"{setup.RELEASE}/rg", kind, b"")])
                with self.assertRaisesRegex(ValueError, "bounded regular file"):
                    self.install()
                self.assert_unpublished()

    def test_archive_paths_and_permissions_are_not_extracted(self):
        contents = b"#!/bin/sh\nprintf 'ripgrep 14.1.1\\n'\n"
        self.archive([(str(self.marker), tarfile.REGTYPE, b"unwanted"),
                      ("../../escaped", tarfile.REGTYPE, b"unwanted"),
                      (f"{setup.RELEASE}/rg", tarfile.REGTYPE, contents)])
        owned = self.install()
        self.assertEqual([path.name for path in owned.iterdir()], ["rg"])
        self.assertFalse(self.marker.exists())
        self.assertFalse((self.root / "escaped").exists())

    def test_archive_and_binary_size_budgets_are_enforced(self):
        self.archive()
        for constant, message in (("MAX_ARCHIVE_BYTES", "download byte budget"),
                                  ("MAX_BINARY_BYTES", "bounded regular file")):
            with self.subTest(constant=constant), patch.object(setup, constant, 1):
                with self.assertRaisesRegex(ValueError, message):
                    self.install()
                self.assert_unpublished()

    def test_curl_failures_never_fall_back_or_publish_partial_files(self):
        self.archive()
        # curl: DNS failure, HTTP failure, timeout, certificate failure, byte cap.
        for status in (6, 22, 28, 60, 63):
            with self.subTest(status=status), patch.dict(os.environ, {"TEST_CURL_EXIT": str(status)}):
                with self.assertRaises(subprocess.CalledProcessError) as result:
                    self.install()
                self.assertEqual(result.exception.returncode, status)
                self.assert_unpublished()

    def test_parent_timeout_kills_and_reaps_stalled_download(self):
        self.archive()
        with patch.dict(os.environ, {"TEST_CURL_SLEEP": "1"}), \
                patch.object(setup, "DOWNLOAD_TIMEOUT_SECONDS", 0.25):
            with self.assertRaises(subprocess.TimeoutExpired):
                self.install()
        pid = int((self.root / "curl-pid").read_text())
        with self.assertRaises(ProcessLookupError):
            os.kill(pid, 0)
        self.assert_unpublished()

    def test_old_or_unrecognized_curl_fails_before_download(self):
        self.archive()
        for version in ("curl 8.3.0", "curl 7.88.1", "unknown"):
            with self.subTest(version=version), patch.dict(os.environ, {"TEST_CURL_VERSION": version}):
                with self.assertRaisesRegex(ValueError, "curl >= 8.4.0"):
                    self.install()
                self.assertFalse(self.curl_args.exists())
                self.assert_unpublished()

    def test_wrong_binary_version_is_not_published(self):
        self.archive(version="14.1.10")
        with self.assertRaisesRegex(ValueError, "unexpected version"):
            self.install()
        self.assert_unpublished()

    def test_version_failure_and_timeout_are_not_published(self):
        for contents, exception in ((b"#!/bin/sh\nexit 7\n", subprocess.CalledProcessError),
                                    (f"#!{sys.executable}\nimport time\ntime.sleep(30)\n".encode(),
                                     subprocess.TimeoutExpired)):
            with self.subTest(exception=exception), patch.object(setup, "VERSION_TIMEOUT_SECONDS", 0.25):
                self.archive([(f"{setup.RELEASE}/rg", tarfile.REGTYPE, contents)])
                with self.assertRaises(exception):
                    self.install()
                self.assert_unpublished()

    def test_unsupported_host_and_unsafe_path_fail_before_download(self):
        with patch.object(setup.platform, "machine", return_value="aarch64"):
            with self.assertRaisesRegex(ValueError, "Linux x86_64"):
                self.install()
        unsafe_root = self.root / "unsafe\npath"
        unsafe_root.mkdir()
        with self.assertRaisesRegex(ValueError, "safe for GITHUB_PATH"):
            setup.install(unsafe_root, self.github_path)
        self.assertEqual(list(unsafe_root.iterdir()), [])
        self.assertFalse(self.curl_args.exists())
        self.assert_unpublished()

    def test_path_publication_failure_cleans_only_new_owned_directory(self):
        self.archive()
        existing = self.runner / "quazonai-ripgrep-existing"
        existing.mkdir()
        (existing / "rg").write_text("preserve")
        with self.assertRaises(IsADirectoryError):
            setup.install(self.runner, self.root)
        self.assertEqual(list(self.runner.iterdir()), [existing])
        self.assertEqual((existing / "rg").read_text(), "preserve")
        self.assertEqual(self.github_path.read_text(), "/previous/tool\n")

    def test_container_calls_setup_after_existing_tests(self):
        action = (Path(__file__).resolve().parents[2] / ".github/actions/container/action.yml").read_text()
        prerequisite = action.split("    - name: Install the pinned scientific Rust toolchain", 1)[0]
        self.assertIn("python3 -B deploy/docker/setup_ripgrep.py", prerequisite)
        self.assertNotIn("apt-get", prerequisite)
        self.assertLess(prerequisite.index("RELEASE_BRANCH=dev python3 -B -m unittest"),
                        prerequisite.index("python3 -B deploy/docker/setup_ripgrep.py"))


if __name__ == "__main__":
    unittest.main()
