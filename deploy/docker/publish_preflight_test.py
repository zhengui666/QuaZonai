"""Publication ordering and shared-version regressions; no registry writes."""
import json
from pathlib import Path
import subprocess
import unittest
from unittest.mock import patch

import release


VERSION = "0.157.0"
REVISION = "a" * 40
DIGEST = "ghcr.io/zhengui666/quazonai-codex@sha256:" + "d" * 64
LABELS = {"org.opencontainers.image.revision": REVISION,
          "org.opencontainers.image.version": "v1.2.3"}
ARGV = ["release.py", "push-images", "--version", "v1.2.3", "--revision", REVISION,
        "--image", "test-app", "--runtime-image", "test-runtime",
        "--codex-image", "test-codex", "--codex-version", VERSION, "--output", "/unused"]


class PublicationPreflightTests(unittest.TestCase):
    def test_bad_second_image_prevents_every_push(self):
        for field, wrong in (("org.opencontainers.image.revision", "b" * 40),
                             ("org.opencontainers.image.version", "v9.9.9")):
            with self.subTest(field=field), patch.object(release.sys, "argv", ARGV), \
                    patch.object(release, "verify"), patch.object(release, "run", side_effect=[
                        json.dumps(LABELS), json.dumps({**LABELS, field: wrong})]), \
                    patch.object(release, "push_image") as push, \
                    patch.object(release.codex, "verify_candidate") as check_codex, \
                    patch.object(release, "bundle") as bundle, \
                    self.assertRaisesRegex(ValueError, "version and source"):
                release.main()
            push.assert_not_called()
            check_codex.assert_not_called()
            bundle.assert_not_called()

    def test_bad_codex_or_unreadable_existing_version_prevents_every_push(self):
        for failure in ("candidate", "registry"):
            with self.subTest(failure=failure), patch.object(release.sys, "argv", ARGV), \
                    patch.object(release, "verify"), \
                    patch.object(release, "run", return_value=json.dumps(LABELS)), \
                    patch.object(release, "docker_configuration", return_value={}), \
                    patch.object(release.codex, "verify_candidate",
                                 side_effect=ValueError("candidate") if failure == "candidate" else None), \
                    patch.object(release, "published_codex_image", side_effect=ValueError("registry")), \
                    patch.object(release, "push_image") as push, \
                    patch.object(release, "bundle") as bundle, self.assertRaisesRegex(ValueError, failure):
                release.main()
            push.assert_not_called()
            bundle.assert_not_called()

    def test_all_preflight_precedes_first_push_and_existing_codex_is_reused(self):
        for existing in (None, DIGEST):
            events = []
            def inspect(args, **kwargs):
                events.append(("inspect", args[-1]))
                return json.dumps(LABELS)
            def check(config, target, image):
                events.append(("codex", image))
            def lookup(target):
                events.append(("lookup", target))
                return existing
            def publish(image, repository, tag):
                events.append(("push", image))
                return repository + "@sha256:" + "c" * 64
            with self.subTest(existing=existing), patch.object(release.sys, "argv", ARGV), \
                    patch.object(release, "verify"), patch.object(release, "run", side_effect=inspect), \
                    patch.object(release, "docker_configuration", return_value={}), \
                    patch.object(release.codex, "verify_candidate", side_effect=check), \
                    patch.object(release, "published_codex_image", side_effect=lookup), \
                    patch.object(release, "push_image", side_effect=publish), \
                    patch.object(release, "bundle") as bundle:
                release.main()
            self.assertEqual(events[:4], [("inspect", "test-app"), ("inspect", "test-runtime"),
                                          ("codex", "test-codex"), ("lookup", VERSION)])
            self.assertEqual(events[4:], [("push", "test-app"), ("push", "test-runtime")]
                             + ([] if existing else [("push", "test-codex")]))
            self.assertEqual(bundle.call_args.kwargs["codex_image"],
                             existing or release.CODEX_REPOSITORY + "@sha256:" + "c" * 64)
            self.assertEqual(bundle.call_args.kwargs["codex_version"], VERSION)
            self.assertEqual(bundle.call_args.args[3], Path("/unused"))
            self.assertTrue(bundle.call_args.kwargs["published"])

    def test_standalone_publisher_uses_same_immutable_version_policy(self):
        for existing in (None, DIGEST):
            with self.subTest(existing=existing), patch.object(release.sys, "argv", [
                "release.py", "publish-codex", "--codex-version", VERSION, "--image", "rebuilt-image"
            ]), patch.object(release, "docker_configuration", return_value={}), \
                    patch.object(release.codex, "verify_candidate") as check, \
                    patch.object(release, "published_codex_image", return_value=existing), \
                    patch.object(release, "push_image", return_value=DIGEST) as push, \
                    patch.object(release, "advance_codex_latest") as latest:
                release.main()
            check.assert_called_once_with({}, VERSION, "rebuilt-image")
            if existing:
                push.assert_not_called()
            else:
                push.assert_called_once_with("rebuilt-image", release.CODEX_REPOSITORY, VERSION)
            latest.assert_called_once_with(DIGEST, VERSION)


