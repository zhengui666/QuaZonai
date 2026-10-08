"""Offline acquisition boundary tests; no network, account or market-data qualification."""

import contextlib
from copy import deepcopy
from decimal import Decimal
import io
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import urllib.error

import acquire
import providers
from providers import Selection


OBSERVATION = {"status": 200, "headers": {"Content-Type": "application/json"},
               "request_started_at": "2026-09-30T00:00:00Z", "retrieved_at": "2026-09-30T00:00:01Z"}
CANDLE = b'[0,1.0000000000000000000001,3,2.0000000000000000000001,2,0.0000000000000000000001]'


class ProvidersTest(unittest.TestCase):
    def test_registry_exposes_distinct_semantics_without_qualification(self):
        self.assertEqual(providers.PROVIDERS["polymarket-prices"].descriptor["record_kind"], "PRICE_MARK")
        self.assertEqual(providers.PROVIDERS["coinbase-candles"].descriptor["record_kind"], "OHLCV_CANDLE")
        for provider in providers.PROVIDERS.values():
            self.assertEqual(provider.descriptor["authentication"], "NONE")
            self.assertEqual(provider.descriptor["native_conversion"], "UNSUPPORTED")
            self.assertEqual(provider.descriptor["permission_status"], "REQUIRES_INDEPENDENT_REVIEW")
            self.assertIsNone(provider.descriptor["license"])

    def test_returned_plan_cannot_mutate_global_admission_or_provider_capabilities(self):
        result = acquire.plan("coinbase-candles", Selection("BTC-USD", 0, 60, 60))
        result["admission"]["research_qualified"] = True
        result["provider"]["native_conversion"] = "SUPPORTED"
        original = acquire.plan("coinbase-candles", Selection("BTC-USD", 0, 60, 60))
        self.assertFalse(original["admission"]["research_qualified"])
        self.assertEqual(original["provider"]["native_conversion"], "UNSUPPORTED")

    def test_plans_split_into_bounded_explicit_windows(self):
        crypto = acquire.plan("coinbase-candles", Selection("ETH-BTC", 0, 600 * 60, 60))
        requests = crypto["requests"]
        self.assertEqual(len(requests), 3)
        self.assertEqual([(r["start_seconds"], r["end_seconds"]) for r in requests],
                         [(0, 17940), (17940, 35880), (35880, 36000)])
        self.assertIn("products/ETH-BTC/candles?start=1970-01-01T00%3A00%3A00Z&end=1970-01-01T04%3A59%3A00Z&granularity=60", requests[0]["url"])
        marks = acquire.plan("polymarket-prices", Selection("123", 10, 86420, 120))
        self.assertEqual(len(marks["requests"]), 2)
        self.assertIn("market=123&startTs=10&endTs=86410&fidelity=2", marks["requests"][0]["url"])

    def test_invalid_instruments_windows_intervals_and_budgets_fail_before_io(self):
        cases = [
            ("coinbase-candles", Selection("../BTC-USD", 0, 60, 60)),
            ("coinbase-candles", Selection("btc-usd", 0, 60, 60)),
            ("coinbase-candles", Selection("BTC-USD?auth=secret", 0, 60, 60)),
            ("coinbase-candles", Selection("BTC-USD", 1, 60, 60)),
            ("coinbase-candles", Selection("BTC-USD", 0, 60, 61)),
            ("coinbase-candles", Selection("BTC-USD", 60, 0, 60)),
            ("coinbase-candles", Selection("BTC-USD", False, 60, 60)),
            ("polymarket-prices", Selection(str(2**256), 0, 60, 60)),
            ("polymarket-prices", Selection("00123", 0, 60, 60)),
            ("polymarket-prices", Selection("123", 0, 60, 61)),
        ]
        for provider, selection in cases:
            with self.subTest(provider=provider, selection=selection), self.assertRaises(ValueError):
                acquire.plan(provider, selection)
        for budget in (0, -1, True):
            with self.assertRaises(ValueError):
                acquire.plan("coinbase-candles", Selection("BTC-USD", 0, 60, 60), budget)

    def test_registry_extension_uses_same_dispatch_and_validates_request_plan(self):
        class Other(providers.CoinbaseCandles):
            descriptor = {**providers.CoinbaseCandles.descriptor, "id": "other"}
        with patch.dict(providers.PROVIDERS, {"other": Other()}):
            self.assertEqual(acquire.plan("other", Selection("BTC-USD", 0, 60, 60))["provider"]["id"], "other")
            for url, start, end in [("http://api.exchange.coinbase.com/x", 0, 60),
                                    ("https://attacker.example/x", 0, 60),
                                    ("https://user@api.exchange.coinbase.com/x", 0, 60),
                                    ("https://api.exchange.coinbase.com/x", 1, 60),
                                    ("https://api.exchange.coinbase.com/x", 0, 30)]:
                with patch.object(Other, "plan", return_value=[providers.Request(url, start, end)]), self.assertRaises(ValueError):
                    acquire.plan("other", Selection("BTC-USD", 0, 60, 60))

    def test_candles_preserve_decimal_precision_and_bucket_end(self):
        rows = providers.CoinbaseCandles().decode(b'[' + CANDLE + b']', Selection("BTC-USD", 0, 60, 60))
        self.assertEqual(rows[0]["open"], "2.0000000000000000000001")
        self.assertEqual(rows[0]["volume"], "0.0000000000000000000001")
        self.assertEqual(rows[0]["selection_time_seconds"], 0)
        self.assertEqual(rows[0]["event_time_seconds"], 60)
        self.assertEqual(providers.decimal_text(Decimal("1e-40")), "0." + "0" * 39 + "1")
        with self.assertRaises(ValueError):
            providers.decimal_text(0.1)

    def test_invalid_json_numbers_and_duplicates_fail(self):
        invalid = [b'{"history":[],"history":[]}', b'{"history":[{"t":0,"p":NaN}]}',
                   b'{"history":[{"t":0,"p":Infinity}]}', b'{"history":[{"t":0,"p":1e999}]}',
                   b'{"history":[{"t":0,"p":true}]}', b'{"history":[{"t":0,"p":"0.5"}]}',
                   b'{"history":[{"t":0,"p":1.01}]}', b'{"history":[{"t":0.0,"p":0.5}]}',
                   b'{"history":[{"t":0,"p":0.5},{"t":0,"p":0.5}]}',
                   b'{"history":[],"error":"upstream unavailable"}']
        for body in invalid:
            with self.subTest(body=body), self.assertRaises(ValueError):
                providers.PolymarketPrices().decode(body, Selection("123", 0, 60, 60))
        crypto = [b'[[0,3,1,2,2,1]]', b'[[1,1,3,2,2,1]]', b'[[0,1,3,2,2,-1]]',
                  b'[[0,1,3,2,2]]', b'[[0,0,3,2,2,1]]', b'[[0,1,3,2,2,1],[0,1,3,2,2,1]]',
                  b'[{"error":"bad"}]', b'{"message":"bad"}', b'[' + b','.join([CANDLE] * 301) + b']']
        for body in crypto:
            with self.subTest(body=body[:80]), self.assertRaises(ValueError):
                providers.CoinbaseCandles().decode(body, Selection("BTC-USD", 0, 60, 60))


class AcquisitionTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.output = self.root / "acquisition"
        self.terms = self.root / "terms.txt"
        self.terms.write_bytes(b"Original source terms for controlled offline test only.")
        self.selection = Selection("BTC-USD", 0, 180, 60)
        self.body = b'[[180,1,3,2,2,1],[120,1,3,2,2,1],' + CANDLE + b']'
        self.fetch = patch.object(acquire, "fetch", return_value=(self.body, deepcopy(OBSERVATION))).start()
        patch.object(acquire.time, "sleep").start()
        self.addCleanup(patch.stopall)

    def download(self, **kwargs):
        return acquire.acquire("coinbase-candles", self.selection, self.output, self.terms, **kwargs)

    def rewrite_manifest(self, manifest):
        (self.output / "acquisition.json").write_bytes(acquire.json_bytes(manifest))

    def test_raw_bytes_bounds_terms_and_late_observation_are_preserved_and_verified(self):
        manifest = self.download()
        self.assertEqual((self.output / "raw/0000.json").read_bytes(), self.body)
        self.assertEqual((self.output / "source-terms.bin").read_bytes(), self.terms.read_bytes())
        self.assertEqual(manifest["record_count"], 2)
        self.assertEqual(manifest["responses"][0]["counts"],
                         {"source_rows": 3, "selected_rows": 2, "outside_request_window": 1})
        rows = [json.loads(row) for row in (self.output / "records.jsonl").read_text().splitlines()]
        self.assertEqual([row["event_time_seconds"] for row in rows], [60, 180])
        self.assertEqual(rows[0]["open"], "2.0000000000000000000001")
        self.assertEqual(rows[0]["source_row_index"], 2)
        self.assertEqual(rows[0]["observed_at"], OBSERVATION["retrieved_at"])
        self.assertIsNone(rows[0]["historical_available_at"])
        self.fetch.reset_mock()
        self.assertEqual(acquire.verify(self.output)["integrity"], "SOURCE_RECORDS_VALIDATED")
        self.assertEqual(set(manifest["records"]), {"path", "size"})
        self.fetch.assert_not_called()
        self.assertFalse(manifest["admission"]["research_qualified"])

    def test_source_records_are_checked_without_calculating_legacy_checksums(self):
        with patch("hashlib.sha256", side_effect=AssertionError("No source checksum calculation")), \
                patch("hashlib.sha1", side_effect=AssertionError("No substitute checksum")):
            manifest = self.download()
            for record in [manifest["records"], manifest["source_terms"]["file"],
                           *(response["file"] for response in manifest["responses"])]:
                self.assertEqual(set(record), {"path", "size"})
                record["sha256"] = "legacy metadata is not verified"
            self.rewrite_manifest(manifest)
            self.assertEqual(acquire.verify(self.output)["integrity"], "SOURCE_RECORDS_VALIDATED")
            target = self.output / "raw/0000.json"
            original = target.read_bytes()
            changed = original.replace(b"[120,1,3,2,2,1]", b"[120,1,3,2,2,2]")
            self.assertEqual(len(changed), len(original))
            self.assertNotEqual(changed, original)
            target.write_bytes(changed)
            with self.assertRaisesRegex(ValueError, "derived records"):
                acquire.verify(self.output)

    def test_no_overwrite_or_resume_even_when_previous_output_is_incomplete(self):
        self.output.mkdir()
        (self.output / "private.txt").write_text("keep")
        with self.assertRaises(FileExistsError):
            self.download()
        self.fetch.assert_not_called()
        self.assertEqual((self.output / "private.txt").read_text(), "keep")

    def test_existing_snapshot_cannot_be_refreshed_or_reused(self):
        self.download()
        before = (self.output / "acquisition.json").read_bytes()
        self.fetch.reset_mock()
        with self.assertRaises(FileExistsError):
            self.download()
        self.fetch.assert_not_called()
        self.assertEqual((self.output / "acquisition.json").read_bytes(), before)

    def test_failed_second_response_has_no_final_publication(self):
        self.selection = Selection("BTC-USD", 0, 300 * 60, 60)
        self.fetch.side_effect = [(b'[' + CANDLE + b']', deepcopy(OBSERVATION)), OSError("interrupted")]
        with self.assertRaises(OSError):
            self.download()
        self.assertTrue((self.output / "raw/0000.json").is_file())
        self.assertFalse((self.output / "acquisition.json").exists())
        self.assertFalse(list(self.output.rglob("*.partial")))

    def test_duplicate_or_malformed_response_does_not_publish(self):
        for index, body in enumerate((b'[' + CANDLE + b',' + CANDLE + b']', b'not json',
                                     b'[[0,3,1,2,2,1]]', b'[[0,1,3,2,2,-1]]')):
            self.output = self.root / str(index)
            self.fetch.return_value = (body, deepcopy(OBSERVATION))
            with self.assertRaises(ValueError):
                self.download()
            self.assertEqual((self.output / "raw/0000.json").read_bytes(), body)
            self.assertFalse((self.output / "records.jsonl").exists())
            self.assertFalse((self.output / "acquisition.json").exists())

    def test_empty_is_frozen_as_no_observations_not_zero_filled(self):
        self.fetch.return_value = (b'[]', deepcopy(OBSERVATION))
        manifest = self.download()
        self.assertEqual(manifest["observation_status"], "NO_OBSERVATIONS")
        self.assertEqual(manifest["record_count"], 0)
        self.assertEqual((self.output / "records.jsonl").read_bytes(), b'')
        self.assertEqual(acquire.verify(self.output)["record_count"], 0)

    def test_polymarket_marks_use_same_runner_and_are_never_fabricated_candles(self):
        self.fetch.return_value = (b'{"history":[{"t":10,"p":0.12345678901234567890123456789}]}', deepcopy(OBSERVATION))
        manifest = acquire.acquire("polymarket-prices", Selection("123", 0, 60, 60), self.output, self.terms)
        row = json.loads((self.output / "records.jsonl").read_bytes())
        self.assertEqual(row["kind"], "PRICE_MARK")
        self.assertEqual(row["price"], "0.12345678901234567890123456789")
        self.assertNotIn("volume", row)
        self.assertNotIn("close", row)
        self.assertEqual(manifest["record_count"], 1)
        self.assertEqual(acquire.verify(self.output)["record_count"], 1)

    def test_wrong_sizes_and_rewritten_derivatives_are_rejected(self):
        manifest = self.download()
        for path in ("raw/0000.json", "source-terms.bin", "records.jsonl"):
            target = self.output / path
            original = target.read_bytes()
            target.write_bytes(b"modified")
            with self.assertRaises(ValueError):
                acquire.verify(self.output)
            target.write_bytes(original)
        changed = deepcopy(manifest)
        target = self.output / "records.jsonl"
        fake = target.read_bytes().replace(b'2.0000000000000000000001', b'2.5')
        target.write_bytes(fake)
        changed["records"] = {"path": "records.jsonl", "size": len(fake)}
        self.rewrite_manifest(changed)
        with self.assertRaisesRegex(ValueError, "derived records"):
            acquire.verify(self.output)

    def test_manifest_cannot_change_request_bounds_paths_or_qualification(self):
        manifest = self.download()
        mutations = [lambda m: m.update(research_qualified=True),
                     lambda m: m["admission"].update(research_qualified=0),
                     lambda m: m["admission"].update(research_qualified=True),
                     lambda m: m["requests"][0].update(url="https://attacker.example/"),
                     lambda m: m["responses"][0]["file"].update(path="../terms.txt"),
                     lambda m: m["responses"].clear(),
                     lambda m: m["source_terms"].update(evidence_status="VERIFIED"),
                     lambda m: m["responses"][0]["observation"].update(retrieved_at="1970-01-01T00:00:00Z")]
        for mutation in mutations:
            changed = deepcopy(manifest)
            mutation(changed)
            self.rewrite_manifest(changed)
            with self.assertRaises(ValueError):
                acquire.verify(self.output)
        self.rewrite_manifest(manifest)
        acquire.verify(self.output)

    def test_publication_clock_must_be_valid_utc_and_after_every_response(self):
        manifest = self.download()
        invalid = (None, 1, "not a timestamp", "2026-09-30", "2026-09-30T00:00:02",
                   "2026-09-30T01:00:02+01:00", "2026-09-30T00:00:02.1234567Z",
                   "2026-09-30T00:00:00Z", "2026-09-29T23:59:59Z")
        for value in invalid:
            with self.subTest(value=value):
                self.rewrite_manifest({**manifest, "created_at": value})
                with self.assertRaises(ValueError):
                    acquire.verify(self.output)
        # Equality is allowed at the clock's recorded resolution.
        self.rewrite_manifest({**manifest, "created_at": OBSERVATION["retrieved_at"]})
        self.assertEqual(acquire.verify(self.output)["integrity"], "SOURCE_RECORDS_VALIDATED")

    def test_sequential_response_clocks_cannot_overlap_or_move_backwards(self):
        self.selection = Selection("BTC-USD", 0, 300 * 60, 60)
        later = {**OBSERVATION, "request_started_at": "2026-09-30T00:00:02Z",
                 "retrieved_at": "2026-09-30T00:00:03Z"}
        self.fetch.side_effect = [(b'[]', deepcopy(OBSERVATION)), (b'[]', later)]
        manifest = self.download()
        self.assertEqual(acquire.verify(self.output)["integrity"], "SOURCE_RECORDS_VALIDATED")
        for value in ("2026-09-30T00:00:00Z", "2026-09-29T23:59:58Z"):
            changed = deepcopy(manifest)
            changed["responses"][1]["observation"]["request_started_at"] = value
            self.rewrite_manifest(changed)
            with self.assertRaisesRegex(ValueError, "sequential response clocks"):
                acquire.verify(self.output)

    def test_bad_publication_or_sequential_clock_never_publishes(self):
        with patch.object(acquire, "now", return_value="2026-09-30T00:00:00Z"):
            with self.assertRaisesRegex(ValueError, "publication clock"):
                self.download()
        self.assertFalse((self.output / "acquisition.json").exists())
        self.output = self.root / "second"
        self.selection = Selection("BTC-USD", 0, 300 * 60, 60)
        self.fetch.return_value = (b'[]', deepcopy(OBSERVATION))
        with self.assertRaisesRegex(ValueError, "sequential response clocks"):
            self.download()
        self.assertFalse((self.output / "acquisition.json").exists())

    def test_symlink_output_or_terms_cannot_be_used(self):
        self.output.symlink_to(self.root / "elsewhere")
        with self.assertRaisesRegex(ValueError, "symlinks"):
            self.download()
        self.fetch.assert_not_called()
        self.output.unlink()
        original = self.root / "original.txt"
        self.terms.rename(original)
        self.terms.symlink_to(original)
        with self.assertRaisesRegex(ValueError, "symlinks"):
            self.download()
        self.fetch.assert_not_called()

    def test_terms_and_future_intervals_fail_before_io(self):
        self.terms.write_bytes(b' ')
        with self.assertRaises(ValueError):
            self.download()
        self.fetch.assert_not_called()
        self.assertFalse(self.output.exists())
        self.terms.write_bytes(b'terms')
        with patch.object(acquire.time, "time", return_value=120), self.assertRaises(ValueError):
            self.download()
        self.fetch.assert_not_called()
        self.assertFalse(self.output.exists())

    def test_bad_observation_clock_cannot_publish(self):
        observation = {**OBSERVATION, "retrieved_at": "2020-01-01T00:00:00Z"}
        self.fetch.return_value = (self.body, observation)
        with self.assertRaisesRegex(ValueError, "clock"):
            self.download()
        self.assertFalse((self.output / "acquisition.json").exists())

    def test_total_response_budget_is_enforced_by_runner(self):
        with self.assertRaisesRegex(ValueError, "byte budget"):
            self.download(max_bytes=1)
        self.assertFalse((self.output / "acquisition.json").exists())

    def test_output_has_no_implicit_budget_and_explicit_download_bound_remains(self):
        manifest = self.download(max_bytes=1000)
        self.assertGreater((self.output / "acquisition.json").stat().st_size, 1000)
        self.assertIsNone(manifest["limits"]["max_output_bytes"])
        self.assertEqual(acquire.verify(self.output)["record_count"], manifest["record_count"])
        self.output = self.root / "second"
        self.selection = Selection("BTC-USD", 0, 300 * 60, 60)
        later = {**OBSERVATION, "request_started_at": "2026-09-30T00:00:02Z",
                 "retrieved_at": "2026-09-30T00:00:03Z"}
        self.fetch.side_effect = [(b'[]', deepcopy(OBSERVATION)), (b'[]', later)]
        self.download(max_bytes=100)
        self.assertEqual(self.fetch.call_args_list[-2].args[1], 100)
        self.assertEqual(self.fetch.call_args_list[-1].args[1], 98)


