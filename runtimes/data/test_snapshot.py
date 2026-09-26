"""Offline regression checks: python3 -m unittest discover -s runtimes/data -v."""

import contextlib
import hashlib
import io
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import snapshot


COMMIT = "a" * 40
DATASET = "example/history"
API = f"{snapshot.HUB}/api/datasets/{DATASET}"
FIXED_API = f"{API}/revision/{COMMIT}?blobs=true"
BASE = f"{snapshot.HUB}/datasets/{DATASET}/resolve/{COMMIT}/"


class SnapshotTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.output = Path(self.temp.name) / "output"
        self.content = {"README.md": b"source terms", "SNAPSHOT.json": b"{}",
                        "data/2022.parquet": b"real source bytes", "data/2023.parquet": b"other year"}
        self.metadata = {"sha": COMMIT, "private": False, "gated": False,
                         "cardData": {"license": "cc-by-4.0"}, "siblings": []}
        for path, body in self.content.items():
            item = {"rfilename": path, "size": len(body),
                    "blobId": hashlib.sha1(f"blob {len(body)}\0".encode() + body).hexdigest()}
            if path.endswith("parquet"):
                item["lfs"] = {"size": len(body), "sha256": hashlib.sha256(body).hexdigest()}
            self.metadata["siblings"].append(item)
        self.http = patch.object(snapshot.urllib.request, "urlopen", side_effect=self.respond).start()
        self.addCleanup(patch.stopall)

    def respond(self, url, timeout):
        self.assertEqual(timeout, 60)
        if url == FIXED_API:
            return io.BytesIO(json.dumps(self.metadata).encode())
        if url.startswith(API):
            return io.BytesIO(json.dumps({"sha": COMMIT}).encode())
        if url.startswith(BASE):
            return io.BytesIO(self.content[url.removeprefix(BASE)])
        self.fail("unexpected request")

    def plan(self, **kwargs):
        return snapshot.plan(DATASET, ["data/2022.parquet"], **kwargs)

    def test_plan_pins_revision_includes_evidence_and_downloads_no_files(self):
        result = self.plan(revision="branch/date")
        self.assertEqual(self.http.call_args_list[0].args[0], API + "/revision/branch%2Fdate")
        self.assertEqual(self.http.call_args_list[1].args[0], FIXED_API)
        self.assertEqual(self.http.call_count, 2)
        self.assertEqual(result["revision"], COMMIT)
        self.assertEqual([item["path"] for item in result["files"]],
                         ["README.md", "SNAPSHOT.json", "data/2022.parquet"])
        self.assertEqual(result["total_bytes"], 31)
        self.assertEqual(result["license_reference"], BASE + "README.md")
        self.assertFalse(self.output.exists())

    def test_selection_and_metadata_fail_closed(self):
        for includes in ([], ["missing*"], ["../*"], ["/data/*"], ["data\\*"]):
            with self.subTest(includes=includes), self.assertRaises(ValueError):
                snapshot.plan(DATASET, includes)
        for change in ({"sha": "b" * 40}, {"gated": "auto"},
                       {"siblings": [{"rfilename": "../outside"}]},
                       {"siblings": [{"rfilename": "data/2022.parquet", "size": True}]}):
            with self.subTest(change=change), patch.dict(self.metadata, change), self.assertRaises(ValueError):
                self.plan()
        with self.assertRaisesRegex(ValueError, "requested commit"):
            self.plan(revision="b" * 40)

    def test_budget_is_checked_before_any_file_download(self):
        with self.assertRaisesRegex(ValueError, "exceeding"):
            self.plan(max_bytes=30)
        self.assertEqual(self.http.call_count, 2)
        self.assertFalse(self.output.exists())

    def test_download_and_completed_reuse_validate_every_hash(self):
        selection = self.plan()
        manifest = snapshot.download(selection, self.output)
        self.assertEqual(set(manifest), {"schema_version", "repository", "revision", "license",
                                        "license_reference", "retrieved_at", "files"})
        self.assertEqual(manifest["schema_version"], 1)
        self.assertTrue(manifest["retrieved_at"].endswith("Z"))
        for item in manifest["files"]:
            self.assertEqual(set(item), {"path", "size", "sha256", "url"})
            self.assertEqual(item["sha256"], hashlib.sha256(self.content[item["path"]]).hexdigest())
        saved = (self.output / "snapshot.json").read_bytes()
        self.http.reset_mock()
        self.assertEqual(snapshot.download(selection, self.output), manifest)
        self.http.assert_not_called()
        target = self.output / "data/2022.parquet"
        target.write_bytes(b"x" * target.stat().st_size)
        with self.assertRaisesRegex(ValueError, "SHA-256 mismatch"):
            snapshot.download(selection, self.output)
        self.assertEqual((self.output / "snapshot.json").read_bytes(), saved)

    def test_interruption_leaves_no_manifest_and_reuses_verified_complete_files(self):
        selection = snapshot.plan(DATASET, ["data/*"])
        original = self.respond

        class InterruptedStream(io.BytesIO):
            def read(self, size):
                if self.tell():
                    raise OSError("connection lost during transfer")
                return super().read(3)

        def interrupted(url, timeout):
            if url == BASE + "data/2023.parquet":
                return InterruptedStream(self.content["data/2023.parquet"])
            return original(url, timeout)

        self.http.side_effect = interrupted
        with self.assertRaises(OSError):
            snapshot.download(selection, self.output)
        self.assertFalse((self.output / "snapshot.json").exists())
        self.assertFalse(list(self.output.rglob("*.partial")))
        self.assertTrue((self.output / "data/2022.parquet").is_file())
        self.http.reset_mock()
        self.http.side_effect = original
        snapshot.download(selection, self.output)
        self.assertNotIn(BASE + "data/2022.parquet", [call.args[0] for call in self.http.call_args_list])
        self.assertTrue((self.output / "snapshot.json").is_file())

    def test_modified_manifest_cannot_authorize_corrupt_regular_git_file(self):
        selection = self.plan()
        manifest = snapshot.download(selection, self.output)
        readme = self.output / "README.md"
        corrupt = b"x" * readme.stat().st_size
        readme.write_bytes(corrupt)
        manifest["files"][0]["sha256"] = hashlib.sha256(corrupt).hexdigest()
        (self.output / "snapshot.json").write_text(json.dumps(manifest))
        with self.assertRaisesRegex(ValueError, "Git blob mismatch"):
            snapshot.download(selection, self.output)

    def test_integrity_failures_never_publish_manifest_or_corrupt_file(self):
        selection = self.plan()
        for body in (b"x" * 17, b"short", b"too many bytes for this file"):
            with self.subTest(body=body), patch.dict(self.content, {"data/2022.parquet": body}):
                with self.assertRaises(ValueError):
                    snapshot.download(selection, self.output)
                self.assertFalse((self.output / "data/2022.parquet").exists())
                self.assertFalse((self.output / "snapshot.json").exists())
                self.assertFalse(list(self.output.rglob("*.partial")))

    def test_existing_conflicts_and_symlinks_are_never_overwritten(self):
        selection = self.plan()
        self.output.mkdir()
        readme = self.output / "README.md"
        readme.write_bytes(b"private file")
        with self.assertRaisesRegex(ValueError, "SHA-256 mismatch"):
            snapshot.download(selection, self.output)
        self.assertEqual(readme.read_bytes(), b"private file")
        readme.unlink()
        outside = Path(self.temp.name) / "outside"
        outside.write_bytes(b"outside")
        readme.symlink_to(outside)
        with self.assertRaisesRegex(ValueError, "symlinks"):
            snapshot.download(selection, self.output)
        self.assertEqual(outside.read_bytes(), b"outside")
        self.assertFalse((self.output / "snapshot.json").exists())

    def test_manifest_conflict_and_missing_license_fail_without_modification(self):
        with patch.dict(self.metadata, {"cardData": {}}):
            selection = self.plan()
            with self.assertRaisesRegex(ValueError, "no license"):
                snapshot.download(selection, self.output)
            self.assertFalse(self.output.exists())
            self.assertEqual(self.plan(license="verified-custom-terms")["license"], "verified-custom-terms")
        selection = self.plan()
        snapshot.download(selection, self.output)
        saved = (self.output / "snapshot.json").read_bytes()
        with self.assertRaisesRegex(ValueError, "conflicts"):
            snapshot.download(self.plan(revision="main") | {"revision": "b" * 40}, self.output)
        self.assertEqual((self.output / "snapshot.json").read_bytes(), saved)

    def test_cli_does_not_echo_sensitive_exception_text(self):
        self.http.side_effect = OSError("https://redirect.invalid/?secret=hidden")
        stderr = io.StringIO()
        with contextlib.redirect_stderr(stderr):
            status = snapshot.main(["plan", "--dataset", DATASET, "--include", "data/*"])
        self.assertEqual(status, 1)
        self.assertNotIn("hidden", stderr.getvalue())


if __name__ == "__main__":
    unittest.main()
