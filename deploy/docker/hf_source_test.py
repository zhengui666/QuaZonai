"""Real installed-manager dispatch using the actual plugin inventory, without Docker execution."""

import json
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

import manage

sys.path.insert(0, str(Path(__file__).parents[2] / "runtimes/data"))
import source_plugins


class InstalledHfSourceTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.installation, self.outputs = self.root / "installation", self.root / "outputs"
        self.installation.mkdir()
        self.outputs.mkdir()
        self.config = {"root": str(self.installation), "uid": os.getuid(), "gid": os.getgid(),
                       "project": "hf-source-test", "bundle": str(self.root / "bundle"),
                       "codex_home": str(self.root / "codex"), "unit_directory": str(self.root / "units"),
                       "docker_socket": "/run/docker.sock"}
        self.release = {"version": "v0.0.0-hf-test", "revision": "a" * 40,
                        "image": "sha256:" + "b" * 64}
        self.calls = []
        self.addCleanup(patch.stopall)
        patch.object(manage, "configuration", return_value=self.config).start()
        patch.object(manage, "manifest", return_value=self.release).start()
        patch.dict(os.environ, {"DOCKER_HOST": "unix:///run/docker.sock"}).start()
        patch.object(manage, "run", side_effect=self.respond).start()

    def respond(self, args, **kwargs):
        self.calls.append(args)
        if args[:2] == ["docker", "info"]:
            return "[]"
        if args[:3] == ["docker", "image", "inspect"]:
            return json.dumps({"org.opencontainers.image.revision": self.release["revision"]})
        if args[-1] == "plugins":
            return json.dumps(source_plugins.plugin_descriptors())
        raise AssertionError(args)

    def command(self, arguments, inputs=None, output=None):
        return manage.source_command(self.installation, inputs or [], output, arguments, "c" * 32)

    def test_download_new_handoffs_share_stable_writable_cache_mount(self):
        cache = self.outputs / "hf-cache"
        for name in ("request-01", "request-02"):
            arguments = ["download", "hf-dataset", "--dataset", "example/history",
                         "--include", "data/a.parquet", "--cache-dir", str(cache),
                         "--output", str(self.outputs / name)]
            command = self.command(arguments, output=self.outputs)
            self.assertEqual(command[command.index("--network") + 1], "bridge")
            self.assertIn(f"type=bind,source={self.outputs},target={self.outputs}", command)
            self.assertEqual(command[-len(arguments):], arguments)
        self.assertFalse(cache.exists())  # These checks never run the container/download.
        self.assertEqual(sum(c[-1] == "plugins" for c in self.calls), 2)

    def test_actual_inventory_selects_networked_plan_and_offline_verification(self):
        command = self.command(["plan", "hf-dataset", "--dataset", "example/history", "--include", "data/a.parquet"])
        self.assertEqual(command[command.index("--network") + 1], "bridge")
        command = self.command(["verify", "hf-dataset", "--selection", str(self.outputs / "request/selection.json")],
                               inputs=[self.outputs])
        self.assertEqual(command[command.index("--network") + 1], "none")
        self.assertIn(f"type=bind,source={self.outputs},target={self.outputs},readonly", command)

    def test_installed_retry_keeps_original_output_and_requires_new_handoff(self):
        original = self.outputs / "request-01"
        original.mkdir()
        (original / "request.json").write_bytes(b"original partial request")
        arguments = ["download", "hf-dataset", "--dataset", "example/history", "--include", "data/a.parquet",
                     "--cache-dir", str(self.outputs / "hf-cache"), "--output", str(original)]
        with self.assertRaisesRegex(ValueError, "must be new"):
            self.command(arguments, output=self.outputs)
        self.assertEqual((original / "request.json").read_bytes(), b"original partial request")
        self.assertEqual(self.calls, [])


if __name__ == "__main__":
    unittest.main()
