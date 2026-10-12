"""Application/scientific publication ordering; no registry writes or model calls."""
import json
import os
from pathlib import Path
import unittest
from unittest.mock import patch
import release

REVISION = "a" * 40
LABELS = {"org.opencontainers.image.revision": REVISION, "org.opencontainers.image.version": "v1.2.3"}
ARGV = ["release.py", "push-images", "--version", "v1.2.3", "--revision", REVISION,
        "--image", "test-app", "--runtime-image", "test-runtime", "--output", "/unused"]

class PublicationPreflightTests(unittest.TestCase):
    def test_bad_second_image_prevents_every_push(self):
        for field, wrong in (("org.opencontainers.image.revision", "b" * 40),
                             ("org.opencontainers.image.version", "v9.9.9")):
            with self.subTest(field=field), patch.object(release.sys, "argv", ARGV), \
                    patch.object(release, "verify"), patch.object(release, "run", side_effect=[
                        json.dumps(LABELS), json.dumps({**LABELS, field: wrong})]), \
                    patch.object(release, "push_image") as push, patch.object(release, "bundle") as bundle, \
                    self.assertRaisesRegex(ValueError, "version and source"):
                release.main()
            push.assert_not_called()
            bundle.assert_not_called()

    def test_all_candidate_checks_precede_first_push_and_use_original_release_tag(self):
        for branch in ("main", "dev"):
            events = []
            def inspect(args, **kwargs):
                events.append(("inspect", args[-1]))
                return json.dumps(LABELS)
            def publish(image, repository, tag):
                events.append(("push", image, tag))
                return repository + "@sha256:" + "c" * 64
            with self.subTest(branch=branch), patch.dict(os.environ, {"RELEASE_BRANCH": branch}), \
                    patch.object(release.sys, "argv", ARGV), patch.object(release, "verify"), \
                    patch.object(release, "run", side_effect=inspect), \
                    patch.object(release, "push_image", side_effect=publish), patch.object(release, "bundle") as bundle:
                release.main()
            self.assertEqual(events, [("inspect", "test-app"), ("inspect", "test-runtime"),
                                      ("push", "test-app", "v1.2.3"), ("push", "test-runtime", "v1.2.3")])
            self.assertEqual(bundle.call_args.args[3], Path("/unused"))
            self.assertTrue(bundle.call_args.kwargs["published"])
            self.assertEqual(set(bundle.call_args.kwargs), {"runtime_image", "published"})

    def test_retired_producer_is_not_available(self):
        with patch.object(release.sys, "argv", ["release.py", "build-codex"]), self.assertRaises(SystemExit):
            release.main()
        self.assertFalse(hasattr(release, "CODEX_REPOSITORY"))

if __name__ == '__main__':
    unittest.main()