class Response(io.BytesIO):
    def __init__(self, body, status=200, headers=None):
        super().__init__(body)
        self.status = status
        self.headers = headers if headers is not None else {"Content-Type": "application/json", "Content-Length": str(len(body))}


class TransportTest(unittest.TestCase):
    def test_exact_bytes_headers_no_auth_and_no_redirects(self):
        body = b'[[0,1.00000000000000001,2,1,2,0]]'
        with patch.object(acquire.OPENER, "open", return_value=Response(body)) as opened:
            observed, metadata = acquire.fetch("https://api.exchange.coinbase.com/products/BTC-USD/candles?start=0&end=60&granularity=60", 1000)
        self.assertEqual(observed, body)
        self.assertEqual(metadata["status"], 200)
        request = opened.call_args.args[0]
        self.assertNotIn("Authorization", request.headers)
        self.assertNotIn("Cookie", request.headers)
        self.assertEqual(request.get_method(), "GET")
        self.assertIsNone(acquire.NoRedirect().redirect_request(None, None, 302, None, None, "https://elsewhere.example"))

    def test_limits_length_encoding_status_and_non_json_fail(self):
        cases = [(b'[]', 200, {"Content-Type": "application/json", "Content-Length": "101"}),
                 (b'[]', 200, {"Content-Type": "application/json", "Content-Length": "3"}),
                 (b'[]', 200, {"Content-Type": "application/json", "Content-Length": "-1"}),
                 (b'[]', 200, {"Content-Type": "application/json", "Content-Encoding": "gzip"}),
                 (b'[]', 200, {"Content-Type": "text/html"}),
                 (b'[]', 206, {"Content-Type": "application/json"}),
                 (b'x' * 100, 200, {"Content-Type": "application/json"})]
        for body, status, headers in cases:
            with self.subTest(status=status, headers=headers):
                with patch.object(acquire.OPENER, "open", return_value=Response(body, status, headers)), self.assertRaises(ValueError):
                    acquire.fetch("https://api.exchange.coinbase.com/", 100)
        with patch.object(acquire.OPENER, "open") as opened, self.assertRaises(ValueError):
            acquire.fetch("https://api.exchange.coinbase.com/", 0)
        opened.assert_not_called()

    def test_http_errors_and_body_failures_are_not_retried(self):
        for error in (urllib.error.HTTPError("url", 429, "rate limited", {}, None),
                      urllib.error.HTTPError("url", 402, "payment required", {}, None),
                      OSError("connection interrupted")):
            with patch.object(acquire.OPENER, "open", side_effect=error) as opened, self.assertRaises(Exception):
                acquire.fetch("https://api.exchange.coinbase.com/", 100)
            self.assertEqual(opened.call_count, 1)
        class Broken(Response):
            def read1(self, size):
                raise OSError("interrupted body")
        with patch.object(acquire.OPENER, "open", return_value=Broken(b'[]')) as opened, self.assertRaises(OSError):
            acquire.fetch("https://api.exchange.coinbase.com/", 100)
        self.assertEqual(opened.call_count, 1)

    def test_cli_providers_and_offline_plan_work_without_network(self):
        for args in (["providers"], ["plan", "--provider", "coinbase-candles", "--instrument", "BTC-USD",
                                      "--start-seconds", "0", "--end-seconds", "60", "--interval-seconds", "60"]):
            with patch.object(acquire.OPENER, "open") as opened, contextlib.redirect_stdout(io.StringIO()) as stdout:
                self.assertEqual(acquire.main(args), 0)
            self.assertTrue(json.loads(stdout.getvalue()))
            opened.assert_not_called()


if __name__ == "__main__":
    unittest.main()
