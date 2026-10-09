"""Failure evidence and cleanup; real Runtime acceptance remains in smoke.py."""
import contextlib
import http.server
import io
import json
import os
from pathlib import Path
import sqlite3
import subprocess
import tempfile
import threading
import unittest
from unittest.mock import Mock, patch
import urllib.error
import urllib.parse

import smoke


class PublishedBundleCurlTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.assets = self.root / "assets"
        self.assets.mkdir()
        self.requests = []
        self.responses = {"/health/live": (204, b""), "/": (200, b"<!doctype html><html>real frontend</html>")}
        self.headers = {}
        owner = self

        class Handler(http.server.BaseHTTPRequestHandler):
            def do_GET(self):
                owner.requests.append((self.path, self.headers.get("Accept")))
                status, body = owner.responses.get(self.path, (404, b"not found"))
                self.send_response(status)
                for name, value in owner.headers.items():
                    self.send_header(name, value)
                self.end_headers()
                self.wfile.write(body)

            def log_message(self, *args):
                pass

        self.server = http.server.HTTPServer(("127.0.0.1", 0), Handler)
        self.thread = threading.Thread(target=self.server.serve_forever, kwargs={"poll_interval": 0.01})
        self.thread.start()
        self.addCleanup(self.stop_server)
        self.port = self.server.server_port
        self.origin = f"http://localhost:{self.port}"
        self.base = f"https://github.com/{smoke.manage.REPOSITORY}/releases/download/v0.0.0-ci.1/"
        # Capture only the outer installer boundary, before Docker or any real
        # installation. The generated fixture and manager readiness calls run.
        with patch.object(smoke.manage, "manifest", return_value={"version": "v0.0.0-ci.1"}), \
             patch.object(smoke, "ports", return_value=(self.port, 5432)), \
             patch.object(smoke.manage, "run", side_effect=RuntimeError("installer boundary")) as run, \
             patch.object(smoke, "cleanup_installation"):
            with self.assertRaisesRegex(RuntimeError, "installer boundary"):
                smoke.verify_published_bundle(self.root, self.root / "bundle", self.assets)
        self.env = run.call_args.kwargs["env"]
        self.curl = self.root / "release-downloads/curl"

    def stop_server(self):
        self.server.shutdown()
        self.thread.join(timeout=5)
        self.server.server_close()

    def curl_run(self, args):
        return subprocess.run([str(self.curl), *args], env=self.env, capture_output=True, timeout=15)

    def health_args(self, url=None):
        return ["--silent", "--show-error", "--noproxy", "*", "--max-time", "10", "--output", "/dev/null",
                "--write-out", "%{http_code}", url or self.origin + "/health/live"]

    def frontend_args(self, output):
        return ["--fail", "--silent", "--show-error", "--noproxy", "*", "--max-time", "10", "--header",
                "Accept: text/html", "--output", str(output), self.origin + "/"]

    def console(self):
        config = self.root / "config.json"
        config.write_text(json.dumps({"port": self.port}))
        return subprocess.run(["bash", "-c", 'source "$1"; QZ_WORK=$2; qz_verify_console "$3"',
                               "readiness-test", str(Path(smoke.__file__).with_name("manage.sh")),
                               str(self.root), str(config)], env=self.env, capture_output=True, timeout=20)

    def test_release_assets_remain_exact_and_closed(self):
        output = self.root / "download"
        for name in ("SHA256SUMS", "quazonai-cli-linux-x86_64.tar.gz", "quazonai-deploy.tar.gz"):
            content = b"exact release bytes\x00\xff" + name.encode()
            (self.assets / name).write_bytes(content)
            result = self.curl_run(["--fail", "--location", "--output", str(output), self.base + name])
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(output.read_bytes(), content)
        for name in ("unknown", "../SHA256SUMS", "SHA256SUMS?query=1", "%2e%2e/SHA256SUMS"):
            with self.subTest(name=name):
                output.unlink(missing_ok=True)
                result = self.curl_run(["--output", str(output), self.base + name])
                self.assertNotEqual(result.returncode, 0)
                self.assertFalse(output.exists())
        self.assertEqual(self.requests, [])

    def test_real_readiness_status_and_frontend_bytes(self):
        result = self.curl_run(self.health_args())
        self.assertEqual((result.returncode, result.stdout, result.stderr), (0, b"204", b""))
        output = self.root / "frontend with spaces.html"
        result = self.curl_run(self.frontend_args(output))
        self.assertEqual((result.returncode, result.stdout, result.stderr), (0, b"", b""))
        self.assertEqual(output.read_bytes(), self.responses["/"][1])
        self.assertEqual(self.requests, [("/health/live", "*/*"), ("/", "text/html")])
        result = self.console()
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_original_manager_rejects_non204_http_errors_and_nonhtml(self):
        for health, frontend, message in ((200, (200, b"<!doctype html>"), b"API liveness check failed"),
                                          (503, (200, b"<!doctype html>"), b"API liveness check failed"),
                                          (204, (503, b"unavailable"), b"curl: (22)"),
                                          (204, (200, b"not HTML"), b"Production frontend was not served")):
            with self.subTest(health=health, frontend=frontend):
                self.responses = {"/health/live": (health, b""), "/": frontend}
                result = self.console()
                self.assertNotEqual(result.returncode, 0)
                self.assertIn(message, result.stderr)

    def test_real_curl_http_failure_status_and_stderr_are_preserved(self):
        self.responses["/"] = (503, b"unavailable")
        result = self.curl_run(self.frontend_args(self.root / "frontend.html"))
        self.assertEqual(result.returncode, 22)
        self.assertEqual(result.stdout, b"")
        self.assertIn(b"curl: (22)", result.stderr)
        self.assertEqual(self.requests, [("/", "text/html")])

    def test_other_urls_and_curl_routing_options_are_not_forwarded(self):
        for url in (f"http://127.0.0.1:{self.port}/health/live", f"http://localhost:{self.port + 1}/health/live",
                    f"http://user@localhost:{self.port}/health/live", self.origin + "/health/live?query=1",
                    self.origin + "/health/live#fragment", self.origin + "/other", "https://example.invalid/"):
            with self.subTest(url=url):
                result = self.curl_run(self.health_args(url))
                self.assertNotEqual(result.returncode, 0)
                self.assertIn(b"AssertionError", result.stderr)
        config = self.root / "curlrc"
        config.write_text('url = "' + self.origin + '/other"\n')
        for extra in ([self.origin + "/other"], ["--config", str(config)], ["-K", str(config)],
                      ["--location"], ["-L"], ["--resolve", f"localhost:{self.port}:127.0.0.1"],
                      ["--connect-to", f"localhost:{self.port}:127.0.0.1:{self.port}"]):
            with self.subTest(extra=extra):
                for args in (self.health_args(), self.frontend_args(self.root / "frontend.html")):
                    result = self.curl_run([*extra, *args])
                    self.assertNotEqual(result.returncode, 0)
                    self.assertIn(b"AssertionError", result.stderr)
        self.assertEqual(self.requests, [])

    def test_redirect_is_not_followed(self):
        self.responses["/health/live"] = (302, b"")
        self.headers["Location"] = self.origin + "/other"
        result = self.curl_run(self.health_args())
        self.assertEqual((result.returncode, result.stdout), (0, b"302"))
        self.assertEqual(self.requests, [("/health/live", "*/*")])
        result = self.console()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn(b"API liveness check failed", result.stderr)
        self.assertEqual([path for path, _ in self.requests], ["/health/live", "/health/live"])


