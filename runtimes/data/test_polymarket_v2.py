"""Source preset regressions. Envelope/native stubs are not live-source acceptance."""

import contextlib
from copy import deepcopy
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

import hf_dataset
import polymarket_v2 as source
import snapshot
import source_plugins as plugins


COMMIT = "a" * 40
TOKEN = "102936224134271070189104847090829839924697394514566827387181305960175107677216"
ENVELOPE = b"PAR1test-envelope-not-decoded-by-PythonPAR1"
START = 1788220800  # 2026-09-01T00:00:00Z
END = START + 86400


class PolymarketV2Test(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.cache = self.root / "cache"
        self.plan_path = self.root / "original-plan.json"
        self.request_dir = self.root / "request"
        self.binary = self.root / "native executable"
        self.binary.write_text(f"#!{sys.executable}\n")
        self.binary.chmod(0o700)
        self.originals = [{"BinaryOption": {"id": f"condition-{TOKEN}.POLYMARKET", "raw_symbol": TOKEN,
                           "currency": "pUSD", "ts_event": 1, "ts_init": 2}}]
        self.instruments = self.root / "original-instruments.json"
        self.instruments.write_text(json.dumps(self.originals))

    def metadata(self, days=("2026_09_01", "2026_09_02"), size=len(ENVELOPE)):
        return COMMIT, {f"OrderFilled/{day}.parquet": {"size": size} for day in days}, {
            "cardData": {"license": "cc-by-4.0"}}, None

    def plan(self, end="2026-09-02"):
        with patch.object(snapshot, "repository_metadata", return_value=self.metadata()):
            result = source.plan("2026-09-01", end, [TOKEN], "original-tag")
        source.publish(self.plan_path, result)
        return result

    def selection(self, end="2026-09-02"):
        plan = self.plan(end)
        for item in plan["files"]:
            file = hf_dataset.cache_root(self.cache, plan) / "files" / item["path"]
            file.parent.mkdir(parents=True, exist_ok=True)
            file.write_bytes(ENVELOPE)
        with patch.object(snapshot, "repository_metadata", side_effect=AssertionError("offline")), \
                patch.object(snapshot.urllib.request, "urlopen", side_effect=AssertionError("offline")):
            result = hf_dataset.freeze(self.plan_path, self.cache, self.request_dir)
        return result

    def args(self, operation, *values):
        return plugins.parser().parse_args([operation, source.PLUGIN, *map(str, values)])

    def convert_args(self, *extra):
        return self.args("convert", "--selection", self.request_dir / "selection.json",
                         "--instruments", self.instruments, "--bar-seconds", "60",
                         "--native-bin", self.binary, "--output", self.root / "native", *extra)

    def native_runner(self, returncode=0, changed=None):
        def run(argv, **kwargs):
            output = Path(argv[argv.index("--output") + 1])
            (output / "catalog").mkdir(parents=True)
            (output / "catalog/fixture.parquet").write_bytes(ENVELOPE)
            selected = snapshot.fetch_local_manifest(Path(argv[argv.index("--selection") + 1]))
            definitions = json.loads(Path(argv[argv.index("--instruments") + 1]).read_text())
            reference = f"{snapshot.HUB}/datasets/{source.REPOSITORY}/tree/{COMMIT}"
            report = {"schema_version": 1, "native_version": "0.63.0", "instruments": 1,
                      "instrument_versions": 1, "bars": 2, "trades": 2, "quotes": 0, "deltas": 0, "closes": 0,
                      "catalog_relative_path": "catalog", "coverage": "UNPROVEN",
                      "historical_availability": "UNVERIFIED", "registered_in_quazonai": False,
                      "limitations": ["Offline native orchestration fixture, not decoded data"],
                      "source_reference": reference, "source_observed_at": selected["retrieved_at"]}
            metadata = {"format": source.FORMAT, "selection_manifest": selected,
                        "clock_basis": "REQUEST_SELECTION_AT_NOT_HISTORICAL_AVAILABILITY",
                        "instruments_definitions": definitions, "instruments_bytes": self.instruments.stat().st_size,
                        "selection": {"start_seconds": int(argv[argv.index("--start-seconds") + 1]),
                                      "end_seconds": int(argv[argv.index("--end-seconds") + 1]), "bar_seconds": 60}}
            evidence = {"source_reference": reference, "source_observed_at": report["source_observed_at"],
                        "source_metadata": metadata, "instruments": definitions,
                        "trades": [{}, {}], "bars": [{}, {}], "quotes": [], "deltas": [], "closes": []}
            (output / "import-report.json").write_text(json.dumps(report))
            (output / "source-evidence.json").write_text(json.dumps(evidence))
            if changed:
                changed()
            return subprocess.CompletedProcess(argv, returncode)
        return run

    def test_native_definition_lineage_keeps_decimal_and_u64_values_exact(self):
        original = plugins.providers.read_json(b'{"ts_init":1788220800000000001,"rate":0.000000000000000001,"name":"literal \\u4e2d","null":null}')
        encoded = source.definition_json(original)
        self.assertIn('1788220800000000001', encoded)
        self.assertEqual(plugins.providers.read_json(encoded), original)
        self.assertNotIn('"1E-18"', encoded)

    def test_registered_source_exposes_only_actual_network_operations(self):
        inventory = {item["id"]: item for item in plugins.plugin_descriptors()}
        self.assertIn(source.PLUGIN, inventory)
        descriptor = inventory[source.PLUGIN]
        self.assertEqual(descriptor["public_network_operations"], ["download", "plan"])
        self.assertEqual(descriptor["capabilities"],
                         ["convert", "download", "export-dataset", "freeze", "plan", "prepare", "verify"])
        self.assertFalse(descriptor["admission"]["research_qualified"])
        self.assertFalse(descriptor["admission"]["registered_in_quazonai"])

    def test_exact_date_plan_preserves_intent_without_remote_market_claims(self):
        args = self.args("plan", "--start-date", "2026-09-01", "--end-date", "2026-09-03", "--token", TOKEN,
                         "--revision", "original-tag", "--output", self.plan_path)
        with patch.object(snapshot, "repository_metadata", return_value=self.metadata()) as metadata:
            result = args.run(args)
        metadata.assert_called_once_with(source.REPOSITORY, "original-tag")
        self.assertEqual(result["revision"], COMMIT)
        self.assertEqual(result["license"], "cc-by-4.0")
        self.assertEqual(result["request"], {"includes": ["OrderFilled/2026_09_01.parquet", "OrderFilled/2026_09_02.parquet"],
                                           "markets": [], "start_date": None, "end_date": None})
        self.assertIsNone(result["partition_index"])
        self.assertEqual(result["polymarket_v2"]["tokens"], [TOKEN])
        self.assertEqual(result["coverage"], "NOT_ASSERTED")
        self.assertEqual(snapshot.fetch_local_manifest(self.plan_path), result)

    def test_leap_year_and_month_boundaries_are_half_open(self):
        request = source.source_request("2024-02-28", "2024-03-02", ["123"])
        self.assertEqual(source.files(request), ["OrderFilled/2024_02_28.parquet", "OrderFilled/2024_02_29.parquet",
                                                 "OrderFilled/2024_03_01.parquet"])

    def test_invalid_dates_and_noncanonical_tokens_fail(self):
        for tokens in ([], [TOKEN, TOKEN], ["0"], ["01"], [123], [{}], [str(1 << 256)], ["*"], ["../token"]):
            with self.subTest(tokens=tokens), self.assertRaises(ValueError):
                source.source_request("2026-09-01", "2026-09-02", tokens)
        for start, end in (("2026-02-29", "2026-03-02"), ("2026-09-02", "2026-09-01"),
                           ("2026-09-01", "2026-09-01"), (None, "2026-09-02")):
            with self.subTest(start=start), self.assertRaises(ValueError):
                source.source_request(start, end, [TOKEN])

    def test_missing_day_or_whole_file_budget_never_shrinks_a_request(self):
        for metadata, budget in ((self.metadata(days=("2026_09_01",)), None), (self.metadata(), len(ENVELOPE))):
            with patch.object(snapshot, "repository_metadata", return_value=metadata), self.assertRaises(ValueError):
                source.plan("2026-09-01", "2026-09-03", [TOKEN], COMMIT, budget)
        with patch.object(snapshot, "repository_metadata", return_value=self.metadata(size=6_000_000_000)):
            result = source.plan("2026-09-01", "2026-09-03", [TOKEN], COMMIT)
        self.assertEqual(result["total_bytes"], 12_000_000_000)
        self.assertIsNone(result["max_bytes"])

    def test_existing_or_symlinked_plan_never_overwrites_or_fetches(self):
        self.plan_path.write_text("original user file")
        args = self.args("plan", "--start-date", "2026-09-01", "--end-date", "2026-09-02", "--token", TOKEN,
                         "--output", self.plan_path)
        with patch.object(snapshot, "repository_metadata") as network, self.assertRaises(ValueError):
            args.run(args)
        network.assert_not_called()
        self.assertEqual(self.plan_path.read_text(), "original user file")
        self.plan_path.unlink()
        self.plan_path.symlink_to(self.root / "not-created")
        with self.assertRaises(ValueError):
            source.publish(self.plan_path, {})

    def test_wrong_repository_dates_derived_layers_or_tampered_intent_reject(self):
        original = self.plan()
        changes = [lambda p: p.update(repository="example/other"),
                   lambda p: p["polymarket_v2"].update(schema="unknown"),
                   lambda p: p["polymarket_v2"].update(end_date="2026-09-03"),
                   lambda p: p["polymarket_v2"].update(tokens=[TOKEN, TOKEN]),
                   lambda p: p["request"].update(markets=["ALL"]),
                   lambda p: p["request"].update(includes=["daily_aligned/2026_09_01.parquet"])]
        for change in changes:
            candidate = deepcopy(original)
            change(candidate)
            with self.subTest(change=change), self.assertRaises(ValueError):
                source.validate_plan(candidate)

    def test_saved_plan_download_reuses_cache_without_resolving_mutable_refs(self):
        selected = self.selection()
        args = self.args("download", "--plan", self.plan_path, "--cache-dir", self.cache, "--output", self.root / "download")
        with patch.object(snapshot, "repository_metadata", side_effect=AssertionError("must not resolve refs")), \
                patch.object(snapshot.urllib.request, "urlopen", side_effect=AssertionError("cached")):
            result = args.run(args)
            self.assertEqual(args.run(args), result)
        self.assertEqual(result["plan"], selected["plan"])
        self.assertEqual(result["downloaded_bytes"], 0)
        self.assertEqual(result["cached_files"], 1)
        changed = deepcopy(result["plan"])
        changed["polymarket_v2"]["tokens"] = ["123"]
        with self.assertRaises(ValueError):
            hf_dataset.download(changed, self.cache, args.output)

    def test_freeze_and_verify_are_offline_and_missing_source_does_not_publish(self):
        selected = self.selection()
        args = self.args("verify", "--selection", self.request_dir / "selection.json")
        with patch.object(snapshot.urllib.request, "urlopen", side_effect=AssertionError("offline")), \
                patch.object(snapshot, "file_hash", side_effect=AssertionError("no new hashes")):
            self.assertEqual(args.run(args)["revision"], COMMIT)
        Path(selected["files"][0]["local_path"]).unlink()
        frozen = self.args("freeze", "--plan", self.plan_path, "--cache-dir", self.cache, "--output", self.root / "missing")
        with patch.object(snapshot, "acquire_cached_file", side_effect=AssertionError("offline")), self.assertRaises(ValueError):
            frozen.run(frozen)
        self.assertFalse((frozen.output / "selection.json").exists())

    def test_conversion_reuses_original_native_bridge_and_preserves_all_definitions(self):
        self.selection()
        args = self.convert_args()
        with patch.object(plugins.subprocess, "run", side_effect=self.native_runner()) as run, \
                patch.object(snapshot.urllib.request, "urlopen", side_effect=AssertionError("offline")), \
                patch.object(snapshot, "file_hash", side_effect=AssertionError("no new hashes")):
            result = args.run(args)
        argv = run.call_args.args[0]
        self.assertEqual(argv[argv.index("--format") + 1], source.FORMAT)
        self.assertEqual(argv[argv.index("--start-seconds") + 1], str(START))
        self.assertEqual(argv[argv.index("--end-seconds") + 1], str(END))
        self.assertEqual(argv[argv.index("--instruments") + 1], str(self.instruments))
        self.assertEqual(result["plugin"], "hf-dataset")
        self.assertEqual(result["source_preset"], source.PLUGIN)
        self.assertEqual(json.loads(self.instruments.read_text()), self.originals)
        self.assertFalse(result["admission"]["research_qualified"])

    def test_native_window_may_narrow_never_widen_preserved_utc_dates(self):
        request = source.source_request("2026-09-01", "2026-09-02", [TOKEN])
        self.assertEqual(source.window(request, START + 60, END - 60), (START + 60, END - 60))
        for start, end in ((START - 1, END), (START, END + 1), (None, END), (END, START)):
            with self.subTest(start=start, end=end), self.assertRaises(ValueError):
                source.window(request, start, end)

    def test_wrong_token_currency_or_duplicate_definition_fails_before_native(self):
        self.selection()
        args = self.convert_args()
        changes = [lambda d: d[0]["BinaryOption"].update(raw_symbol="123"),
                   lambda d: d[0]["BinaryOption"].update(currency="USDC.e"),
                   lambda d: d.append(deepcopy(d[0])), lambda d: d[0].update(CurrencyPair={})]
        for change in changes:
            definitions = deepcopy(self.originals)
            change(definitions)
            self.instruments.write_text(json.dumps(definitions))
            with patch.object(plugins.subprocess, "run") as run, self.assertRaises(ValueError):
                args.run(args)
            run.assert_not_called()
            self.assertFalse(args.output.exists())

    def test_native_failure_and_input_mutation_retain_artifacts_without_success(self):
        self.selection()
        args = self.convert_args()
        with patch.object(plugins.subprocess, "run", side_effect=self.native_runner(returncode=1)), self.assertRaises(ValueError):
            args.run(args)
        self.assertTrue((args.output / "catalog").exists())
        args.output = self.root / "changed-native"
        with patch.object(plugins.subprocess, "run", side_effect=self.native_runner(
                changed=lambda: self.instruments.write_text("changed input"))), self.assertRaises(ValueError):
            args.run(args)
        self.assertTrue((args.output / "source-evidence.json").exists())

    def preparation(self):
        selected = self.selection()
        conversion = self.convert_args()
        with patch.object(plugins.subprocess, "run", side_effect=self.native_runner()):
            conversion.run(conversion)
        self.declaration = self.root / "declaration.json"
        self.declaration.write_text(json.dumps({"schema_version": 1, "registered_ref": "fixture:v2",
            "storage_version": "original", "origin": "FIXTURE", "pit_status": "UNVERIFIED"}))
        self.native_selection = self.root / "native-selection.json"
        self.native_selection.write_text(json.dumps({"dataset_revision_id": "original-fixture-id",
            "selection": {"schema_version": 1, "bar_types": [f"condition-{TOKEN}.POLYMARKET-1-MINUTE-LAST-EXTERNAL"],
                "event_start_ns": str(START * 1_000_000_000), "event_end_ns": str(END * 1_000_000_000),
                "decision_cutoff_ns": "1800000000000000000", "maximum_rows": 100}, "settlements": []}))
        args = self.args("prepare", "--native-output", conversion.output, "--declaration", self.declaration,
            "--selection", self.native_selection, "--native-bin", self.binary, "--output", self.root / "prepared")
        return args, selected

    def prepare_runner(self, args):
        def run(argv, **kwargs):
            (args.output / "catalog").mkdir(parents=True)
            (args.output / "catalog/fixture.parquet").write_bytes(ENVELOPE)
            metadata = json.loads(self.declaration.read_text()) | {"row_count": "2"}
            (args.output / "catalog-metadata.json").write_text(json.dumps(metadata))
            return subprocess.CompletedProcess(argv, 0)
        return run

    def test_preparation_saves_current_service_handoff_without_raw_cache_or_hashes(self):
        args, selected = self.preparation()
        Path(selected["files"][0]["local_path"]).unlink()
        with patch.object(plugins.subprocess, "run", side_effect=self.prepare_runner(args)), \
                patch.object(snapshot.urllib.request, "urlopen", side_effect=AssertionError("offline")), \
                patch.object(plugins.hashlib, "sha256", side_effect=AssertionError("HF branch does not hash")), \
                patch.object(plugins, "published_native", wraps=plugins.published_native) as publication:
            result = args.run(args)
        self.assertEqual(publication.call_count, 1)
        self.assertEqual(result, snapshot.fetch_local_manifest(args.output / "preparation.json"))
        self.assertEqual(result["plugin"], "hf-dataset")
        self.assertEqual(result["source_preset"], source.PLUGIN)
        self.assertEqual(result["source_request"]["tokens"], [TOKEN])
        self.assertEqual(json.loads(result["source_definitions_json"]), self.originals)
        self.assertEqual(result["native_selection"], json.loads(self.native_selection.read_text()))
        self.assertNotIn("sha256", json.dumps(result))
        self.assertNotIn("runtime_configuration", result["unperformed_steps"])
        self.assertIn("service_dataset_configuration", result["unperformed_steps"])

    def test_preparation_requires_preset_even_when_generic_hf_publication_is_valid(self):
        args, _ = self.preparation()
        path = args.native_output / "source-evidence.json"
        evidence = json.loads(path.read_text())
        del evidence["source_metadata"]["selection_manifest"]["plan"][source.REQUEST_KEY]
        path.write_text(json.dumps(evidence))
        with patch.object(plugins.subprocess, "run") as run, self.assertRaises(ValueError):
            args.run(args)
        run.assert_not_called()
        self.assertFalse(args.output.exists())

    def test_archive_pit_promotion_rejects_even_through_generic_hf_prepare(self):
        args, _ = self.preparation()
        declaration = json.loads(self.declaration.read_text())
        declaration["pit_status"] = "VERIFIED"
        self.declaration.write_text(json.dumps(declaration))
        for run_prepare in (lambda: args.run(args), lambda: plugins.prepare_source("hf-dataset", args)):
            with patch.object(plugins.subprocess, "run") as run, self.assertRaisesRegex(ValueError, "UNVERIFIED"):
                run_prepare()
            run.assert_not_called()
            self.assertFalse(args.output.exists())

    def test_export_registry_uses_fixed_typescript_entrypoint_and_no_provider_environment(self):
        args = self.args("export-dataset", "--prepared", "relative/preparation.json", "--request", "relative/factor.json",
                         "--id", "original-id", "--name", "Original name", "--output", "relative/dataset.json", "--ready")
        result = {"status": "DATASET_CONFIG_EXPORTED"}
        with patch.dict(os.environ, {"QZ_TEST_PROVIDER_SECRET": "fixture-only-not-a-credential"}), \
                patch.object(plugins.subprocess, "run", return_value=subprocess.CompletedProcess([], 0, json.dumps(result).encode())) as run:
            self.assertEqual(args.run(args), result)
        argv = run.call_args.args[0]
        self.assertEqual(argv[:2], ["node", "--experimental-strip-types"])
        self.assertTrue(argv[2].endswith("apps/service/src/dataset-export.ts"))
        self.assertEqual(argv[argv.index("--request") + 1], str(Path("relative/factor.json").absolute()))
        self.assertNotIn("QZ_TEST_PROVIDER_SECRET", run.call_args.kwargs["env"])
        self.assertIs(run.call_args.kwargs["shell"], False)


if __name__ == "__main__":
    unittest.main()
