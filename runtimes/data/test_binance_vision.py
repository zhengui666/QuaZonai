"""Synthetic-only archive qualification; never accesses a data endpoint."""

import contextlib
from copy import deepcopy
import hashlib
import io
import json
import os
from pathlib import Path
import socket
import stat
import struct
import tempfile
import unittest
from unittest.mock import patch
import warnings
import zipfile

import acquire
import binance_vision as vision
import providers
import source_plugins


SELECTION = vision.Selection("BTCUSDT", "BTC", "USDT", "2025-01-01", "1m")
CLOCK = "2026-10-01T00:00:00Z"
SYNTHETIC = {"kind": "SYNTHETIC", "retrieval": None}
DECLARED = {"kind": "OPERATOR_DECLARED", "retrieval": {
    "checksum": {"started_at": "2026-09-30T00:00:00Z", "completed_at": "2026-09-30T00:00:01Z"},
    "archive": {"started_at": "2026-09-30T00:00:02Z", "completed_at": "2026-09-30T00:00:03.000000009Z"}}}


def row(selection=SELECTION, bucket=0):
    spec = vision.plan(selection)
    scale = 1000 if spec["source_timestamp_unit"] == "us" else 1_000_000
    opened = int(spec["start_ns"]) + bucket * spec["interval_seconds"] * 1_000_000_000
    return [str(opened // scale), "1.00000000000000000000000000001", "3.0000", "1.000",
            "2.00000000000000000000000000002", "9.00000000000000000000000000009",
            str((opened + spec["interval_seconds"] * 1_000_000_000) // scale - 1),
            "15.00000000000000000000000000015", "7", "1.0", "2.00", "0"]


def csv_bytes(rows):
    return ("\n".join(",".join(fields) for fields in rows) + ("\n" if rows else "")).encode()


def archive_bytes(body=None, selection=SELECTION, members=None, compression=zipfile.ZIP_DEFLATED):
    if body is None:
        body = csv_bytes([row(selection), row(selection, 2)])
    if members is None:
        members = [(vision.plan(selection)["member_name"], body)]
    output = io.BytesIO()
    with warnings.catch_warnings():
        warnings.simplefilter("ignore", UserWarning)
        with zipfile.ZipFile(output, "w", compression=compression) as archive:
            for member, data in members:
                archive.writestr(member, data)
    return output.getvalue()


def checksum(archive, selection=SELECTION):
    return f"{hashlib.sha256(archive).hexdigest()}  {vision.plan(selection)['archive_name']}\n".encode()


class ParsingTest(unittest.TestCase):
    def setUp(self):
        self.network = patch.object(socket, "create_connection", side_effect=AssertionError("network is forbidden"))
        self.network.start()
        self.addCleanup(self.network.stop)

    def decode(self, archive=None, selection=SELECTION):
        if archive is None:
            archive = archive_bytes(selection=selection)
        return vision.decode(archive, checksum(archive, selection), selection)

    def test_selection_is_general_fixed_identity_and_offline(self):
        for interval, seconds in vision.INTERVALS.items():
            for symbol, base, quote, day in [("ETHBTC", "ETH", "BTC", "2024-12-31"),
                                              ("SOLUSDC", "SOL", "USDC", "2025-01-02")]:
                selection = vision.Selection(symbol, base, quote, day, interval)
                result = self.decode(selection=selection) if seconds < 43200 else self.decode(
                    archive_bytes(csv_bytes([row(selection)]), selection), selection)
                self.assertEqual(result["rows"][0]["symbol"], symbol)
                self.assertEqual(result["counts"]["possible_buckets"], str(86400 // seconds))
        spec = vision.plan(SELECTION)
        self.assertEqual(spec["provider"]["venue"], "BINANCE")
        self.assertEqual(spec["artifact_scope"], "OFFLINE_LOCAL_IMPORT")
        self.assertEqual(set(spec["provider"]), {"id", "version", "venue", "market", "record_kind"})
        spec["provider"]["venue"] = "OTHER"
        self.assertEqual(vision.plan(SELECTION)["provider"]["venue"], "BINANCE")

    def test_invalid_selection_and_native_time_overflow(self):
        for field, values in {"symbol": ["../BTCUSDT", "btcUSDT", "BTCUSDT?x", "BTCUSD", "BTCUSDT\x00"],
                              "base_asset": ["", "BTCUSDT", "USDT"], "quote_asset": ["USD", "BTC"],
                              "day": ["2025-1-1", "2025-02-29", "1969-12-31", "9999-12-31"],
                              "interval": ["1s", "1w", "1M", "7m", 60]}.items():
            for value in values:
                with self.subTest(field=field, value=value), self.assertRaises(ValueError):
                    vision.plan(vision.Selection(**(vision.asdict(SELECTION) | {field: value})))

    def test_transition_clocks_exact_decimals_and_missing_buckets(self):
        for day, unit, scale in [("2024-12-31", "ms", 1_000_000), ("2025-01-01", "us", 1000)]:
            selection = vision.Selection("ETHBTC", "ETH", "BTC", day, "1m")
            decoded = self.decode(selection=selection)
            original = row(selection)
            first = decoded["rows"][0]
            self.assertEqual(first["open"], original[1])
            self.assertEqual(first["base_volume"], original[5])
            self.assertEqual(first["quote_volume"], original[7])
            self.assertEqual(first["source_timestamp_unit"], unit)
            self.assertEqual(int(first["event_end_ns"]) - int(first["source_close_inclusive_ns"]), scale)
            self.assertEqual(first["source_row_index"], "0")
            self.assertIsNone(first["observed_at"])
            self.assertIsNone(first["historical_available_at"])
            self.assertEqual(decoded["counts"]["missing_buckets"], "1438")
            self.assertEqual(decoded["counts"]["missing_bucket_open_ns"][0], str(int(first["event_end_ns"])))

    def test_full_day_and_empty_keep_actual_counts(self):
        complete = self.decode(archive_bytes(csv_bytes([row(bucket=i) for i in range(1440)])))
        self.assertEqual(complete["counts"]["rows"], "1440")
        self.assertEqual(complete["counts"]["missing_buckets"], "0")
        empty = self.decode(archive_bytes(b""))
        self.assertEqual(empty["counts"]["rows"], "0")
        self.assertEqual(empty["counts"]["missing_buckets"], "1440")

    def test_bad_source_clocks_duplicate_order_and_rowcounts(self):
        first = row()
        cases = [[first, first], [row(bucket=2), first], [row(bucket=-1)], [row(bucket=1440)],
                 [first] * 1441]
        for column, value in [(0, str(int(first[0]) + 1)), (6, str(int(first[6]) + 1)),
                              (0, str(int(first[0]) // 1000)), (6, "18446744073709551615")]:
            changed = first.copy()
            changed[column] = value
            cases.append([changed])
        for rows in cases:
            with self.subTest(rows=len(rows)), self.assertRaises(ValueError):
                self.decode(archive_bytes(csv_bytes(rows)))

    def test_malformed_amounts_relationships_and_fields(self):
        for column, values in {1: ["NaN", "Infinity", "1e-8", "+1", " 1", "1 ", "01", "0", "-0", ".1", "1.", "1" * 101],
                              2: ["0.5"], 3: ["2"], 5: ["-1"], 8: ["1.0", "-1", "01", "18446744073709551616"],
                              9: ["10"], 10: ["16"], 11: ["", "x" * 129]}.items():
            for value in values:
                fields = row()
                fields[column] = value
                with self.subTest(column=column, value=value), self.assertRaises(ValueError):
                    self.decode(archive_bytes(csv_bytes([fields])))
        body = csv_bytes([row()])
        for changed in [b"\xef\xbb\xbf" + body, body + b"\n", body.replace(b",", b",\t", 1),
                        body.replace(b"\n", b"\r"), body.replace(b",", b",\x00", 1),
                        b'"' + body, b'"one\ntwo",' + body, body + b"a" * 4097,
                        csv_bytes([row()[:-1]]), csv_bytes([row() + ["0"]])]:
            with self.subTest(body=changed[:20]), self.assertRaises(ValueError):
                self.decode(archive_bytes(changed))
        self.assertEqual(self.decode(archive_bytes(body.replace(b"\n", b"\r\n")))["counts"]["rows"], "1")

    def test_checksum_exact_entry_and_budgets(self):
        archive = archive_bytes()
        valid = checksum(archive)
        for changed in [b"", valid + valid, valid + b"\n", valid.replace(b"  BTC", b"  ../BTC"),
                        valid.replace(b"  BTC", b"  ETH"), b"0" * 64 + valid[64:], b"x" * 4097]:
            with self.subTest(checksum=changed[:80]), self.assertRaises(ValueError):
                vision.decode(archive, changed, SELECTION)
        self.assertEqual(vision.decode(archive, valid.replace(b"  ", b" *"), SELECTION)["counts"]["rows"], "2")
        with self.assertRaises(ValueError):
            self.decode(b"x" * (vision.LIMITS["archive_bytes"] + 1))

    def test_unsafe_members_compression_sizes_and_prefixes(self):
        name = vision.plan(SELECTION)["member_name"]
        body = csv_bytes([row()])
        for bad in ["../" + name, "/" + name, "C:" + name, "a/" + name, "a\\" + name, name + "/", "wrong.csv"]:
            with self.subTest(name=bad), self.assertRaises(ValueError):
                self.decode(archive_bytes(members=[(bad, body)]))
        for members in [[(name, body), (name, body)], [(name, body), ("extra", b"x")], []]:
            with self.assertRaises(ValueError):
                self.decode(archive_bytes(members=members))
        for mode in [stat.S_IFLNK | 0o777, stat.S_IFIFO | 0o600, stat.S_IFCHR | 0o600, stat.S_IFDIR | 0o700]:
            member = zipfile.ZipInfo(name)
            member.create_system = 3
            member.external_attr = mode << 16
            with self.assertRaises(ValueError):
                self.decode(archive_bytes(members=[(member, body)]))
        with self.assertRaises(ValueError):
            self.decode(archive_bytes(compression=zipfile.ZIP_BZIP2))
        with self.assertRaises(ValueError):
            self.decode(b"prefix" + archive_bytes())
        with self.assertRaises(ValueError):
            self.decode(archive_bytes(b"x" * (vision.LIMITS["csv_bytes"] + 1)))
        self.assertEqual(self.decode(archive_bytes(compression=zipfile.ZIP_STORED))["counts"]["rows"], "2")

    def test_corrupt_crc_truncation_encryption_and_nul(self):
        archive = archive_bytes(compression=zipfile.ZIP_STORED)
        # Mutate synthetic ZIP records only in tests. Production never parses ZIP structures.
        central = archive.index(b"PK\x01\x02")
        payload = archive.index(csv_bytes([row(), row(bucket=2)]))
        corrupted = bytearray(archive)
        corrupted[payload + 17] ^= 1
        encrypted = bytearray(archive)
        struct.pack_into("<H", encrypted, 6, 1)
        struct.pack_into("<H", encrypted, central + 8, 1)
        nul = archive.replace(b".csv", b"\x00csv")
        name = vision.plan(SELECTION)["member_name"]
        alias = archive_bytes(members=[(name + "X../alias", csv_bytes([row()]))])
        alias = alias.replace((name + "X../alias").encode(), (name + "\x00../alias").encode())
        for changed in [bytes(corrupted), bytes(encrypted), nul, alias, archive[:-1], archive[:-22], archive[:40], b""]:
            with self.assertRaises(ValueError):
                self.decode(changed)

    def test_stdlib_container_limitations_are_explicit_not_canonical_validation(self):
        archive = archive_bytes()
        self.assertEqual(self.decode(archive + b"uninterpreted trailer")["counts"]["rows"], "2")
        stream = io.BytesIO()
        with zipfile.ZipFile(stream, "w") as container:
            with container.open(vision.plan(SELECTION)["member_name"], "w", force_zip64=True) as member:
                member.write(csv_bytes([row()]))
        self.assertEqual(self.decode(stream.getvalue())["counts"]["rows"], "1")
        self.assertEqual(vision.plan(SELECTION)["container_validation"], "STDLIB_MEMBER_VALIDATION_NOT_CANONICAL_ZIP")


class BundleTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.archive = self.root / "operator.zip"
        self.checksum = self.root / "operator.CHECKSUM"
        self.output = self.root / "frozen"
        self.provenance = self.root / "declaration.json"
        self.archive.write_bytes(archive_bytes())
        self.checksum.write_bytes(checksum(self.archive.read_bytes()))
        self.provenance.write_bytes(vision.json_bytes(SYNTHETIC))
        self.clock = patch.object(vision, "now", return_value=CLOCK)
        self.clock.start()
        self.addCleanup(self.clock.stop)
        self.network = patch.object(socket, "create_connection", side_effect=AssertionError("network is forbidden"))
        self.network.start()
        self.addCleanup(self.network.stop)

    def freeze(self, **kwargs):
        return vision.freeze(SELECTION, self.archive, self.checksum, self.output, **kwargs)

    def rewrite(self, manifest):
        (self.output / "archive.json").write_bytes(vision.json_bytes(manifest))

    def test_original_bytes_reproduced_synthetic_and_unknown_are_distinct(self):
        original = self.archive.read_bytes()
        evidence = self.root / "license.txt"
        evidence.write_bytes(b"Synthetic license text; no real data permission.\r\n")
        manifest = self.freeze(provenance_path=self.provenance, evidence_paths={"license": evidence})
        self.assertEqual((self.output / "raw/archive.zip").read_bytes(), original)
        self.assertEqual((self.output / "provenance.json").read_bytes(), self.provenance.read_bytes())
        self.assertEqual((self.output / "evidence/license.bin").read_bytes(), evidence.read_bytes())
        self.assertEqual(manifest["provenance_kind"], "SYNTHETIC")
        self.assertEqual(vision.verify(self.output)["integrity"], "VERIFIED")
        self.assertEqual(manifest["evidence"]["license"]["status"], "OPERATOR_SUPPLIED_UNVERIFIED")
        self.assertIsNone(manifest["implementation_revision"])
        self.assertFalse(manifest["admission"]["research_qualified"])
        self.output = self.root / "unknown"
        unknown = self.freeze()
        self.assertEqual(unknown["provenance_kind"], "UNKNOWN")
        rows = [json.loads(line) for line in (self.output / "records.jsonl").read_text().splitlines()]
        self.assertTrue(all(r["observed_at"] is None and r["declared_observed_at"] is None for r in rows))
        self.assertEqual(vision.verify(self.output)["provenance_status"], "UNKNOWN")

    def test_declared_original_clocks_retained_never_attested(self):
        self.provenance.write_bytes(vision.json_bytes(DECLARED))
        manifest = self.freeze(provenance_path=self.provenance)
        self.assertEqual(manifest["provenance_status"], "DECLARED_UNVERIFIED")
        rows = [json.loads(line) for line in (self.output / "records.jsonl").read_text().splitlines()]
        for row_ in rows:
            self.assertIsNone(row_["observed_at"])
            self.assertEqual(row_["declared_observed_at"], DECLARED["retrieval"]["archive"]["completed_at"])
        self.assertEqual(vision.utc_ns(rows[0]["declared_observed_at"]) % 1_000_000_000, 9)
        self.assertEqual(vision.verify(self.output)["integrity"], "VERIFIED")

    def test_provenance_clock_failures_and_false_attestations(self):
        cases = [{"kind": "ATTESTED", "retrieval": None}, {"kind": "UNKNOWN", "retrieval": DECLARED["retrieval"]},
                 {"kind": "SYNTHETIC"}, {"kind": "OPERATOR_DECLARED", "retrieval": {}}]
        for role, key, value in [("checksum", "started_at", "2025-01-01T00:00:00Z"),
                                 ("archive", "started_at", "2026-09-30T00:00:00Z"),
                                 ("archive", "completed_at", "2026-10-02T00:00:00Z"),
                                 ("checksum", "completed_at", "2026-09-30T00:00:00+01:00")]:
            changed = deepcopy(DECLARED)
            changed["retrieval"][role][key] = value
            cases.append(changed)
        for value in cases:
            self.provenance.write_bytes(vision.json_bytes(value))
            with self.subTest(value=value), self.assertRaises(ValueError):
                self.freeze(provenance_path=self.provenance)
            self.assertFalse(self.output.exists())
        with patch.object(vision, "now", return_value="2025-01-01T00:00:00Z"), self.assertRaises(ValueError):
            self.freeze()

    def test_self_consistent_future_import_and_publication_are_rejected(self):
        manifest = self.freeze()
        manifest["imported_at"] = "2026-10-02T00:00:00Z"
        manifest["published_at"] = "2026-10-02T00:00:01Z"
        self.rewrite(manifest)
        with self.assertRaisesRegex(ValueError, "future"):
            vision.verify(self.output)

    def test_manifest_provider_schema_urls_clocks_lineage_and_evidence_forgery(self):
        original = self.freeze(provenance_path=self.provenance)
        mutations = [("schema", "qz.public_acquisition/1"), ("provider", original["provider"] | {"venue": "COINBASE"}),
                     ("provider", original["provider"] | {"version": 2}), ("provider", original["provider"] | {"id": "custom"}),
                     ("source_references", []), ("published_at", "2025-01-01T00:00:00Z"),
                     ("provenance_kind", "ATTESTED"), ("counts", original["counts"] | {"rows": "99"}),
                     ("admission", original["admission"] | {"research_qualified": True}),
                     ("evidence", original["evidence"] | {"license": {"status": "VERIFIED"}})]
        for key, value in mutations:
            self.rewrite(original | {key: value})
            with self.subTest(key=key), self.assertRaises(ValueError):
                vision.verify(self.output)
        self.rewrite(original)
        path = self.output / "records.jsonl"
        rows = [json.loads(line) for line in path.read_text().splitlines()]
        rows[0]["source_row_index"] = "1"
        body = b"".join(vision.json_bytes(value) for value in rows)
        path.write_bytes(body)
        original["files"]["records.jsonl"] = vision.file_record("records.jsonl", body)
        self.rewrite(original)
        with self.assertRaisesRegex(ValueError, "do not reproduce"):
            vision.verify(self.output)

    def test_forged_file_paths_hashes_unknown_fields_and_duplicate_json(self):
        original = self.freeze()
        for changed in [original | {"files": {"../outside": {}}}, original | {"extra": True},
                        original | {"files": original["files"] | {"records.jsonl": {"sha256": "0" * 64}}}]:
            self.rewrite(changed)
            with self.assertRaises(ValueError):
                vision.verify(self.output)
        self.rewrite(original)
        (self.output / "archive.json").write_bytes(b'{"schema":"x",' + vision.json_bytes(original)[1:])
        with self.assertRaisesRegex(ValueError, "duplicate"):
            vision.verify(self.output)

    def test_verification_rechecks_its_inputs_before_returning(self):
        self.freeze()
        original_read = vision.local_bytes
        count = 0
        def changed_read(path, limit):
            nonlocal count
            body = original_read(path, limit)
            if Path(path).name == "records.jsonl":
                count += 1
                if count == 1:
                    (self.output / "raw/archive.zip").write_bytes(b"changed after first read")
            return body
        with patch.object(vision, "local_bytes", side_effect=changed_read), self.assertRaisesRegex(ValueError, "changed"):
            vision.verify(self.output)

    def test_symlink_fifo_and_oversized_local_inputs(self):
        alias = self.root / "alias.zip"
        alias.symlink_to(self.archive)
        with self.assertRaises(ValueError):
            vision.local_bytes(alias, vision.LIMITS["archive_bytes"])
        directory = self.root / "aliased"
        directory.symlink_to(self.root, target_is_directory=True)
        with self.assertRaises(ValueError):
            vision.local_bytes(directory / self.archive.name, vision.LIMITS["archive_bytes"])
        if hasattr(os, "mkfifo"):
            fifo = self.root / "fifo"
            os.mkfifo(fifo)
            with self.assertRaises(ValueError):
                vision.local_bytes(fifo, 10)
        with self.assertRaises(ValueError):
            vision.local_bytes(self.archive, 1)
        self.freeze()
        frozen = self.output / "raw/archive.zip"
        frozen.unlink()
        frozen.symlink_to(self.archive)
        with self.assertRaises(ValueError):
            vision.verify(self.output)

    def test_no_overwrite_no_resume_and_bad_input_no_publication(self):
        self.output.mkdir()
        sentinel = self.output / "keep"
        sentinel.write_text("original")
        with self.assertRaises(FileExistsError):
            self.freeze()
        self.assertEqual(sentinel.read_text(), "original")
        self.output = self.root / "bad"
        self.checksum.write_bytes(b"wrong")
        with self.assertRaises(ValueError):
            self.freeze()
        self.assertFalse((self.output / "archive.json").exists())

    def test_changed_inputs_or_partial_output_never_publish_final_manifest(self):
        original_publish = vision.publish_bytes
        for scenario in ("input", "output", "interruption"):
            self.output = self.root / scenario
            self.archive.write_bytes(archive_bytes())
            self.checksum.write_bytes(checksum(self.archive.read_bytes()))
            def changed_publish(path, body):
                original_publish(path, body)
                if path.name == "records.jsonl":
                    if scenario == "input":
                        self.archive.write_bytes(b"changed")
                    elif scenario == "output":
                        (self.output / "raw/archive.zip").write_bytes(b"changed")
                    else:
                        raise OSError("synthetic interruption")
            with patch.object(vision, "publish_bytes", side_effect=changed_publish), self.assertRaises((ValueError, OSError)):
                self.freeze()
            self.assertFalse((self.output / "archive.json").exists())

    def test_cli_offline_plan_inspect_freeze_verify_and_absent_capabilities(self):
        args = ["--symbol", "BTCUSDT", "--base-asset", "BTC", "--quote-asset", "USDT",
                "--day", "2025-01-01", "--interval", "1m"]
        for command, options in [("plan", args), ("inspect", args + ["--archive", str(self.archive), "--checksum", str(self.checksum)]),
                                  ("freeze", args + ["--archive", str(self.archive), "--checksum", str(self.checksum),
                                                       "--output", str(self.output), "--provenance", str(self.provenance)]),
                                  ("verify", ["--output", str(self.output)])]:
            with contextlib.redirect_stdout(io.StringIO()) as stdout:
                self.assertEqual(vision.main([command, *options]), 0)
            parsed = json.loads(stdout.getvalue())
            if command == "inspect":
                self.assertNotIn("rows", parsed)
            if command == "verify":
                self.assertEqual(parsed["integrity"], "VERIFIED")
        for command in ("download", "convert", "prepare"):
            with contextlib.redirect_stderr(io.StringIO()), self.assertRaises(SystemExit):
                vision.main([command])
        self.assertNotIn(PROVIDER_ID := vision.PROVIDER["id"], providers.PROVIDERS)
        self.assertNotIn(PROVIDER_ID, source_plugins.PLUGINS)

    def test_old_acquisition_contract_remains_independent(self):
        old = acquire.plan("coinbase-candles", providers.Selection("BTC-USD", 0, 60, 60))
        frozen = vision.json_bytes(old)
        self.freeze()
        self.assertEqual(acquire.SCHEMA, "qz.public_acquisition/1")
        self.assertEqual(frozen, vision.json_bytes(acquire.plan("coinbase-candles", providers.Selection("BTC-USD", 0, 60, 60))))
        self.assertEqual(old["provider"]["native_conversion"], "UNSUPPORTED")


if __name__ == "__main__":
    unittest.main()
