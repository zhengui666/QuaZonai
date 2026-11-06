import contextlib
import io
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import cryptohftdata
import source_plugins


class CryptoHFTDataTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.archive = self.root / "archive"
        self.archive.mkdir()
        for market in cryptohftdata.MARKETS.values():
            (self.archive / market).mkdir()
        self.manifest = {"status": "COMPLETE", "bytes": 123, "objects": 3,
                         "start_date_utc": "2026-02-12", "end_date_utc": "2026-10-13"}
        (self.archive / "dataset.json").write_text(json.dumps(self.manifest))

    def file(self, relative, data=b"original downloaded file"):
        path = self.archive / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data)
        return path

    def select(self, **options):
        return cryptohftdata.selection(self.archive, "perpetual", ["BTC"],
                                       "2026-10-13", "2026-10-14", **options)

    def test_inspection_reads_summary_without_walking_archive(self):
        with patch.object(Path, "iterdir", side_effect=AssertionError("must not scan archive")), \
                patch.object(Path, "rglob", side_effect=AssertionError("must not scan archive")):
            result = cryptohftdata.inspect(self.archive)
        self.assertEqual(result["objects"], 3)
        self.assertEqual(result["end_date"], "2026-10-14")
        self.assertFalse(result["research_qualified"])

    def test_selected_files_keep_original_plain_and_compressed_formats_and_clocks(self):
        plain = self.file("hyperliquid_futures/2026-10-13/23/BTC_orderbook.parquet")
        compressed = self.file("hyperliquid_futures/2026-10-13/23/BTC_mark_price.parquet.zst")
        selection = self.select(hours=[23])
        self.assertEqual(selection["source_root"], str(self.archive))
        self.assertEqual({f["path"] for f in selection["files"]},
                         {plain.relative_to(self.archive).as_posix(), compressed.relative_to(self.archive).as_posix()})
        definitions = {s["name"]: s for s in selection["profile"]["streams"]}
        self.assertEqual(definitions["books"]["kind"], "book_snapshots")
        self.assertEqual(definitions["books"]["time_unit"], "ms")
        self.assertEqual(definitions["books"]["available_time_unit"], "ns")
        self.assertEqual(definitions["context-zstd"]["time_unit"], "ns")
        self.assertEqual(definitions["context-zstd"]["columns"]["time"], "received_time")
        self.assertEqual(definitions["context-zstd"]["format"], "parquet-zstd")
        self.assertEqual(selection["missing"], [])
        self.assertNotIn("sha256", json.dumps(selection))
        self.assertNotIn("huggingface", selection["source_reference"])

    def test_sparse_hours_remain_reported_and_unrelated_files_are_not_selected(self):
        self.file("hyperliquid_futures/2026-10-13/23/BTC_orderbook.parquet")
        self.file("hyperliquid_futures/2026-10-13/22/ETH_orderbook.parquet")
        selection = self.select(streams=["books"], hours=[22, 23])
        self.assertEqual(len(selection["files"]), 1)
        self.assertEqual(len(selection["missing"]), 1)
        self.assertIn("/22/BTC_", selection["missing"][0]["pattern"])
        self.assertEqual(selection["selection"]["symbols"], ["BTC"])

    def test_invalid_selection_and_ambiguous_source_do_not_publish(self):
        for options in ({"hours": [-1]}, {"hours": [24]}, {"streams": ["trades"]}):
            with self.subTest(options=options), self.assertRaises(ValueError):
                self.select(**options)
        for symbol in ("../BTC", "BTC/USDC", "", ".."):
            with self.subTest(symbol=symbol), self.assertRaises(ValueError):
                cryptohftdata.selection(self.archive, "perpetual", [symbol], "2026-10-13", "2026-10-14")
        with self.assertRaises(ValueError):
            cryptohftdata.selection(self.archive, "spot", ["UBTC_USDC"], "2026-10-13", "2026-10-14", ["context"])
        with self.assertRaises(ValueError):
            cryptohftdata.selection(self.archive, "perpetual", ["BTC"], "2026-10-14", "2026-10-15")
        self.file("hyperliquid_futures/2026-10-13/23/BTC_orderbook.parquet")
        self.file("hyperliquid_futures/2026-10-13/23/BTC_orderbook.parquet.zst")
        with self.assertRaisesRegex(ValueError, "ambiguous"):
            self.select()

    def test_source_symlink_cannot_escape_archive_and_missing_symbol_fails(self):
        original = self.root / "outside.parquet"
        original.write_bytes(b"outside")
        path = self.archive / "hyperliquid_futures/2026-10-13/23/BTC_orderbook.parquet"
        path.parent.mkdir(parents=True)
        path.symlink_to(original)
        with self.assertRaisesRegex(ValueError, "outside"):
            self.select()
        path.unlink()
        self.file("hyperliquid_futures/2026-10-13/23/BTC_orderbook.parquet")
        with self.assertRaisesRegex(ValueError, "selected symbol"):
            cryptohftdata.selection(self.archive, "perpetual", ["BTC", "MISSING"], "2026-10-13", "2026-10-14")

    def test_convert_dispatch_keeps_raw_files_and_passes_existing_native_interface(self):
        raw = self.file("hyperliquid_futures/2026-10-13/23/BTC_orderbook.parquet")
        output = self.root / "native"
        report = {"schema_version": 1, "status": "IMPORTED", "readback_records": 41,
                  "missing_symbols": {}, "rejected_rows": {}, "native_records": {"quotes": {"BTC": 1}}}
        def run(args, argv, binary):
            self.assertEqual(binary, "market-history")
            self.assertEqual(argv[0], "import")
            self.assertIn("--native-precision", argv)
            output.mkdir()
            (output / "catalog").mkdir()
            (output / "import-report.json").write_text(json.dumps(report))
            return output
        with patch.object(source_plugins, "run_native", side_effect=run), \
                contextlib.redirect_stdout(io.StringIO()) as stdout:
            code = source_plugins.main(["convert", cryptohftdata.PLUGIN, "--archive", str(self.archive),
                                       "--market", "perpetual", "--symbol", "BTC", "--start-date", "2026-10-13",
                                       "--end-date", "2026-10-14", "--hour", "23", "--stream", "books",
                                       "--instruments", str(self.root / "instruments.json"), "--output", str(output)])
        self.assertEqual(code, 0)
        result = json.loads(stdout.getvalue())
        self.assertEqual(result["catalog_root"], str(output / "catalog"))
        self.assertFalse(result["research_qualified"])
        self.assertEqual(raw.read_bytes(), b"original downloaded file")
        self.assertEqual(json.loads(Path(result["source_manifest"]).read_text())["source_root"], str(self.archive))
        self.assertEqual(source_plugins.PLUGINS[cryptohftdata.PLUGIN].capabilities.keys(), {"inspect", "convert"})


if __name__ == "__main__":
    unittest.main()
