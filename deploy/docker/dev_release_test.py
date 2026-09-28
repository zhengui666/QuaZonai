"""Automatic tags preserve the exact push and retry identity; no GitHub writes."""
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import release


class DevReleaseTests(unittest.TestCase):
    def test_timestamped_tag_retry_ancestry_and_source_binding(self):
        previous = Path.cwd()
        with tempfile.TemporaryDirectory() as temporary, patch.dict(os.environ, {"RELEASE_BRANCH": "dev"}):
            os.chdir(temporary)
            try:
                def git(*args):
                    return release.run(["git", *args], capture=True)
                git("init", "-b", "dev")
                git("config", "user.email", "release-test@example.invalid")
                git("config", "user.name", "Release test")
                Path("Cargo.toml").write_text('[workspace.package]\nversion = "2.0.0-dev.1"\n')
                git("add", ".")
                git("commit", "-m", "source")
                revision = git("rev-parse", "HEAD")
                git("update-ref", "refs/remotes/origin/dev", revision)
                execution = {"head_sha": revision, "head_branch": "dev", "event": "push",
                             "created_at": "2026-09-28T01:02:03Z"}
                expected = "v2.0.0-dev.20260928010203.1234"
                command = release.run
                writes = []
                def run(args, **kwargs):
                    if args[:3] == ["gh", "api", "--method"]:
                        writes.append(args)
                        git("tag", expected, revision)
                        return '{"object":{"sha":"' + revision + '"}}'
                    return command(args, **kwargs)
                with patch.object(release, "api", return_value=execution), patch.object(release, "run", side_effect=run):
                    self.assertEqual(release.dev_tag(revision, "1234"), expected)
                    self.assertEqual(release.dev_tag(revision, "1234"), expected)
                    self.assertEqual(len(writes), 1)
                    self.assertIn("sha=" + revision, writes[0])
                    with self.assertRaises(ValueError):
                        release.dev_tag(revision, "../bad")
                for changed in ({"head_sha": "b" * 40}, {"head_branch": "feature"}, {"event": "pull_request"}):
                    with patch.object(release, "api", return_value={**execution, **changed}), self.assertRaises(ValueError):
                        release.dev_tag(revision, "1234")
                Path("change").write_text("next")
                git("add", ".")
                git("commit", "-m", "next")
                newer = git("rev-parse", "HEAD")
                with self.assertRaises(ValueError):
                    release.dev_tag(newer, "1234")
                git("tag", "--force", expected, newer)
                with patch.object(release, "api", return_value=execution), self.assertRaisesRegex(ValueError, "not moved"):
                    release.dev_tag(revision, "1234")
            finally:
                os.chdir(previous)

    def test_wait_requires_all_current_source_checks_and_rejects_failure(self):
        success = {path: {"status": "completed", "conclusion": "success"} for path in release.CI_PATHS}
        with patch.object(release, "ci_runs", side_effect=[{}, success]), patch.object(release.time, "sleep") as sleep:
            release.wait_ci("a" * 40)
            sleep.assert_called_once_with(30)
        for conclusion in ("failure", "cancelled", "skipped", "timed_out"):
            failed = {**success, ".github/workflows/cli.yml": {"status": "completed", "conclusion": conclusion}}
            with patch.object(release, "ci_runs", return_value=failed), self.assertRaises(ValueError):
                release.wait_ci("a" * 40)

    def test_dev_does_not_authorize_stable_tags_or_an_arbitrary_branch(self):
        with patch.dict(os.environ, {"RELEASE_BRANCH": "dev"}), self.assertRaisesRegex(ValueError, "timestamped"):
            release.verify("v2.0.0", "a" * 40)
        with patch.dict(os.environ, {"RELEASE_BRANCH": "unreviewed"}), self.assertRaises(ValueError):
            release.is_merged("a" * 40)


if __name__ == "__main__":
    unittest.main()
