"""Offline regressions for full processing above former application ceilings.

These fixtures establish parser/transport behavior, not real-source qualification.
No external dataset, paid endpoint, or native runtime is used.
"""

from copy import deepcopy
import hashlib
import io
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import zipfile

import acquire
import binance_vision as vision
import evm
import hf_dataset
import providers
import snapshot
import source_plugins
import test_acquire as http_fixture
import test_binance_vision as archive_fixture
import test_evm as evm_fixture


MIB = 1024 * 1024


class UncappedDataTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)

    def test_all_129_supplier_pages_cover_the_original_selection(self):
        for name, instrument, seconds in (("coinbase-candles", "BTC-USD", 299 * 60),
                                          ("polymarket-prices", "123", 86400)):
            selection = providers.Selection(instrument, 0, 129 * seconds, 60)
            result = acquire.plan(name, selection)
            self.assertEqual(len(result["requests"]), 129)
            self.assertEqual(result["requests"][0]["start_seconds"], 0)
            self.assertEqual(result["requests"][-1]["end_seconds"], selection.end_seconds)
            self.assertTrue(all(left["end_seconds"] == right["start_seconds"]
                                for left, right in zip(result["requests"], result["requests"][1:])))
            self.assertEqual(set(result["limits"].values()), {None})
            budget = 129 * MIB
            self.assertEqual(acquire.plan(name, selection, budget)["limits"]["max_response_total_bytes"], budget)

    def test_all_100001_marks_and_exact_large_decimals_survive(self):
        count = 100_001
        body = json.dumps({"history": [{"t": n, "p": 1} for n in range(count)]}).encode()
        rows = providers.PolymarketPrices().decode(body, providers.Selection("123", 0, count, 60))
        accumulated = {}
        acquire.collect_rows(accumulated, rows)
        encoded = acquire.record_bytes(accumulated)
        self.assertEqual(len(encoded.splitlines()), count)
        self.assertEqual(json.loads(encoded.splitlines()[-1])["selection_time_seconds"], count - 1)
        with self.assertRaisesRegex(ValueError, "duplicate"):
            acquire.collect_rows(accumulated, [rows[-1]])
        values = providers.CoinbaseCandles().decode(b'[[0,1,3,2,2,1e-101]]',
                                                   providers.Selection("BTC-USD", 0, 60, 60))
        self.assertEqual(values[0]["volume"], "0." + "0" * 100 + "1")

    def test_public_http_reads_complete_body_above_4_mib_with_and_without_length(self):
        body = b'{"payload":"' + b"x" * (4 * MIB + 1) + b'"}'
        for declared in (True, False):
            headers = {"Content-Type": "application/json"}
            if declared:
                headers["Content-Length"] = str(len(body))
            response = http_fixture.Response(body, headers=headers)
            with patch.object(acquire.OPENER, "open", return_value=response):
                actual, _ = acquire.fetch("https://api.exchange.coinbase.com/test")
            self.assertEqual(actual, body)

    def test_terms_and_total_output_above_128_mib_are_preserved_and_verified(self):
        terms = self.root / "terms.txt"
        with terms.open("wb") as stream:
            for _ in range(129):
                stream.write(b"t" * MIB)
        output = self.root / "acquisition"
        selection = providers.Selection("BTC-USD", 0, 60, 60)
        body = b"[" + http_fixture.CANDLE + b"]"
        with patch.object(acquire, "fetch", return_value=(body, deepcopy(http_fixture.OBSERVATION))):
            manifest = acquire.acquire("coinbase-candles", selection, output, terms)
        self.assertEqual(manifest["source_terms"]["file"]["size"], 129 * MIB)
        self.assertEqual(snapshot.file_hash(output / "source-terms.bin"), snapshot.file_hash(terms))
        self.assertEqual(acquire.verify(output)["record_count"], 1)
        self.assertIsNone(manifest["limits"]["max_response_total_bytes"])

    def test_metadata_and_native_evidence_reads_have_no_32_or_128_mib_cap(self):
        path = self.root / "evidence.json"
        with path.open("wb") as stream:
            stream.write(b'{"value":1}')
            for _ in range(129):
                stream.write(b" " * MIB)
        self.assertEqual(source_plugins.load_json(path), {"value": 1})
        self.assertEqual(snapshot.fetch_local_manifest(path), {"value": 1})
        self.assertEqual(path.stat().st_size, 129 * MIB + len(b'{"value":1}'))
        metadata = b'{"revision":"fixed"}' + b" " * (32 * MIB + 1)
        with patch.object(snapshot.urllib.request, "urlopen", return_value=io.BytesIO(metadata)):
            self.assertEqual(snapshot.fetch_json("https://huggingface.co/example"), {"revision": "fixed"})

    def test_hf_large_json_is_fully_parsed_and_long_jsonl_row_is_not_clipped(self):
        path = self.root / "large.json"
        body = b'{"value":"' + b"a" * (32 * MIB + 1) + b'"}'
        path.write_bytes(body)
        self.assertEqual(hf_dataset.inspect_file(path, {"size": len(body), "format": "json"}), "JSON_DOCUMENT")
        path.write_bytes(body[:-1] + b"!")
        with self.assertRaises(ValueError):
            hf_dataset.inspect_file(path, {"size": len(body), "format": "json"})
        body = b'{"value":"' + b"a" * (MIB + 1) + b'"}\n'
        path.write_bytes(body)
        self.assertEqual(hf_dataset.inspect_file(path, {"size": len(body), "format": "jsonl"}), "JSONL_FIRST_ROW")
        self.assertEqual(path.read_bytes(), body)

    def test_hf_full_100001_file_index_and_large_total_are_accepted(self):
        count = 100_001
        revision = "a" * 40
        files = [{"path": f"data/{i}.opaque", "markets": ["market"], "format": "opaque",
                  "start_date": "2026-09-01", "end_date": "2026-09-02"} for i in range(count)]
        index = {"schema": hf_dataset.INDEX_SCHEMA, "files": files}
        body = json.dumps(index).encode()
        self.assertGreater(len(body), 8 * MIB)
        available = {item["path"]: {"size": 2048} for item in files}
        available["partitions.json"] = {"size": len(body)}
        with patch.object(snapshot, "repository_metadata", return_value=(revision, available, {}, "metadata")), \
                patch.object(snapshot, "fetch_bytes", return_value=body) as fetched:
            plan = hf_dataset.plan("example/history", manifest="partitions.json", markets=["market"])
        fetched.assert_called_once_with(snapshot.repository_file_url("example/history", revision, "partitions.json"), None)
        self.assertEqual(len(plan["files"]), count)
        self.assertEqual(plan["total_bytes"], count * 2048)
        self.assertGreater(plan["total_bytes"], 128 * MIB)
        self.assertIsNone(plan["max_bytes"])
        hf_dataset.validate_plan(plan)
        plan["files"].append(deepcopy(plan["files"][-1]))
        with self.assertRaisesRegex(ValueError, "identity"):
            hf_dataset.validate_plan(plan)

    def test_binance_large_member_fields_and_decimals_preserve_sha_and_gaps(self):
        fields = archive_fixture.row()
        fields[1] = "1." + "0" * 101 + "1"
        fields[11] = "x" * (4 * MIB + 1)
        body = archive_fixture.csv_bytes([fields])
        archive = archive_fixture.archive_bytes(body, compression=zipfile.ZIP_STORED)
        self.assertGreater(len(archive), MIB)
        result = vision.decode(archive, archive_fixture.checksum(archive), archive_fixture.SELECTION)
        self.assertEqual(result["member"]["size"], len(body))
        self.assertEqual(result["member"]["sha256"], hashlib.sha256(body).hexdigest())
        self.assertEqual(result["rows"][0]["source_ignored"], fields[11])
        self.assertEqual(result["rows"][0]["open"], fields[1])
        self.assertEqual(result["counts"]["missing_buckets"], "1439")
        with self.assertRaises(ValueError):
            vision.decode(archive[:-1] + b"X", archive_fixture.checksum(archive), archive_fixture.SELECTION)

    def test_rpc_complete_body_above_32_mib_and_explicit_selection_counts(self):
        body = json.dumps(evm_fixture.reply("0x89")).encode() + b" " * (32 * MIB + 1)
        for declared in (True, False):
            response = evm_fixture.Response(body, declared=declared)
            with patch.object(evm.OPENER, "open", return_value=response):
                value, size = evm.rpc_call(evm_fixture.ENDPOINTS[0], "eth_chainId", [])
            self.assertEqual(value, evm_fixture.reply("0x89"))
            self.assertEqual(size, len(body))
            self.assertEqual(response.consumed, len(body))
        self.assertEqual(evm.select_blocks(first=0, last=4096), list(range(4097)))
        endpoints = [f"https://rpc{i}.example" for i in range(5)]
        addresses = ["0x" + f"{i:040x}" for i in range(257)]
        requests = []
        def rpc(endpoint, method, params, remaining):
            self.assertIsNone(remaining)
            requests.append((endpoint, method, params))
            if method == "eth_chainId":
                return evm_fixture.reply("0x89"), 1
            if method == "eth_getBlockByNumber":
                return evm_fixture.reply(evm_fixture.header(int(params[0], 16))), 1
            self.assertEqual(params[0]["address"], addresses)
            return evm_fixture.reply([]), 1
        output = self.root / "evm.json"
        with patch.object(evm, "rpc_call", side_effect=rpc):
            summary = evm.download(137, endpoints, [11], addresses, evm_fixture.TOPIC, output)
        result = json.loads(output.read_bytes())
        self.assertEqual(result["query"]["addresses"], addresses)
        self.assertEqual([item["endpoint"] for item in result["observations"]], endpoints)
        self.assertEqual(summary["downloaded_bytes"], len(requests))
        self.assertEqual(summary["rows"], 0)  # Empty source logs do not become invented events.

    def test_healthy_http_and_rpc_transfers_have_no_cumulative_deadline(self):
        # A monotonic call here would reintroduce a hard elapsed-time budget.
        with patch.object(acquire.time, "monotonic", side_effect=AssertionError("wall-time cap")):
            body = b"[]"
            with patch.object(acquire.OPENER, "open", return_value=http_fixture.Response(body)) as opened:
                self.assertEqual(acquire.fetch("https://api.exchange.coinbase.com/test")[0], body)
            self.assertEqual(opened.call_args.kwargs["timeout"], 30)
            body = json.dumps(evm_fixture.reply("0x89")).encode()
            with patch.object(evm.OPENER, "open", return_value=evm_fixture.Response(body)) as opened:
                self.assertEqual(evm.rpc_call(evm_fixture.ENDPOINTS[0], "eth_chainId", [])[1], len(body))
            self.assertEqual(opened.call_args.kwargs["timeout"], 30)

    def test_native_publication_counts_and_identity_metadata_have_no_total_cap(self):
        output = self.root / "native"
        (output / "catalog").mkdir(parents=True)
        (output / "catalog/data.parquet").write_bytes(b"PAR1fixturePAR1")
        report = {"schema_version": 1, "native_version": "0.63.0", "catalog_relative_path": "catalog",
                  "coverage": "UNPROVEN", "historical_availability": "UNVERIFIED",
                  "registered_in_quazonai": False, "research_qualified": False,
                  "limitations": ["Fixture envelope only"], "instruments": 1_000_001,
                  "instrument_versions": 1_000_001}
        (output / "import-report.json").write_text(json.dumps(report))
        (output / "source-evidence.json").write_text("{}")
        self.assertEqual(source_plugins.published_native(output), (report, {}))
        identity = {"schema_version": 1, "registered_ref": "r" * 513, "storage_version": "v" * 121}
        self.assertEqual(source_plugins.catalog_identity(identity),
                         {"native_catalog_ref": identity["registered_ref"], "native_storage_version": identity["storage_version"]})
        report["instrument_versions"] -= 1
        (output / "import-report.json").write_text(json.dumps(report))
        with self.assertRaisesRegex(ValueError, "counts"):
            source_plugins.published_native(output)

    def test_legacy_public_manifest_limits_are_metadata_without_rewriting(self):
        terms = self.root / "terms.txt"
        terms.write_bytes(b"source terms")
        output = self.root / "public"
        body = b"[" + http_fixture.CANDLE + b"]"
        with patch.object(acquire, "fetch", return_value=(body, deepcopy(http_fixture.OBSERVATION))):
            manifest = acquire.acquire("coinbase-candles", providers.Selection("BTC-USD", 0, 60, 60), output, terms)
        manifest["limits"] = {"max_response_bytes": 4 * MIB, "max_response_total_bytes": 32 * MIB,
                              "max_requests": 128, "max_records": 100_000, "max_output_bytes": 128 * MIB}
        path = output / "acquisition.json"
        preserved = acquire.json_bytes(manifest)
        path.write_bytes(preserved)
        self.assertEqual(acquire.verify(output)["record_count"], 1)
        self.assertEqual(path.read_bytes(), preserved)
        changed = deepcopy(manifest)
        changed["limits"]["max_records"] += 1
        path.write_bytes(acquire.json_bytes(changed))
        with self.assertRaisesRegex(ValueError, "limit metadata"):
            acquire.verify(output)
        manifest["responses"][0]["counts"]["selected_rows"] = 0
        path.write_bytes(acquire.json_bytes(manifest))
        with self.assertRaisesRegex(ValueError, "counts"):
            acquire.verify(output)

    def test_legacy_archive_manifest_limits_remain_immutable_and_verified(self):
        archive = archive_fixture.archive_bytes()
        archive_path, checksum_path = self.root / "original.zip", self.root / "checksum"
        archive_path.write_bytes(archive)
        checksum_path.write_bytes(archive_fixture.checksum(archive))
        output = self.root / "archive"
        manifest = vision.freeze(archive_fixture.SELECTION, archive_path, checksum_path, output)
        manifest["limits"] = {"archive_bytes": MIB, "checksum_bytes": 4096, "csv_bytes": 4 * MIB,
            "row_bytes": 4096, "field_bytes": 128, "decimal_digits": 100, "rows": 1440,
            "provenance_bytes": 4096, "evidence_bytes_each": 2 * MIB, "manifest_bytes": 256 * 1024,
            "records_bytes": 4 * MIB}
        path = output / "archive.json"
        preserved = acquire.json_bytes(manifest)
        path.write_bytes(preserved)
        self.assertEqual(vision.verify(output)["integrity"], "VERIFIED")
        self.assertEqual(path.read_bytes(), preserved)
        changed = deepcopy(manifest)
        changed["limits"]["archive_bytes"] += 1
        path.write_bytes(acquire.json_bytes(changed))
        with self.assertRaisesRegex(ValueError, "limit metadata"):
            vision.verify(output)
        path.write_bytes(preserved)
        with (output / "raw/archive.zip").open("ab") as stream:
            stream.write(b"tampered")
        with self.assertRaises(ValueError):
            vision.verify(output)


if __name__ == "__main__":
    unittest.main()
