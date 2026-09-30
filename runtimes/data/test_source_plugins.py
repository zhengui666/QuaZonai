"""Offline orchestration tests. Stub native artifacts never establish actual conversion or admission."""

import contextlib
from copy import deepcopy
import hashlib
import io
import json
from pathlib import Path
import subprocess
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

import acquire
import providers
import snapshot
import source_plugins as plugins


COMMIT = "a" * 40
CLOCK = "2026-09-01T00:00:00Z"
OBSERVATION = {"status": 200, "headers": {"Content-Type": "application/json"},
               "request_started_at": CLOCK, "retrieved_at": "2026-09-01T00:00:01Z"}


class RegistryTest(unittest.TestCase):
    def invoke(self, argv):
        with contextlib.redirect_stdout(io.StringIO()) as stdout:
            self.assertEqual(plugins.main(argv), 0)
        return json.loads(stdout.getvalue())

    def test_capabilities_are_distinct_and_legacy_descriptors_stay_frozen(self):
        before = deepcopy([p.descriptor for p in providers.PROVIDERS.values()])
        result = {p["id"]: p for p in self.invoke(["plugins"])}
        self.assertNotIn("convert", result["polymarket-prices"]["capabilities"])
        self.assertNotIn("convert", result["hf-snapshot"]["capabilities"])
        for name in ("coinbase-candles", "polymarket-capture", "polymarket-archive"):
            self.assertIn("convert", result[name]["capabilities"])
        for item in result.values():
            self.assertEqual(item["access"], "PUBLIC_FREE")
            self.assertEqual(item["authentication"], "NONE")
            self.assertFalse(item["admission"]["research_qualified"])
            self.assertFalse(item["admission"]["registered_in_quazonai"])
        self.assertEqual(before, [p.descriptor for p in providers.PROVIDERS.values()])
        self.assertTrue(all(p["native_conversion"] == "UNSUPPORTED" for p in before))

    def test_new_plugin_and_operation_need_no_dispatch_edit(self):
        def options(parser):
            parser.add_argument("--original", required=True)
        example = plugins.SourcePlugin({"id": "example"}, {
            "inspect": plugins.Capability(options, lambda args: {"original": args.original})})
        with patch.dict(plugins.PLUGINS, {"example": example}):
            self.assertEqual(self.invoke(["inspect", "example", "--original", "kept"]), {"original": "kept"})

    def test_unsupported_conversion_is_not_a_false_ready_state(self):
        for name in ("polymarket-prices", "hf-snapshot"):
            with contextlib.redirect_stderr(io.StringIO()), self.assertRaises(SystemExit) as error:
                plugins.main(["convert", name])
            self.assertEqual(error.exception.code, 2)

    def test_http_plan_is_original_and_offline(self):
        args = ["plan", "coinbase-candles", "--instrument", "BTC-USD", "--start-seconds", "0",
                "--end-seconds", "120", "--interval-seconds", "60"]
        with patch.object(acquire.OPENER, "open") as network:
            result = self.invoke(args)
        self.assertEqual(result, acquire.plan("coinbase-candles", providers.Selection("BTC-USD", 0, 120, 60)))
        network.assert_not_called()

    def test_snapshot_operations_delegate_without_http_json_selection(self):
        for name in ("hf-snapshot", "polymarket-capture", "polymarket-archive"):
            argv = ["plan", name, "--dataset", "example/source", "--include", "original/*.parquet",
                    "--revision", COMMIT, "--max-bytes", "256", "--license", "original-license"]
            with patch.object(snapshot, "plan", return_value={"original": True}) as plan:
                self.assertEqual(self.invoke(argv), {"original": True})
                plan.assert_called_once_with("example/source", ["original/*.parquet"], COMMIT, 256, "original-license")
            argv[0] = "download"
            with patch.object(snapshot, "plan", return_value={"original": True}) as plan, \
                    patch.object(snapshot, "download", return_value={"frozen": True}) as download:
                self.assertEqual(self.invoke(argv + ["--output", "/tmp/new-snapshot"]), {"frozen": True})
                download.assert_called_once_with({"original": True}, Path("/tmp/new-snapshot"))


class PreparationTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.output = self.root / "native output; literal"
        self.binary = self.root / "native converter; literal"
        self.binary.write_text(f"#!{sys.executable}\nimport sys\nprint('original native diagnostic', file=sys.stderr)\n")
        self.binary.chmod(0o700)
        self.instruments = self.root / "original instruments.json"
        self.instruments.write_text('[{"CurrencyPair":{"original":"test fixture only"}}]')
        self.terms = self.root / "original-terms.txt"
        self.terms.write_text("Original source terms: offline test fixture only")
        self.acquisition = self.root / "acquisition"
        with patch.object(acquire, "fetch", return_value=(b'[[0,1,3,2,2,1],[60,1,3,2,2,1]]', OBSERVATION)):
            acquire.acquire("coinbase-candles", providers.Selection("BTC-USD", 0, 120, 60),
                            self.acquisition, self.terms)
        self.candles = SimpleNamespace(acquisition=self.acquisition / "acquisition.json",
            instruments=self.instruments, native_bin=self.binary, output=self.output)
        self.snapshot_dir = self.root / "snapshot"
        self.snapshot_dir.mkdir()
        source = self.snapshot_dir / "original.parquet"
        source.write_bytes(b"original archival source fixture, not decoded in Python")
        self.manifest = {"schema_version": 1, "repository": "example/capture", "revision": COMMIT,
            "license": "original-license", "license_reference": "https://huggingface.co/example/terms",
            "retrieved_at": CLOCK, "files": [{"path": source.name, "size": source.stat().st_size,
                "sha256": hashlib.sha256(source.read_bytes()).hexdigest(),
                "url": f"{snapshot.HUB}/datasets/example/capture/resolve/{COMMIT}/{source.name}"}]}
        self.snapshot_path = self.snapshot_dir / "snapshot.json"
        self.snapshot_path.write_text(json.dumps(self.manifest))
        self.capture = SimpleNamespace(native_bin=self.binary, output=self.output, snapshot=self.snapshot_path,
            max_bytes=1024, market_slug="btc-updown-5m-1785357600", start_seconds=1785357600,
            end_seconds=1785357900, bar_seconds=1)

    def base_report(self):
        return {"schema_version": 1, "native_version": "0.63.0", "instruments": 1,
            "instrument_versions": 1, "bars": 2, "catalog_relative_path": "catalog",
            "coverage": "UNPROVEN", "historical_availability": "UNVERIFIED",
            "registered_in_quazonai": False, "limitations": ["Offline fixture; not qualified"]}

    def candle_report(self):
        return {**self.base_report(), "source_provider": "coinbase-candles",
            "source_record_kind": "OHLCV_CANDLE", "acquisition_sha256": plugins.sha256(self.candles.acquisition),
            "instrument_definitions_sha256": plugins.sha256(self.instruments),
            "source_evidence_relative_path": "source-evidence.json", "native_readback_verified": True,
            "research_qualified": False}

    def candle_evidence(self):
        return {"schema_version": 1, "source_acquisition_path": str(self.candles.acquisition),
            "acquisition_sha256": plugins.sha256(self.candles.acquisition),
            "instrument_definitions_sha256": plugins.sha256(self.instruments),
            "acquisition": json.loads(self.candles.acquisition.read_text()),
            "instrument_definitions": json.loads(self.instruments.read_text())}

    def history_artifacts(self, capture=True):
        reference = f"{snapshot.HUB}/datasets/{self.manifest['repository']}/tree/{COMMIT}"
        report = {**self.base_report(), "trades": 2, "quotes": 0, "deltas": 0, "closes": 0,
                  "source_reference": reference, "source_observed_at": CLOCK, "imported_at": CLOCK}
        evidence = {"schema_version": 1, "source_reference": reference, "source_observed_at": CLOCK,
            "instruments": [{"original": "test fixture"}], "trades": [{}, {}],
            "quotes": [], "deltas": [], "bars": [{}, {}], "closes": [],
            "source_metadata": {"snapshot": deepcopy(self.manifest),
                "format": "lokima-dual-capture" if capture else self.archive.format,
                "selection": {"start_seconds": self.capture.start_seconds,
                    "end_seconds": self.capture.end_seconds, "bar_seconds": self.capture.bar_seconds},
                "market": {"slug": self.capture.market_slug}}}
        return report, evidence

    def publish(self, report, evidence=None):
        self.output.mkdir()
        catalog = self.output / "catalog"
        catalog.mkdir()
        # Only publication framing is checked here. Native Rust tests own actual Parquet roundtrips.
        (catalog / "fixture.parquet").write_bytes(b"PAR1fixture-footerPAR1")
        (self.output / "source-evidence.json").write_text(json.dumps(evidence or self.candle_evidence()))
        (self.output / "import-report.json").write_text(json.dumps(report))

    def runner(self, report=None, evidence=None, change=None, returncode=0):
        def run(argv, **kwargs):
            self.assertIs(type(argv), list)
            self.assertEqual(argv[0], str(self.binary))
            self.assertEqual(argv[-2:], ["--output", str(self.output)])
            self.assertIs(kwargs["shell"], False)
            self.assertIs(kwargs["check"], False)
            self.assertEqual(kwargs["stdin"], subprocess.DEVNULL)
            if report is not None:
                self.publish(report, evidence)
            if change:
                change()
            return SimpleNamespace(returncode=returncode)
        return run

    def test_coinbase_requires_verify_then_exact_safe_native_argv_and_report(self):
        report = self.candle_report()
        with patch.object(acquire, "verify", wraps=acquire.verify) as verify, \
                patch.object(plugins.subprocess, "run", side_effect=self.runner(report)) as run:
            result = plugins.candle_convert(self.candles)
        self.assertEqual(verify.call_count, 2)
        self.assertEqual(run.call_args.args[0], [str(self.binary), "ingest-candles", "--acquisition",
            str(self.candles.acquisition), "--instruments", str(self.instruments), "--output", str(self.output)])
        self.assertEqual(result["status"], "NATIVE_ARTIFACTS_VALIDATED")
        self.assertFalse(result["admission"]["research_qualified"])
        self.assertEqual(result["native_report"], report)

    def test_tampered_acquisition_never_starts_native(self):
        (self.acquisition / "records.jsonl").write_text("changed")
        with patch.object(plugins.subprocess, "run") as run, self.assertRaises(ValueError):
            plugins.candle_convert(self.candles)
        run.assert_not_called()
        self.assertFalse(self.output.exists())

    def test_snapshot_hash_budget_and_symlinks_fail_before_native(self):
        original = (self.snapshot_dir / "original.parquet").read_bytes()
        for mode in ("tampered", "budget", "symlink"):
            with self.subTest(mode=mode):
                if mode == "tampered":
                    (self.snapshot_dir / "original.parquet").write_bytes(b"changed")
                elif mode == "budget":
                    self.capture.max_bytes = 1
                else:
                    path = self.snapshot_dir / "original.parquet"
                    path.unlink()
                    path.symlink_to(self.terms)
                with patch.object(plugins.subprocess, "run") as run, self.assertRaises(ValueError):
                    plugins.history_convert(self.capture, capture=True)
                run.assert_not_called()
                path = self.snapshot_dir / "original.parquet"
                path.unlink()
                path.write_bytes(original)
                self.capture.max_bytes = 1024

    def test_capture_uses_native_adapter_and_preserves_original_selection(self):
        report, evidence = self.history_artifacts()
        with patch.object(plugins.subprocess, "run", side_effect=self.runner(report, evidence)) as run:
            result = plugins.history_convert(self.capture, capture=True)
        self.assertEqual(run.call_args.args[0], [str(self.binary), "capture", "--snapshot", str(self.snapshot_path),
            "--start-seconds", "1785357600", "--end-seconds", "1785357900", "--bar-seconds", "1",
            "--market-slug", self.capture.market_slug, "--output", str(self.output)])
        self.assertEqual(result["plugin"], "polymarket-capture")
        self.assertFalse(result["admission"]["registered_in_quazonai"])

    def test_existing_archive_formats_share_native_dispatch(self):
        for format_name in plugins.ARCHIVE_FORMATS:
            self.output = self.root / format_name
            self.archive = SimpleNamespace(**vars(self.capture))
            self.archive.output = self.output
            self.archive.format = format_name
            self.archive.instruments = self.instruments
            self.archive.chain_evidence = None
            report, evidence = self.history_artifacts(capture=False)
            with patch.object(plugins.subprocess, "run", side_effect=self.runner(report, evidence)) as run:
                self.assertEqual(plugins.history_convert(self.archive)["plugin"], "polymarket-archive")
            argv = run.call_args.args[0]
            self.assertEqual(argv[1], "archive")
            self.assertEqual(argv[argv.index("--format") + 1], format_name)
            self.assertEqual(argv[argv.index("--instruments") + 1], str(self.instruments))

    def test_source_changed_during_native_conversion_is_not_success(self):
        report = self.candle_report()
        with patch.object(plugins.subprocess, "run", side_effect=self.runner(report,
                change=lambda: self.instruments.write_text("changed"))), self.assertRaisesRegex(ValueError, "inputs changed"):
            plugins.candle_convert(self.candles)
        self.assertTrue((self.output / "import-report.json").exists())

    def test_snapshot_files_are_rechecked_after_native(self):
        report, evidence = self.history_artifacts()
        with patch.object(plugins.subprocess, "run", side_effect=self.runner(report, evidence,
                change=lambda: (self.snapshot_dir / "original.parquet").write_bytes(b"changed"))), \
                self.assertRaises(ValueError):
            plugins.history_convert(self.capture, capture=True)
        self.assertTrue((self.output / "import-report.json").exists())

    def test_exit_zero_without_publication_is_failure_in_real_subprocess(self):
        with tempfile.TemporaryFile(mode="w+") as diagnostics, \
                patch.object(plugins.sys, "stderr", diagnostics), self.assertRaises(ValueError):
            plugins.candle_convert(self.candles)
        self.assertFalse(self.output.exists())

    def test_nonzero_and_timeout_leave_failed_artifacts(self):
        report = self.candle_report()
        with patch.object(plugins.subprocess, "run", side_effect=self.runner(report, returncode=7)), \
                self.assertRaisesRegex(ValueError, "exited 7"):
            plugins.candle_convert(self.candles)
        self.assertTrue((self.output / "import-report.json").exists())
        self.output = self.root / "timed out"
        self.candles.output = self.output
        def timed_out(*args, **kwargs):
            self.output.mkdir()
            (self.output / "partial").write_text("keep original interrupted material")
            raise subprocess.TimeoutExpired(args[0], 3600)
        with patch.object(plugins.subprocess, "run", side_effect=timed_out), self.assertRaises(subprocess.TimeoutExpired):
            plugins.candle_convert(self.candles)
        self.assertTrue((self.output / "partial").exists())

    def test_reused_output_and_output_symlink_do_not_launch(self):
        self.output.mkdir()
        (self.output / "keep").write_text("original")
        with patch.object(plugins.subprocess, "run") as run, self.assertRaisesRegex(ValueError, "new directory"):
            plugins.candle_convert(self.candles)
        run.assert_not_called()
        self.assertEqual((self.output / "keep").read_text(), "original")
        self.candles.output = self.root / "linked"
        self.candles.output.symlink_to(self.output, target_is_directory=True)
        with patch.object(plugins.subprocess, "run") as run, self.assertRaisesRegex(ValueError, "symlinks"):
            plugins.candle_convert(self.candles)
        run.assert_not_called()

    def test_native_report_must_match_hashes_counts_and_admission(self):
        mutations = [{"acquisition_sha256": "0" * 64}, {"instrument_definitions_sha256": "0" * 64},
            {"bars": 3}, {"bars": True}, {"native_readback_verified": False}, {"research_qualified": True},
            {"source_provider": "polymarket-prices"}, {"source_record_kind": "PRICE_MARK"},
            {"coverage": "COMPLETE"}, {"registered_in_quazonai": True}, {"schema_version": True},
            {"historical_availability": "VERIFIED"}, {"catalog_relative_path": "../outside"},
            {"limitations": []}, {"instruments": 0}]
        for index, changes in enumerate(mutations):
            self.output = self.root / f"invalid-{index}"
            self.candles.output = self.output
            with self.subTest(changes=changes), patch.object(plugins.subprocess, "run",
                    side_effect=self.runner(self.candle_report() | changes)), self.assertRaises(ValueError):
                plugins.candle_convert(self.candles)

    def test_candle_evidence_must_retain_original_inputs_and_hashes(self):
        mutations = [lambda e: e.update(acquisition_sha256="0" * 64),
            lambda e: e.update(instrument_definitions_sha256="0" * 64),
            lambda e: e.update(source_acquisition_path="other"),
            lambda e: e["acquisition"].update(created_at="2000-01-01T00:00:00Z"),
            lambda e: e.update(instrument_definitions=[])]
        for index, change in enumerate(mutations):
            self.output = self.root / f"invalid-evidence-{index}"
            self.candles.output = self.output
            evidence = self.candle_evidence()
            change(evidence)
            with self.subTest(index=index), patch.object(plugins.subprocess, "run",
                    side_effect=self.runner(self.candle_report(), evidence)), self.assertRaises(ValueError):
                plugins.candle_convert(self.candles)

    def test_history_report_must_match_original_format_market_selection_and_counts(self):
        mutations = [lambda r, e: r.update(source_reference="other"),
            lambda r, e: e["source_metadata"].update(format="invented"),
            lambda r, e: e["source_metadata"]["selection"].update(start_seconds=0),
            lambda r, e: e["source_metadata"]["snapshot"].update(revision="b" * 40),
            lambda r, e: e["source_metadata"]["market"].update(slug="other"),
            lambda r, e: r.update(trades=1), lambda r, e: r.update(instrument_versions=2)]
        for index, change in enumerate(mutations):
            self.output = self.root / f"invalid-history-{index}"
            self.capture.output = self.output
            report, evidence = self.history_artifacts()
            change(report, evidence)
            with self.subTest(index=index), patch.object(plugins.subprocess, "run",
                    side_effect=self.runner(report, evidence)), self.assertRaises(ValueError):
                plugins.history_convert(self.capture, capture=True)

    def test_missing_evidence_partial_parquet_and_catalog_symlinks_are_rejected(self):
        changes = [lambda: (self.output / "source-evidence.json").unlink(),
            lambda: (self.output / "catalog/fixture.parquet").write_bytes(b"PAR1partial"),
            lambda: (self.output / "catalog/extra").symlink_to(self.root),
            lambda: (self.output / "catalog/fixture.parquet").unlink()]
        for index, change in enumerate(changes):
            self.output = self.root / f"invalid-artifact-{index}"
            self.candles.output = self.output
            with self.subTest(index=index), patch.object(plugins.subprocess, "run",
                    side_effect=self.runner(self.candle_report(), change=change)), \
                    self.assertRaises((OSError, ValueError)):
                plugins.candle_convert(self.candles)

    def test_duplicate_json_fields_and_unsafe_snapshot_identities_are_rejected(self):
        for changes in ({"schema_version": True}, {"repository": "../source"}, {"revision": "main"},
                        {"license": ""}, {"retrieved_at": "9999-01-01T00:00:00Z"}):
            self.snapshot_path.write_text(json.dumps(self.manifest | changes))
            with self.assertRaises(ValueError):
                plugins.verified_snapshot(self.snapshot_path, 1024)
        body = json.dumps(self.manifest)
        self.snapshot_path.write_text('{"schema_version":1,' + body[1:])
        with self.assertRaisesRegex(ValueError, "duplicate JSON"):
            plugins.verified_snapshot(self.snapshot_path, 1024)

    def test_snapshot_verify_does_not_fetch_or_claim_source_authenticity(self):
        with patch.object(snapshot.urllib.request, "urlopen") as network:
            result = plugins.snapshot_verify(self.capture)
        network.assert_not_called()
        self.assertEqual(result["integrity"], "VERIFIED")
        self.assertEqual(result["revision"], COMMIT)
        self.assertEqual(result["admission"]["historical_availability"], "UNVERIFIED")


if __name__ == "__main__":
    unittest.main()