class PublishedCodexTests(unittest.TestCase):
    def metadata(self):
        return {"RepoDigests": [DIGEST], "Os": "linux", "Architecture": "amd64",
                "Config": {"Labels": {"org.opencontainers.image.version": VERSION}}}

    def test_existing_image_is_verified_by_digest_without_tag_writes(self):
        with patch.object(release.subprocess, "run", return_value=subprocess.CompletedProcess([], 0, "", "")) as pull, \
                patch.object(release, "run", return_value=json.dumps([self.metadata()])) as inspect, \
                patch.object(release, "docker_configuration", return_value={}), \
                patch.object(release.codex, "verify_candidate") as check, \
                patch.object(release, "push_image") as push:
            self.assertEqual(release.published_codex_image(VERSION), DIGEST)
        self.assertEqual(pull.call_args.args[0], ["docker", "pull", release.CODEX_REPOSITORY + ":" + VERSION])
        self.assertEqual(inspect.call_args.args[0], ["docker", "image", "inspect", release.CODEX_REPOSITORY + ":" + VERSION])
        check.assert_called_once_with({}, VERSION, DIGEST)
        push.assert_not_called()

    def test_only_a_missing_manifest_allows_new_publication(self):
        for message in ("manifest unknown", "no such manifest", "name unknown",
                        "unauthorized", "registry timeout", "connection refused"):
            result = subprocess.CompletedProcess([], 1, "", message)
            with self.subTest(message=message), patch.object(release.subprocess, "run", return_value=result), \
                    patch.object(release, "run") as inspect, patch.object(release, "push_image") as push:
                if message in ("manifest unknown", "no such manifest", "name unknown"):
                    self.assertIsNone(release.published_codex_image(VERSION))
                else:
                    with self.assertRaisesRegex(ValueError, "no image was pushed"):
                        release.published_codex_image(VERSION)
            inspect.assert_not_called()
            push.assert_not_called()

    def test_incompatible_existing_metadata_is_not_overwritten(self):
        for change in ({"Architecture": "arm64"}, {"Os": "windows"}, {"RepoDigests": []},
                       {"RepoDigests": ["ghcr.io/other/codex@sha256:" + "d" * 64]},
                       {"RepoDigests": [DIGEST + "extra"]},
                       {"Config": {"Labels": {"org.opencontainers.image.version": "0.156.1"}}}):
            metadata = {**self.metadata(), **change}
            with self.subTest(change=change), patch.object(release.subprocess, "run", return_value=subprocess.CompletedProcess([], 0, "", "")), \
                    patch.object(release, "run", return_value=json.dumps([metadata])), \
                    patch.object(release.codex, "verify_candidate") as check, \
                    patch.object(release, "push_image") as push, self.assertRaisesRegex(ValueError, "not overwritten"):
                release.published_codex_image(VERSION)
            check.assert_not_called()
            push.assert_not_called()

    def test_existing_native_verification_failure_aborts(self):
        with patch.object(release.subprocess, "run", return_value=subprocess.CompletedProcess([], 0, "", "")), \
                patch.object(release, "run", return_value=json.dumps([self.metadata()])), \
                patch.object(release, "docker_configuration", return_value={}), \
                patch.object(release.codex, "verify_candidate", side_effect=ValueError("sandbox unavailable")), \
                patch.object(release, "push_image") as push, self.assertRaisesRegex(ValueError, "sandbox unavailable"):
            release.published_codex_image(VERSION)
        push.assert_not_called()

    def test_latest_does_not_replace_equal_or_newer_version(self):
        for previous in (VERSION, "0.158.0"):
            with self.subTest(previous=previous), patch.object(release.subprocess, "run", return_value=subprocess.CompletedProcess([], 0, "", "")), \
                    patch.object(release, "run", return_value=previous) as inspect, \
                    patch.object(release, "push_image") as push:
                release.advance_codex_latest(DIGEST, VERSION)
            self.assertEqual(inspect.call_count, 1)
            push.assert_not_called()

    def test_latest_advances_only_for_newer_stable_version(self):
        with patch.object(release.subprocess, "run", return_value=subprocess.CompletedProcess([], 0, "", "")), \
                patch.object(release, "run", return_value="0.156.1") as command, \
                patch.object(release, "push_image") as push:
            release.advance_codex_latest(DIGEST, VERSION)
        self.assertEqual(command.call_args.args[0], ["docker", "pull", DIGEST])
        push.assert_called_once_with(DIGEST, release.CODEX_REPOSITORY, "latest")
        with patch.object(release.subprocess, "run") as pull, patch.object(release, "push_image") as push:
            release.advance_codex_latest(DIGEST, "0.158.0-beta.1")
        pull.assert_not_called()
        push.assert_not_called()


if __name__ == "__main__":
    unittest.main()
