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
        self.instruments = {"currencies": [], "instruments": [], "symbol_ids": {"BTC": "BTC-USD-PERP.HYPERLIQUID"}}
        (self.root / "instruments.json").write_text(json.dumps(self.instruments))

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

    def test_declared_spot_archive_requires_only_its_actual_market(self):
        (self.archive / "hyperliquid_futures").rmdir()
        self.file("hyperliquid_spot/2026-10-13/23/HFUN_USDC_orderbook.parquet.zst")
        self.manifest["datasets"] = ["spot orderbook"]
        self.manifest["end_date_utc"] = "2026-10-14"
        descriptor = self.archive / "dataset.json"
        descriptor.write_text(json.dumps(self.manifest))
        self.assertEqual(cryptohftdata.inspect(self.archive)["streams"], ["spot orderbook"])
        selected = cryptohftdata.selection(self.archive, "spot", ["HFUN_USDC"],
                                          "2026-10-13", "2026-10-14", hours=[23])
        self.assertEqual(len(selected["files"]), 1)
        for streams in ([], ["spot orderbook", "spot orderbook"], ["unknown"], [None]):
            with self.subTest(streams=streams):
                self.manifest["datasets"] = streams
                descriptor.write_text(json.dumps(self.manifest))
                with self.assertRaisesRegex(ValueError, "archive datasets"):
                    cryptohftdata.archive(self.archive)
        for streams in (["perpetual trades"], None):
            if streams is None:
                del self.manifest["datasets"]
            else:
                self.manifest["datasets"] = streams
            descriptor.write_text(json.dumps(self.manifest))
            with self.assertRaisesRegex(ValueError, "market directories"):
                cryptohftdata.archive(self.archive)

    def test_selected_files_keep_original_plain_and_compressed_formats_and_clocks(self):
        plain = self.file("hyperliquid_futures/2026-10-13/23/BTC_orderbook.parquet")
        compressed = self.file("hyperliquid_futures/2026-10-13/23/BTC_mark_price.parquet.zst")
        self.file("hyperliquid_futures/2026-10-13/23/BTC_trades.parquet")
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
        for file in selection["files"]:
            state = (self.archive / file["path"]).stat()
            self.assertEqual(file["file_identity"], ":".join(str(value) for value in
                (state.st_dev, state.st_ino, state.st_size, state.st_mtime_ns, state.st_ctime_ns)))
        self.assertNotIn("sha256", json.dumps(selection))
        self.assertNotIn("huggingface", selection["source_reference"])

    def test_sparse_hours_remain_reported_and_unrelated_files_are_not_selected(self):
        self.file("hyperliquid_futures/2026-10-13/23/BTC_orderbook.parquet")
        self.file("hyperliquid_futures/2026-10-13/22/ETH_orderbook.parquet")
        selection = self.select(streams=["books"], hours=[22, 23])
        self.assertEqual(len(selection["files"]), 1)
        self.assertEqual(len(selection["missing"]), 1)
        self.assertEqual(selection["missing_hour_count"], 1)
        self.assertIn("/22/BTC_", selection["missing"][0]["pattern"])
        self.assertNotIn("hours", selection["missing"][0])
        self.assertEqual(selection["selection"]["symbols"], ["BTC"])

    def test_missing_hours_group_by_stream_date_and_symbol_with_exact_count(self):
        for day in ("2026-10-12", "2026-10-13"):
            self.file(f"hyperliquid_futures/{day}/23/BTC_trades.parquet")
        result = cryptohftdata.selection(self.archive, "perpetual", ["BTC", "MISSING"],
            "2026-10-12", "2026-10-14", streams=["books", "trades"], hours=[21, 22, 23])
        self.assertEqual(result["missing_hour_count"], 22)
        self.assertEqual(len(result["missing"]), 8)
        for item in result["missing"]:
            self.assertIn(item["stream"], ("books", "trades"))
            self.assertIn(item["date"], ("2026-10-12", "2026-10-13"))
            self.assertIn("/{hour}/", item["pattern"])
            expected = [21, 22] if item["stream"] == "trades" and "/BTC_" in item["pattern"] else [21, 22, 23]
            self.assertEqual(item["hours"], expected)
        self.assertEqual(sum(len(item["hours"]) for item in result["missing"]), 22)
        self.assertEqual(result["selection"]["symbols"], ["BTC", "MISSING"])
        self.assertEqual(result["unobserved_symbols"], ["MISSING"])

    def test_trade_stream_preserves_real_columns_clocks_and_aggressor_direction(self):
        for market, symbol, compressed in (("perpetual", "BTC", False), ("spot", "UBTC_USDC", True)):
            with self.subTest(market=market):
                relative = f"{cryptohftdata.MARKETS[market]}/2026-10-13/23/{symbol}_trades.parquet"
                if compressed:
                    relative += ".zst"
                self.file(relative)
                selection = cryptohftdata.selection(self.archive, market, [symbol], "2026-10-13",
                                                    "2026-10-14", streams=["trades"], hours=[23])
                stream, = selection["profile"]["streams"]
                self.assertEqual(stream["kind"], "trades")
                self.assertEqual(stream["time_unit"], "ms")
                self.assertEqual(stream["available_time_unit"], "ns")
                self.assertEqual(stream["columns"], {"symbol": "symbol", "time": "event_time",
                    "available_at": "received_time", "side": "is_buyer_maker", "price": "price",
                    "size": "quantity", "trade_id": "trade_id"})
                self.assertEqual(stream["sides"], {"true": "SELLER", "false": "BUYER"})
                self.assertEqual(stream["format"], "parquet-zstd" if compressed else "parquet")
                self.assertEqual(selection["files"][0]["path"], relative)
                self.assertEqual(selection["missing"], [])

    def test_invalid_selection_and_ambiguous_source_do_not_publish(self):
        for options in ({"hours": [-1]}, {"hours": [24]}, {"streams": ["candles"]}):
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

    def test_source_symlink_cannot_escape_and_unobserved_requested_symbols_stay_reported(self):
        original = self.root / "outside.parquet"
        original.write_bytes(b"outside")
        path = self.archive / "hyperliquid_futures/2026-10-13/23/BTC_orderbook.parquet"
        path.parent.mkdir(parents=True)
        path.symlink_to(original)
        with self.assertRaisesRegex(ValueError, "outside"):
            self.select()
        path.unlink()
        self.file("hyperliquid_futures/2026-10-13/23/BTC_orderbook.parquet")
        result = cryptohftdata.selection(self.archive, "perpetual", ["BTC", "MISSING"], "2026-10-13",
                                         "2026-10-14", streams=["books"], hours=[23])
        self.assertEqual(result["coverage"], "OBSERVED_PUBLISHED_FILES")
        self.assertEqual(result["selection"]["symbols"], ["BTC", "MISSING"])
        self.assertEqual(result["unobserved_symbols"], ["MISSING"])
        self.assertEqual(len(result["files"]), 1)
        self.assertEqual(len(result["missing"]), 1)

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
        self.assertEqual(source_plugins.PLUGINS[cryptohftdata.PLUGIN].capabilities.keys(), {"inspect", "plan", "convert"})

    def test_offline_plan_keeps_original_files_and_embeds_original_definitions(self):
        raw = self.file("hyperliquid_futures/2026-10-13/23/BTC_orderbook.parquet")
        output = self.root / "book-snapshots.source.json"
        argv = ["plan", cryptohftdata.PLUGIN, "--archive", str(self.archive),
            "--market", "perpetual", "--symbol", "BTC", "--start-date", "2026-10-13",
            "--end-date", "2026-10-14", "--hour", "23", "--stream", "books",
            "--instruments", str(self.root / "instruments.json"), "--output", str(output)]
        with patch.object(source_plugins, "run_native") as native, \
                contextlib.redirect_stdout(io.StringIO()) as stdout:
            code = source_plugins.main(argv)
        self.assertEqual(code, 0)
        native.assert_not_called()
        summary = json.loads(stdout.getvalue())
        self.assertEqual(summary["files"], 1)
        manifest = json.loads(output.read_text())
        self.assertEqual(manifest["source_root"], str(self.archive))
        self.assertEqual(manifest["instrument_source"], self.instruments)
        self.assertEqual(manifest["files"][0]["path"], raw.relative_to(self.archive).as_posix())
        self.assertEqual(raw.read_bytes(), b"original downloaded file")
        with contextlib.redirect_stderr(io.StringIO()), self.assertRaises(SystemExit):
            source_plugins.parser().parse_args(argv + ["--trade-ticks"])

    def test_received_context_prices_dispatches_original_mapping_and_explicit_revision(self):
        raw = self.file("hyperliquid_futures/2026-10-13/23/BTC_mark_price.parquet")
        output = self.root / "context-prices"
        argv = ["convert", cryptohftdata.PLUGIN, "--archive", str(self.archive),
                "--market", "perpetual", "--symbol", "BTC", "--start-date", "2026-10-13",
                "--end-date", "2026-10-14", "--hour", "23", "--context-prices",
                "--research-source-revision", "original-research", "--instruments", str(self.root / "instruments.json"),
                "--output", str(output)]

        def run(args, native, binary):
            self.assertEqual(binary, "market-history")
            self.assertEqual(native[0], "context-prices")
            self.assertEqual(native[-2:], ["--research-source-revision", "original-research"])
            manifest = json.loads(Path(native[native.index("--manifest") + 1]).read_text())
            self.assertEqual(manifest["selection"]["streams"], ["context"])
            self.assertEqual(manifest["profile"]["streams"][0]["columns"]["available_at"], "received_time")
            self.assertEqual(manifest["instrument_source"], self.instruments)
            output.mkdir()
            (output / "import-report.json").write_text(json.dumps({"status": "IMPORTED", "readback_records": 1}))

        with patch.object(source_plugins, "run_native", side_effect=run) as native, contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(source_plugins.main(argv), 0)
            self.assertEqual(native.call_count, 1)
        self.assertEqual(raw.read_bytes(), b"original downloaded file")
        invalid = argv[:argv.index("--research-source-revision")] + argv[argv.index("--research-source-revision") + 2:]
        invalid[-1] = str(self.root / "unbound")
        with patch.object(source_plugins, "run_native") as native, self.assertRaises(ValueError):
            source_plugins.cryptohftdata_convert(source_plugins.parser().parse_args(invalid))
        native.assert_not_called()

    def test_received_trade_modes_use_existing_native_flags(self):
        self.file("hyperliquid_futures/2026-10-13/23/BTC_trades.parquet")
        for mode in ("--trade-ticks",):
            with self.subTest(mode=mode):
                output = self.root / mode[2:]
                def run(args, argv, binary):
                    self.assertEqual(binary, "market-history")
                    self.assertIn(mode, argv)
                    self.assertIn("--received-time", argv)
                    self.assertEqual(argv[-2:], ["--maximum-receive-delay-ns", "2000000000"])
                    output.mkdir()
                    (output / "import-report.json").write_text(json.dumps({"status": "IMPORTED",
                        "readback_records": 1, "missing_symbols": {}, "rejected_rows": {}}))
                    return output
                with patch.object(source_plugins, "run_native", side_effect=run), \
                        contextlib.redirect_stdout(io.StringIO()) as stdout:
                    code = source_plugins.main(["convert", cryptohftdata.PLUGIN, "--archive", str(self.archive),
                        "--market", "perpetual", "--symbol", "BTC", "--start-date", "2026-10-13",
                        "--end-date", "2026-10-14", "--hour", "23", "--stream", "trades", mode,
                        "--received-time", "--maximum-receive-delay-ns", "2000000000",
                        "--instruments", str(self.root / "instruments.json"), "--output", str(output)])
                self.assertEqual(code, 0)
                self.assertEqual(json.loads(stdout.getvalue())["catalog_root"], str(output / "catalog"))

    def test_trade_mode_rejects_mixed_streams_and_incomplete_receive_clock_options(self):
        self.file("hyperliquid_futures/2026-10-13/23/BTC_orderbook.parquet")
        self.file("hyperliquid_futures/2026-10-13/23/BTC_trades.parquet")
        output = self.root / "invalid"
        base = ["convert", cryptohftdata.PLUGIN, "--archive", str(self.archive), "--market", "perpetual",
            "--symbol", "BTC", "--start-date", "2026-10-13", "--end-date", "2026-10-14", "--hour", "23",
            "--instruments", str(self.root / "instruments.json"), "--output", str(output)]
        for options in (("--stream", "books", "--trade-ticks"),
                        ("--stream", "trades", "--received-time", "--maximum-receive-delay-ns", "1"),
                        ("--stream", "trades", "--trade-ticks", "--received-time"),
                        ("--stream", "trades", "--trade-ticks", "--maximum-receive-delay-ns", "1")):
            with self.subTest(options=options), contextlib.redirect_stderr(io.StringIO()), \
                    patch.object(source_plugins, "run_native") as native:
                self.assertEqual(source_plugins.main(base + list(options)), 1)
                native.assert_not_called()
                self.assertFalse(output.with_name(output.name + ".source.json").exists())


if __name__ == "__main__":
    unittest.main()