class RuntimeFailureTests(unittest.TestCase):
    def setUp(self):
        self.status = {"run_id": "owned-run", "attempt_no": 1,
                       "external_job_id": "owned-run/1", "state": "FAILED", "has_result": True}
        self.result = {**self.status, "error": {"class": "RESOURCE_LIMIT", "code": "MEMORY_LIMIT",
                                               "safe_message": "private native text"},
                       "artifacts": [{"storage_ref": "private artifact path"}]}

    def test_only_closed_codes_are_reported(self):
        request = Mock(return_value=self.result)
        self.assertEqual(smoke.runtime_failure_facts(request, "/jobs/owned-run%2F1", self.status),
                         {"result": "read", "error_class": "RESOURCE_LIMIT", "error_code": "MEMORY_LIMIT"})
        request.assert_called_once_with("GET", "/jobs/owned-run%2F1/result")
        for value in ("private native text", ["private response"], {"credential": "private"}):
            with self.subTest(value=value):
                request.return_value = {**self.result, "error": {"class": value, "code": value}}
                self.assertEqual(smoke.runtime_failure_facts(request, "/jobs/owned-run%2F1", self.status),
                                 {"result": "read", "error_class": "UNRECOGNIZED", "error_code": "UNRECOGNIZED"})

    def test_result_must_match_the_observed_attempt_and_state(self):
        for field, value in (("run_id", "another-run"), ("attempt_no", 2),
                             ("external_job_id", "owned-run/2"), ("state", "SUCCEEDED")):
            with self.subTest(field=field):
                request = Mock(return_value={**self.result, field: value})
                self.assertEqual(smoke.runtime_failure_facts(request, "/jobs/owned-run%2F1", self.status),
                                 {"result": "identity_mismatch"})

    def test_missing_result_is_not_requested(self):
        request = Mock()
        self.assertEqual(smoke.runtime_failure_facts(request, "/jobs/owned-run%2F1",
                                                   {**self.status, "has_result": False}),
                         {"result": "not_available"})
        request.assert_not_called()

    def test_malformed_result_and_read_errors_do_not_escape(self):
        for result, expected in (([], "identity_mismatch"),
                                 ({**self.result, "error": "private response"}, "invalid_error")):
            with self.subTest(result=result):
                self.assertEqual(smoke.runtime_failure_facts(Mock(return_value=result), "/jobs/owned", self.status),
                                 {"result": expected})
        for error in (TimeoutError("private timeout"), ValueError("private response"),
                      AssertionError("private credential")):
            with self.subTest(error=type(error)):
                self.assertEqual(smoke.runtime_failure_facts(Mock(side_effect=error), "/jobs/owned", self.status),
                                 {"result": "unavailable"})

    def test_failure_reads_result_before_cleanup_and_preserves_assertion(self):
        for result_unavailable in (False, True):
            with self.subTest(result_unavailable=result_unavailable), tempfile.TemporaryDirectory() as temporary:
                events, stopped = [], []
                root = Path(temporary)
                process = Mock()
                process.poll.side_effect = lambda: 0 if stopped else None
                process.terminate.side_effect = lambda: (events.append("terminate"), stopped.append(True))
                process.wait.side_effect = lambda **kwargs: events.append("wait")

                def open_request(request, *, timeout):
                    self.assertEqual(timeout, 15)
                    route = urllib.parse.urlsplit(request.full_url).path
                    if route.endswith("/capabilities"):
                        value = {}
                    elif request.method == "PUT":
                        value = {"artifact_id": route.rsplit("/", 1)[1], "byte_count": len(request.data)}
                    elif request.method == "POST":
                        value = {"external_job_id": self.status["external_job_id"]}
                    elif route.endswith("/result"):
                        events.append("result")
                        self.assertFalse(stopped)
                        self.assertTrue(list(root.glob("quazonai-runtime-*/credential")))
                        if result_unavailable:
                            raise urllib.error.URLError("private credential or response")
                        value = self.result
                    else:
                        value = self.status
                    return io.BytesIO(json.dumps(value).encode())

                def docker(config, *args, **kwargs):
                    events.append(args[0])
                    if args[0] == "ps":
                        self.assertEqual(args, ("ps", "--all", "--quiet", "--filter", "label=io.quazonai.run=owned-run"))
                        return "owned-container"
                    self.assertEqual(args, ("rm", "--force", "owned-container"))
                    return ""

                opener = Mock()
                opener.open.side_effect = open_request
                output = io.StringIO()
                config = {"root": str(root / "installation"), "bundle": str(root / "bundle"),
                          "runtime_image": "owned-image", "docker_socket": str(root / "docker.sock")}
                with patch.object(smoke.manage, "run"), patch.object(smoke, "ports", return_value=(12345, 12346)), \
                     patch.object(smoke, "fixture_id", side_effect=("owned-run", "source", "parameters", "inputs")), \
                     patch.object(smoke.urllib.request, "build_opener", return_value=opener), \
                     patch.object(smoke.subprocess, "Popen", return_value=process), \
                     patch.object(smoke.codex, "docker", side_effect=docker), contextlib.redirect_stderr(output):
                    with self.assertRaises(AssertionError) as raised:
                        smoke.verify_runtime(config)
                self.assertEqual(raised.exception.args, (self.status,))
                self.assertEqual(events, ["result", "terminate", "wait", "ps", "rm"])
                process.wait.assert_called_once_with(timeout=20)
                self.assertEqual(list(root.glob("quazonai-runtime-*")), [])
                self.assertNotIn("private", output.getvalue())
                expected = {"result": "unavailable"} if result_unavailable else {
                    "result": "read", "error_class": "RESOURCE_LIMIT", "error_code": "MEMORY_LIMIT"}
                expected["native_exit"] = "unavailable"
                self.assertEqual(output.getvalue(), "Native compile failure: " + json.dumps(expected, sort_keys=True) + "\n")


class NativeExitFactsTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.state = self.root / "state"
        self.state.mkdir()
        self.path = self.state / "journal.sqlite"
        self.spec = {"run_id": "01a0f687-da93-7094-8168-2d5ee2de02d5", "attempt_no": 1,
                     "external_job_id": "01a0f687-da93-7094-8168-2d5ee2de02d5/1"}

    def journal(self, *, exit_code=1, oom_killed=0, job=None, observation=None, strict=True):
        # A real live WAL and the first-party column layout; ANY allows corrupt
        # scalar fixtures that the production INTEGER/CHECK constraints reject.
        connection = sqlite3.connect(self.path)
        self.addCleanup(connection.close)
        connection.execute("PRAGMA journal_mode=WAL")
        suffix = " STRICT" if strict else ""
        connection.execute("CREATE TABLE runtime_jobs(external_id TEXT PRIMARY KEY, run_id TEXT, attempt_no ANY, "
                           "container_id TEXT, started_us INTEGER, phase TEXT)" + suffix)
        connection.execute("CREATE TABLE native_exit_observations(external_id TEXT PRIMARY KEY, container_id TEXT, "
                           "started_us INTEGER, exit_code ANY, oom_killed ANY)" + suffix)
        row = {**self.spec, "container_id": "a" * 64, "started_us": 2, "phase": "TERMINAL", **(job or {})}
        connection.execute("INSERT INTO runtime_jobs VALUES(?,?,?,?,?,?)",
                           (row["external_job_id"], row["run_id"], row["attempt_no"], row["container_id"],
                            row["started_us"], row["phase"]))
        row = {**self.spec, "container_id": "a" * 64, "started_us": 2, **(observation or {})}
        connection.execute("INSERT INTO native_exit_observations VALUES(?,?,?,?,?)",
                           (row["external_job_id"], row["container_id"], row["started_us"], exit_code, oom_killed))
        connection.commit()
        return connection

    def facts(self, spec=None):
        return smoke.runtime_native_exit_facts(self.root, self.spec if spec is None else spec)

    def test_reads_live_wal_without_mutating_source_and_emits_only_two_facts(self):
        self.journal(exit_code=137, oom_killed=1)
        before = {path.name: path.read_bytes() for path in self.state.iterdir()}
        self.assertTrue(before["journal.sqlite-wal"])
        facts = self.facts()
        self.assertEqual(facts, {"native_exit": "read", "exit_code": 137, "oom_killed": True})
        self.assertIs(type(facts["exit_code"]), int)
        self.assertIs(type(facts["oom_killed"]), bool)
        self.assertEqual({path.name: path.read_bytes() for path in self.state.iterdir()}, before)

    def test_missing_and_wrong_owned_identifiers_are_unavailable(self):
        self.journal()
        for spec in ({}, {**self.spec, "run_id": "private-secret"}, {**self.spec, "attempt_no": True},
                     {**self.spec, "attempt_no": 0}, {**self.spec, "external_job_id": "another/1"},
                     {**self.spec, "attempt_no": 2, "external_job_id": self.spec["run_id"] + "/2"}):
            with self.subTest(spec=spec):
                self.assertEqual(self.facts(spec), {"native_exit": "unavailable"})

    def test_unmatched_rows_and_nonterminal_jobs_are_unavailable(self):
        for job, observation in (({"run_id": "another"}, {}), ({"attempt_no": "1"}, {}),
                                 ({"external_job_id": "another/1"}, {}), ({"phase": "RUNNING"}, {}),
                                 ({}, {"external_job_id": "another/1"}), ({}, {"container_id": "b" * 64}),
                                 ({}, {"started_us": 99})):
            with self.subTest(job=job, observation=observation), tempfile.TemporaryDirectory() as temporary:
                self.path = Path(temporary) / "journal.sqlite"
                self.journal(job=job, observation=observation)
                for name in ("journal.sqlite", "journal.sqlite-wal"):
                    (self.state / name).write_bytes((self.path.parent / name).read_bytes())
                self.assertEqual(self.facts(), {"native_exit": "unavailable"})

    def test_wrong_scalar_types_and_invalid_boolean_values_are_unavailable(self):
        for code, oom in (("private-secret", 0), (1.5, 0), (None, 0), (1, "1"), (1, 0.0), (1, 2), (1, None)):
            with self.subTest(code=code, oom=oom), tempfile.TemporaryDirectory() as temporary:
                self.path = Path(temporary) / "journal.sqlite"
                self.journal(exit_code=code, oom_killed=oom)
                for name in ("journal.sqlite", "journal.sqlite-wal"):
                    (self.state / name).write_bytes((self.path.parent / name).read_bytes())
                self.assertEqual(self.facts(), {"native_exit": "unavailable"})

    def test_absent_corrupt_oversized_or_wrong_schema_journal_is_unavailable(self):
        self.assertEqual(self.facts(), {"native_exit": "unavailable"})
        self.path.write_bytes(b"private corrupt SQLite content")
        self.assertEqual(self.facts(), {"native_exit": "unavailable"})
        with self.path.open("wb") as output:
            output.truncate(8 * 1024 * 1024 + 1)
        self.assertEqual(self.facts(), {"native_exit": "unavailable"})
        self.path.unlink()
        self.journal(strict=False)
        self.assertEqual(self.facts(), {"native_exit": "unavailable"})

    def test_symlinks_fifo_and_hardlinks_are_not_read(self):
        outside = self.root / "private-secret"
        outside.write_bytes(b"secret")
        for name in ("journal.sqlite", "journal.sqlite-wal"):
            for kind in ("symlink", "fifo", "hardlink"):
                with self.subTest(name=name, kind=kind):
                    if name.endswith("-wal"):
                        self.path.write_bytes(b"private invalid database")
                    path = self.state / name
                    if kind == "symlink":
                        path.symlink_to(outside)
                    elif kind == "fifo":
                        os.mkfifo(path)
                    else:
                        os.link(outside, path)
                    self.assertEqual(self.facts(), {"native_exit": "unavailable"})
                    path.unlink()
        self.path.unlink(missing_ok=True)
        self.state.rmdir()
        self.state.symlink_to(self.root, target_is_directory=True)
        self.assertEqual(self.facts(), {"native_exit": "unavailable"})

    def test_read_failure_and_source_replacement_are_unavailable_without_text(self):
        self.journal()
        with patch.object(smoke.os, "fdopen", side_effect=OSError("private credential")):
            self.assertEqual(self.facts(), {"native_exit": "unavailable"})
        real_stat = os.stat
        def changed(name, **kwargs):
            if name == "journal.sqlite-wal":
                raise FileNotFoundError("private path")
            return real_stat(name, **kwargs)
        with patch.object(smoke.os, "stat", side_effect=changed):
            self.assertEqual(self.facts(), {"native_exit": "unavailable"})

    def test_boolean_is_not_accepted_as_a_sqlite_integer(self):
        self.journal()
        for row in ((True, 0), (1, False)):
            with self.subTest(row=row):
                journal = Mock()
                journal.execute.return_value.fetchone.return_value = ("main", "owned", "table", 6, 0, 1)
                journal.execute.return_value.fetchall.return_value = [row]
                with patch.object(smoke.sqlite3, "connect", return_value=journal):
                    self.assertEqual(self.facts(), {"native_exit": "unavailable"})

    def test_actual_first_party_migrations_are_supported(self):
        connection = sqlite3.connect(self.path)
        self.addCleanup(connection.close)
        connection.execute("PRAGMA journal_mode=WAL")
        migrations = Path(smoke.__file__).resolve().parents[2] / "apps/runtime/migrations"
        for name in ("0001_journal.sql", "0002_materializations_and_native_exits.sql"):
            connection.executescript((migrations / name).read_text())
        connection.execute(
            "INSERT INTO runtime_jobs(external_id,run_id,attempt_no,owner_epoch,spec_json,submitted_us,deadline_us,"
            "phase,launch_json,container_id,start_intent_us,started_us,terminal_state,finished_us,manifest_json,"
            "output_reservation) VALUES(?,?,1,1,'{}',0,100,'TERMINAL','{}',?,1,2,'FAILED',3,'{}',0)",
            (self.spec["external_job_id"], self.spec["run_id"], "a" * 64))
        connection.execute("INSERT INTO native_exit_observations VALUES(?,?,2,3,1,0,'NATIVE_JOB_FAILED',4)",
                           (self.spec["external_job_id"], "a" * 64))
        connection.commit()
        self.assertEqual(self.facts(), {"native_exit": "read", "exit_code": 1, "oom_killed": False})


if __name__ == "__main__":
    unittest.main()
