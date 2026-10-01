"""Failure evidence and cleanup; real Runtime acceptance remains in smoke.py."""
import contextlib
import io
import json
import os
from pathlib import Path
import sqlite3
import tempfile
import unittest
from unittest.mock import Mock, patch
import urllib.error
import urllib.parse

import smoke


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
