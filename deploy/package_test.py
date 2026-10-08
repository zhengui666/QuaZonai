"""Portable artifact integrity checks; native execution runs in the CLI matrix."""
import io
import json
from pathlib import Path
import subprocess
import tarfile
import tempfile
import unittest
from unittest.mock import patch
import zipfile

import package

VERSION = "v2.0.0-dev.20260928123456.123"
REVISION = "a" * 40


def tar(path, files):
    with tarfile.open(path, "w:gz") as archive:
        for name, data in files.items():
            entry = tarfile.TarInfo(name)
            entry.size = len(data)
            archive.addfile(entry, io.BytesIO(data))


def checksums(root):
    (root / "SHA256SUMS").write_text("".join(
        f"{package.sha256(root / name)}  {name}\n"
        for name in sorted(package.REQUIRED_ASSETS - {"SHA256SUMS"})), encoding="ascii")


class PackageTests(unittest.TestCase):
    def test_readme_bootstrap_downloads_fully_before_execution(self):
        command = next(line for line in (package.ROOT / "README.md").read_text(encoding="utf-8").splitlines()
                       if line.startswith("sh -c '") and "curl" in line)
        self.assertNotIn("python", command)
        self.assertNotIn("| bash", command)
        self.assertIn('&& bash "$f" "$@"', command)
        self.assertIn("trap cleanup 0", command)
        subprocess.run(["sh", "-n", "-c", command], check=True)

    def test_shell_bundle_file_contract(self):
        files = {name: b"fixture\n" for name in package.DEPLOY_FILES}
        files["release.json"] = json.dumps(self.manifest).encode()
        files["README.md"] = VERSION.encode()
        archive = self.root / "bundle.tar.gz"
        tar(archive, files)
        package.check_tar(archive, manifest=self.manifest)
        for extra in ("manage.py", "codex.py", "unexpected.sh"):
            tar(archive, {**files, extra: b"unexpected\n"})
            with self.assertRaisesRegex(ValueError, "exact shell bundle"):
                package.check_tar(archive, manifest=self.manifest)
        for missing in package.DEPLOY_FILES:
            tar(archive, {name: data for name, data in files.items() if name != missing})
            with self.assertRaisesRegex(ValueError, "exact shell bundle"):
                package.check_tar(archive, manifest=self.manifest)

    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.assets = self.root / "assets"
        self.assets.mkdir()
        self.binary = self.root / "quazonai"
        self.binary.write_bytes(b"native-binary-fixture")
        self.manifest = {"schema_version": 2, "version": VERSION, "revision": REVISION}

    def payloads(self):
        (self.assets / "release.json").write_text(json.dumps(self.manifest), encoding="utf-8")
        tar(self.assets / "quazonai-deploy.tar.gz", {
            **{name: b"fixture\n" for name in package.DEPLOY_FILES},
            "release.json": json.dumps(self.manifest).encode(), "README.md": VERSION.encode(),
        })
        for platform in package.CLI_ARCHIVES:
            package.pack_cli(self.binary, platform, self.assets, VERSION, REVISION)
        for name in package.IMAGE_ARCHIVES.values():
            tar(self.assets / name, {
                "manifest.json": json.dumps([{"Config": "config.json", "Layers": ["layer.tar"]}]).encode(),
                "config.json": b"{}", "layer.tar": b"image-layer-fixture",
            })

    def prepare(self):
        templates = self.root / "templates" / "deploy"
        templates.mkdir(parents=True, exist_ok=True)
        for name in ("install.sh", "install.ps1"):
            (templates / name).write_text("version=" + package.MARKER + "\n", encoding="utf-8")
        with patch.object(package, "ROOT", templates.parent):
            package.prepare(self.assets)

    def test_all_platform_archives_contain_only_binary_and_licenses(self):
        for platform, name in package.CLI_ARCHIVES.items():
            with self.subTest(platform=platform):
                result = package.pack_cli(self.binary, platform, self.assets, VERSION, REVISION)
                self.assertEqual(result.name, name)
                if platform.startswith("windows-"):
                    with zipfile.ZipFile(result) as archive:
                        self.assertEqual(set(archive.namelist()), package.cli_members(platform))
                        self.assertEqual(archive.read("quazonai.exe"), self.binary.read_bytes())
                else:
                    with tarfile.open(result) as archive:
                        self.assertEqual(set(archive.getnames()), package.cli_members(platform))
                        self.assertEqual(archive.extractfile("quazonai").read(), self.binary.read_bytes())
                        self.assertEqual(archive.getmember("quazonai").mode, 0o755)

    def test_complete_release_has_exact_tag_and_complete_checksums(self):
        self.payloads()
        self.prepare()
        self.assertEqual({path.name for path in self.assets.iterdir()}, package.REQUIRED_ASSETS)
        self.assertEqual(package.verify(self.assets), self.manifest)
        readme = (self.assets / "README.md").read_text(encoding="utf-8")
        self.assertEqual(readme.count(f"/releases/download/{VERSION}/"), 3)
        self.assertNotIn(package.MARKER, readme)
        for name in ("install.sh", "install.ps1"):
            self.assertEqual((self.assets / name).read_text(encoding="utf-8"), "version=" + VERSION + "\n")

    def test_partial_release_cannot_be_prepared(self):
        self.payloads()
        for name in [*package.CLI_ARCHIVES.values(), *package.IMAGE_ARCHIVES.values(), "quazonai-deploy.tar.gz"]:
            path = self.assets / name
            original = path.read_bytes()
            path.unlink()
            with self.subTest(name=name), self.assertRaisesRegex(ValueError, "Missing"):
                self.prepare()
            path.write_bytes(original)
        self.assertFalse((self.assets / "SHA256SUMS").exists())

    def test_changed_or_incomplete_checksum_cannot_verify(self):
        self.payloads()
        self.prepare()
        guide = self.assets / "README.md"
        guide.write_text("changed " + VERSION, encoding="utf-8")
        with self.assertRaisesRegex(ValueError, "checksum failed"):
            package.verify(self.assets)
        checksums(self.assets)
        sums = self.assets / "SHA256SUMS"
        sums.write_text(sums.read_text(encoding="ascii").splitlines()[0] + "\n", encoding="ascii")
        with self.assertRaisesRegex(ValueError, "every required asset"):
            package.verify(self.assets)

    def test_unsafe_cli_members_are_rejected(self):
        output = package.pack_cli(self.binary, "linux-x86_64", self.assets, VERSION, REVISION)
        tar(output, {"../quazonai": b"bad"})
        with self.assertRaisesRegex(ValueError, "only the binary"):
            package.check_cli_archive(output, "linux-x86_64")
        output = self.assets / package.CLI_ARCHIVES["windows-x86_64"]
        with zipfile.ZipFile(output, "w") as archive:
            for name in package.cli_members("windows-x86_64"):
                member = zipfile.ZipInfo(name)
                member.create_system = 3
                member.external_attr = (0o120777 if name == "quazonai.exe" else 0o100644) << 16
                archive.writestr(member, b"data")
        with self.assertRaisesRegex(ValueError, "only the binary"):
            package.check_cli_archive(output, "windows-x86_64")

    def test_image_archive_requires_safe_complete_docker_payload(self):
        path = self.assets / "image.tar.gz"
        for files in ({"../manifest.json": b"[]"}, {"manifest.json": b"[]"},
                      {"manifest.json": b'[{"Config":"missing","Layers":["missing"]}]'}):
            tar(path, files)
            with self.subTest(files=files), self.assertRaises(ValueError):
                package.check_tar(path)

    def test_bundle_version_and_guide_must_match(self):
        path = self.assets / "bundle.tar.gz"
        for metadata, guide in (({**self.manifest, "revision": "b" * 40}, VERSION),
                                (self.manifest, package.MARKER)):
            tar(path, {**{name: b"fixture\n" for name in package.DEPLOY_FILES},
                       "release.json": json.dumps(metadata).encode(), "README.md": guide.encode()})
            with self.subTest(metadata=metadata, guide=guide), self.assertRaises(ValueError):
                package.check_tar(path, manifest=self.manifest)

    def test_extracted_cli_exercises_version_help_and_offline_schema(self):
        path = package.pack_cli(self.binary, "linux-x86_64", self.assets, VERSION, REVISION)
        outputs = ["quazonai " + VERSION, "login", '["ArtifactCreate"]', '{}']
        with patch.object(package.subprocess, "run", side_effect=[
                subprocess.CompletedProcess([], 0, stdout=value) for value in outputs]) as run:
            package.smoke_cli(path, "linux-x86_64", VERSION)
        self.assertEqual([call.args[0][1:] for call in run.call_args_list], [
            ["--version"], ["client", "--help"], ["openapi", "--list-schemas"],
            ["openapi", "--schema", "ArtifactCreate"],
        ])
        with patch.object(package.subprocess, "run", return_value=subprocess.CompletedProcess([], 0, stdout="wrong")):
            with self.assertRaisesRegex(ValueError, "version differs"):
                package.smoke_cli(path, "linux-x86_64", VERSION)

    def test_rejects_unversioned_identity_and_oversized_assets(self):
        for tag, revision in (("dev", REVISION), ("v1.2.3-01", REVISION), (VERSION, "dev")):
            with self.subTest(tag=tag), self.assertRaises(ValueError):
                package.pack_cli(self.binary, "linux-x86_64", self.assets, tag, revision)
        with self.assertRaisesRegex(ValueError, "oversized"):
            package.regular(self.binary, 1)


if __name__ == "__main__":
    unittest.main()
