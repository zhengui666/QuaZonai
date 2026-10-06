"""Offline orchestration checks; fixture native outputs do not establish actual native acceptance."""

import contextlib
from copy import deepcopy
import datetime
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

import hf_dataset
import snapshot
import source_plugins as plugins


COMMIT = "a" * 40
CLOCK = "2026-09-03T00:00:00Z"
START = 1788220800  # 2026-09-01T00:00:00Z
END = START + 86400


class HfBridgeTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.output = self.root / "native output"
        self.binary = self.root / "native executable"
        self.binary.write_text(f"#!{sys.executable}\n")
        self.binary.chmod(0o700)
        self.instruments = self.root / "original instruments.json"
        self.definitions = [{"id": "ORIGINAL_FIXTURE_TOKEN", "ts_event": 1, "ts_init": 2}]
        self.instruments.write_text(json.dumps(self.definitions, indent=2))
        path = "actual-indexed-path/fills.parquet"
        self.source_file = self.root / "cache/files" / path
        self.source_file.parent.mkdir(parents=True)
        self.original_source_bytes = b"PAR1offline-orchestration-fixturePAR1"
        self.source_file.write_bytes(self.original_source_bytes)
        file = {"path": path, "size": len(self.original_source_bytes), "format": "parquet",
                "url": snapshot.repository_file_url("example/history", COMMIT, path),
                "partition": {"markets": ["original-market"], "start_date": "2026-09-01",
                              "end_date": "2026-09-03", "format": "parquet"}}
        self.index_body = json.dumps({"schema": hf_dataset.INDEX_SCHEMA, "files": [
            {"path": path, **file["partition"]}]}).encode()
        self.plan = {"schema": hf_dataset.PLAN_SCHEMA, "repository": "example/history", "revision": COMMIT,
                     "requested_revision": "main", "license": None, "partition_index": {
                         "path": "partitions.json", "size": len(self.index_body),
                         "url": snapshot.repository_file_url("example/history", COMMIT, "partitions.json")},
                     "request": {"includes": [], "markets": ["original-market"],
                                 "start_date": "2026-09-01", "end_date": "2026-09-03"},
                     "selection_bounds": "[start_date,end_date)", "download_granularity": "FILE_PARTITION",
                     "coverage": "NOT_ASSERTED", "max_bytes": 1024, "total_bytes": file["size"], "files": [file]}
        self.manifest = hf_dataset.selection_result(self.plan, self.root / "cache",
            [{**file, "local_path": str(self.source_file), "cached": False, "resumed_bytes": 0,
              "validation": "PARQUET_ENVELOPE"}], CLOCK)
        self.selection = self.root / "selection.json"
        self.selection.write_bytes(hf_dataset.manifest_bytes(self.manifest))
        self.args = SimpleNamespace(selection=self.selection, instruments=self.instruments,
                                    start_seconds=START, end_seconds=END, format="moose-fills",
                                    bar_seconds=60, chain_evidence=None, native_bin=self.binary, output=self.output)

    def artifacts(self):
        reference = f"{snapshot.HUB}/datasets/example/history/tree/{COMMIT}"
        report = {"schema_version": 1, "native_version": "0.63.0", "instruments": 1, "instrument_versions": 1,
                  "bars": 2, "trades": 2, "quotes": 0, "deltas": 0, "closes": 0,
                  "catalog_relative_path": "catalog", "coverage": "UNPROVEN", "historical_availability": "UNVERIFIED",
                  "registered_in_quazonai": False, "limitations": ["Offline native output fixture"],
                  "source_reference": reference, "source_observed_at": CLOCK}
        metadata = {"selection_manifest": deepcopy(self.manifest), "format": self.args.format,
                    "selection": {"start_seconds": self.args.start_seconds, "end_seconds": self.args.end_seconds,
                                  "bar_seconds": self.args.bar_seconds},
                    "clock_basis": "REQUEST_SELECTION_AT_NOT_HISTORICAL_AVAILABILITY",
                    "instruments_bytes": self.instruments.stat().st_size,
                    "instruments_definitions": deepcopy(self.definitions)}
        evidence = {"source_reference": reference, "source_observed_at": CLOCK, "source_metadata": metadata,
                    "instruments": deepcopy(self.definitions), "trades": [{}, {}], "bars": [{}, {}],
                    "quotes": [], "deltas": [], "closes": []}
        return report, evidence

    def runner(self, report=None, evidence=None, change=None, returncode=0):
        if report is None:
            report, evidence = self.artifacts()
        def run(argv, **kwargs):
            output = Path(argv[argv.index("--output") + 1])
            (output / "catalog").mkdir(parents=True)
            (output / "catalog/fixture.parquet").write_bytes(b"PAR1offline-native-fixturePAR1")
            (output / "import-report.json").write_text(json.dumps(report))
            (output / "source-evidence.json").write_text(json.dumps(evidence))
            if change:
                change()
            return subprocess.CompletedProcess(argv, returncode)
        return run

    def test_convert_real_dispatch_preserves_request_definitions_and_uses_no_hash_or_network(self):
        with patch.object(plugins.subprocess, "run", side_effect=self.runner()) as run, \
                patch.object(snapshot.urllib.request, "urlopen") as network, \
                patch.object(plugins.hashlib, "sha256", side_effect=AssertionError("new path must not hash")), \
                patch.object(snapshot, "file_hash", side_effect=AssertionError("new path must not hash")):
            result = plugins.hf_history_convert(self.args)
        network.assert_not_called()
        argv = run.call_args.args[0]
        self.assertEqual(argv[:4], [str(self.binary), "archive", "--selection", str(self.selection)])
        self.assertNotIn("--snapshot", argv)
        self.assertEqual(argv[argv.index("--instruments") + 1], str(self.instruments))
        self.assertEqual(result["plugin"], "hf-dataset")
        self.assertEqual(result["status"], "NATIVE_ARTIFACTS_VALIDATED")
        self.assertFalse(result["admission"]["research_qualified"])
        self.assertEqual(self.source_file.read_bytes(), self.original_source_bytes)

    def test_sii_raw_format_dispatches_and_prepares_without_network_hashes_or_pit_claim(self):
        self.args.format = "sii-order-filled"
        with patch.object(plugins.subprocess, "run", side_effect=self.runner()) as run, \
                patch.object(snapshot.urllib.request, "urlopen") as network, \
                patch.object(plugins.hashlib, "sha256", side_effect=AssertionError("must not hash")):
            result = plugins.hf_history_convert(self.args)
        self.assertEqual(run.call_args.args[0][run.call_args.args[0].index("--format") + 1], "sii-order-filled")
        self.assertEqual(result["native_report"]["historical_availability"], "UNVERIFIED")
        network.assert_not_called()
        self.args.output = self.root / "second native"
        args, runner = self.prepare_fixture()
        with patch.object(plugins.subprocess, "run", side_effect=runner):
            self.assertEqual(plugins.prepare_source("hf-dataset", args)["status"], "CATALOG_PREPARED")

    def test_native_window_only_narrows_requested_half_open_dates(self):
        hf_dataset.native_window(self.manifest, START, START + 2 * 86400)
        hf_dataset.native_window(self.manifest, START + 60, END)
        for lower, upper in ((START - 1, END), (START, START + 2 * 86400 + 1), (END, START)):
            self.args.start_seconds, self.args.end_seconds = lower, upper
            with self.subTest(lower=lower, upper=upper), patch.object(plugins.subprocess, "run") as run, self.assertRaises(ValueError):
                plugins.hf_history_convert(self.args)
            run.assert_not_called()

    def test_explicit_files_require_no_inferred_date_or_market_mapping(self):
        self.manifest["plan"]["request"] = {"includes": ["actual-indexed-path/fills.parquet"], "markets": [],
                                               "start_date": None, "end_date": None}
        self.manifest["plan"]["partition_index"] = None
        for item in (self.manifest["plan"]["files"][0], self.manifest["files"][0]):
            item.pop("partition")
        self.selection.write_bytes(hf_dataset.manifest_bytes(self.manifest))
        self.args.start_seconds, self.args.end_seconds = START - 86400, END
        with patch.object(plugins.subprocess, "run", side_effect=self.runner()):
            self.assertEqual(plugins.hf_history_convert(self.args)["plugin"], "hf-dataset")

    def test_cache_path_size_format_or_record_mismatch_never_launches_native(self):
        original = deepcopy(self.manifest)
        for change in (lambda m: m["files"][0].update(local_path=str(self.root / "elsewhere")),
                       lambda m: m["files"][0].update(format="opaque"),
                       lambda m: m["files"][0].update(size=1)):
            self.manifest = deepcopy(original)
            change(self.manifest)
            self.selection.write_bytes(hf_dataset.manifest_bytes(self.manifest))
            with patch.object(plugins.subprocess, "run") as run, self.assertRaises(ValueError):
                plugins.hf_history_convert(self.args)
            run.assert_not_called()
        self.manifest = original
        self.selection.write_bytes(hf_dataset.manifest_bytes(self.manifest))
        self.source_file.write_bytes(b"short")
        with patch.object(plugins.subprocess, "run") as run, self.assertRaises(ValueError):
            plugins.hf_history_convert(self.args)
        run.assert_not_called()

    def test_source_or_definition_mutation_is_failure_and_original_outputs_are_retained(self):
        mutations = [lambda: self.source_file.write_bytes(self.original_source_bytes.replace(b"offline", b"changed")),
                     lambda: self.instruments.write_text("changed"),
                     lambda: self.selection.write_text("changed")]
        for index, change in enumerate(mutations):
            self.args.output = self.root / f"changed-{index}"
            report, evidence = self.artifacts()
            with patch.object(plugins.subprocess, "run", side_effect=self.runner(report, evidence, change=change)), \
                    self.assertRaises((OSError, ValueError)):
                plugins.hf_history_convert(self.args)
            self.assertTrue((self.args.output / "source-evidence.json").exists())
            self.source_file.write_bytes(self.original_source_bytes)
            self.instruments.write_text(json.dumps(self.definitions, indent=2))
            self.selection.write_bytes(hf_dataset.manifest_bytes(self.manifest))

    def test_report_request_clock_definitions_and_counts_must_match(self):
        mutations = [lambda r, e: e["source_metadata"]["selection_manifest"]["plan"]["request"].update(markets=["wrong"]),
                     lambda r, e: e["source_metadata"].update(clock_basis="HISTORICAL_PIT"),
                     lambda r, e: e["source_metadata"].update(instruments_definitions=[]),
                     lambda r, e: e["source_metadata"].update(instruments_bytes=1),
                     lambda r, e: e["source_metadata"].update(instruments_sha256="invented"),
                     lambda r, e: r.update(source_reference="wrong"),
                     lambda r, e: r.update(bars=1),
                     lambda r, e: e["source_metadata"]["selection"].update(start_seconds=0)]
        for index, change in enumerate(mutations):
            self.args.output = self.root / f"bad-report-{index}"
            report, evidence = self.artifacts()
            change(report, evidence)
            with patch.object(plugins.subprocess, "run", side_effect=self.runner(report, evidence)), self.assertRaises(ValueError):
                plugins.hf_history_convert(self.args)

    def test_original_scope_index_records_and_clocks_are_validated_before_native(self):
        original = deepcopy(self.manifest)
        changes = [lambda m: m["plan"].update(coverage="COMPLETE"),
                   lambda m: m["plan"].update(download_granularity="ANY"),
                   lambda m: m["plan"].update(selection_bounds="[start,end]"),
                   lambda m: m["plan"].update(partition_index=None),
                   lambda m: m.update(retrieved_at="9999-01-01T00:00:00Z"),
                   lambda m: m.update(cached_files=1),
                   lambda m: m["files"][0].update(validation="PIT_PROVEN")]
        for change in changes:
            candidate = deepcopy(original)
            change(candidate)
            self.selection.write_bytes(hf_dataset.manifest_bytes(candidate))
            with patch.object(plugins.subprocess, "run") as run, self.assertRaises(ValueError):
                plugins.hf_history_convert(self.args)
            run.assert_not_called()

    def test_equal_but_wrong_native_clocks_fail_convert_and_prepare(self):
        report, evidence = self.artifacts()
        report["source_observed_at"] = evidence["source_observed_at"] = "2000-01-01T00:00:00Z"
        with patch.object(plugins.subprocess, "run", side_effect=self.runner(report, evidence)), self.assertRaisesRegex(ValueError, "clock meaning"):
            plugins.hf_history_convert(self.args)
        self.args.output = self.root / "valid native"
        args, _ = self.prepare_fixture()
        original_report = json.loads((args.native_output / "import-report.json").read_text())
        original_evidence = json.loads((args.native_output / "source-evidence.json").read_text())
        original_report["source_observed_at"] = original_evidence["source_observed_at"] = "2000-01-01T00:00:00Z"
        (args.native_output / "import-report.json").write_text(json.dumps(original_report))
        (args.native_output / "source-evidence.json").write_text(json.dumps(original_evidence))
        with patch.object(plugins.subprocess, "run") as run, self.assertRaisesRegex(ValueError, "clock meaning"):
            plugins.prepare_source("hf-dataset", args)
        run.assert_not_called()

    def test_chain_observation_can_be_later_without_claiming_historical_pit(self):
        self.args.format = "time-seventeen-v2"
        report, evidence = self.artifacts()
        report["source_observed_at"] = evidence["source_observed_at"] = "2026-09-04T00:00:00Z"
        original_chain = {"retrieved_at": "2026-09-04T00:00:00Z", "origin": "ORCHESTRATION_FIXTURE"}
        evidence["source_metadata"]["chain_evidence"] = {"snapshot": original_chain}
        chain = self.root / "original chain.json"
        chain.write_text(json.dumps(original_chain))
        self.args.chain_evidence = chain
        with patch.object(plugins.subprocess, "run", side_effect=self.runner(report, evidence)) as run:
            result = plugins.hf_history_convert(self.args)
        self.assertIn("--chain-evidence", run.call_args.args[0])
        self.assertEqual(result["native_report"]["historical_availability"], "UNVERIFIED")

    def prepare_fixture(self):
        with patch.object(plugins.subprocess, "run", side_effect=self.runner()):
            plugins.hf_history_convert(self.args)
        declaration = self.root / "catalog declaration.json"
        declared = {"schema_version": 1, "registered_ref": "fixture:hf-test", "storage_version": "original",
                    "origin": "FIXTURE", "pit_status": "UNVERIFIED"}
        declaration.write_text(json.dumps(declared))
        selection = self.root / "native dataset selection.json"
        selection.write_text('{"selection":{"decision_cutoff_ns":123456789},"settlements":[]}')
        args = SimpleNamespace(native_output=self.args.output, declaration=declaration, selection=selection,
                               output=self.root / "prepared catalog", native_bin=self.binary)
        def run(argv, **kwargs):
            (args.output / "catalog").mkdir(parents=True)
            (args.output / "catalog/fixture.parquet").write_bytes(b"PAR1prepared-fixturePAR1")
            (args.output / "catalog-metadata.json").write_text(json.dumps(declared | {"row_count": 2}))
            return subprocess.CompletedProcess(argv, 0)
        return args, run

    def test_prepare_reuses_native_catalog_without_hash_cache_download_or_registration(self):
        args, runner = self.prepare_fixture()
        self.source_file.unlink()  # Preparation consumes the published native catalog, not the raw cache again.
        with patch.object(plugins.subprocess, "run", side_effect=runner), \
                patch.object(snapshot.urllib.request, "urlopen") as network, \
                patch.object(plugins.hashlib, "sha256", side_effect=AssertionError("new path must not hash")):
            result = plugins.prepare_source("hf-dataset", args)
        network.assert_not_called()
        self.assertEqual(result["status"], "CATALOG_PREPARED")
        self.assertNotIn("sha256", json.dumps(result))
        self.assertEqual(result["catalog_registration"]["metadata_file"], str(args.output / "catalog-metadata.json"))
        self.assertFalse(result["admission"]["registered_in_quazonai"])
        self.assertIn("fresh_DATA_VALIDATE", result["unperformed_steps"])

    def test_prepare_detects_native_catalog_mutation_and_keeps_failed_artifacts(self):
        args, runner = self.prepare_fixture()
        def changed(argv, **kwargs):
            result = runner(argv, **kwargs)
            (args.native_output / "catalog/fixture.parquet").write_bytes(b"PAR1changed-native-fixturePAR1")
            return result
        with patch.object(plugins.subprocess, "run", side_effect=changed), self.assertRaisesRegex(ValueError, "inputs changed"):
            plugins.prepare_source("hf-dataset", args)
        self.assertTrue((args.output / "catalog-metadata.json").exists())

    def test_prepare_rejects_books_only_source_or_fake_clock_basis_before_native(self):
        args, _ = self.prepare_fixture()
        evidence = json.loads((args.native_output / "source-evidence.json").read_text())
        for field, value in (("format", "joseph-books"), ("clock_basis", "PIT_PROVEN")):
            changed = deepcopy(evidence)
            changed["source_metadata"][field] = value
            (args.native_output / "source-evidence.json").write_text(json.dumps(changed))
            with patch.object(plugins.subprocess, "run") as run, self.assertRaises(ValueError):
                plugins.prepare_source("hf-dataset", args)
            run.assert_not_called()

    def test_parser_and_plugin_expose_actual_offline_bridge_capabilities(self):
        descriptor = next(p for p in plugins.plugin_descriptors() if p["id"] == "hf-dataset")
        self.assertEqual(descriptor["public_network_operations"], ["download", "plan"])
        argv = ["convert", "hf-dataset", "--selection", str(self.selection), "--format", "moose-fills",
                "--instruments", str(self.instruments), "--start-seconds", str(START), "--end-seconds", str(END),
                "--bar-seconds", "60", "--native-bin", str(self.binary), "--output", str(self.output)]
        with patch.object(plugins.subprocess, "run", side_effect=self.runner()), \
                contextlib.redirect_stdout(io.StringIO()) as output:
            self.assertEqual(plugins.main(argv), 0)
        self.assertEqual(json.loads(output.getvalue())["plugin"], "hf-dataset")


if __name__ == "__main__":
    unittest.main()
