"""Source transfers are independently replayable artifacts for durable child tasks."""
import io
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import acquire
import hf_dataset
import providers
import snapshot
import source_units as units


OBSERVED = {"status": 200, "headers": {},
            "request_started_at": "2026-09-30T00:00:00Z",
            "retrieved_at": "2026-09-30T00:00:00Z"}


class SourceUnitsTest(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)

    def test_http_units_resume_independently_and_join_only_completed_source_observations(self):
        plan = acquire.plan("coinbase-candles", providers.Selection("BTC-USD", 0, 36000, 60))
        info = units.descriptor("http", plan)
        self.assertEqual((info["concurrency"], info["minIntervalMs"], len(info["units"])), (1, 350, 3))
        terms = self.root / "terms.txt"
        terms.write_text("Original test source terms")
        calls = []
        def fetch(url):
            index = next(i for i, request in enumerate(plan["requests"]) if request["url"] == url)
            calls.append(index)
            return json.dumps([[plan["requests"][index]["start_seconds"], 1, 3, 2, 2, 1]]).encode(), OBSERVED
        work = self.root / "units"
        output = self.root / "published"
        with patch.object(acquire, "fetch", side_effect=fetch):
            first = units.execute("http", plan, work, 0)
            self.assertEqual(units.execute("http", plan, work, 0), first)
            with self.assertRaises(FileNotFoundError):
                units.publish("http", plan, work, output, terms)
            self.assertFalse(output.exists())
            units.execute("http", plan, work, 1)
            units.execute("http", plan, work, 2)
            with patch.object(units, "validate", wraps=units.validate) as validate:
                result = units.publish("http", plan, work, output, terms)
                self.assertEqual(validate.call_count, 1, "publish validates the full plan once")
            self.assertEqual(units.publish("http", plan, work, output, terms), result)
        self.assertEqual(calls, [0, 1, 2])
        self.assertEqual(acquire.verify(output)["record_count"], 3)
        terms.write_text("Changed terms")
        with self.assertRaisesRegex(ValueError, "terms differ"):
            units.publish("http", plan, work, output, terms)
        terms.write_text("Original test source terms")
        # A self-consistent rewritten archive still cannot replace completed task artifacts.
        target = output / "raw/0000.json"
        target.write_bytes(target.read_bytes() + b" ")
        result["responses"][0]["file"]["size"] += 1
        result["raw_response_bytes"] += 1
        (output / "acquisition.json").write_bytes(acquire.json_bytes(result))
        self.assertEqual(acquire.verify(output)["record_count"], 3)
        with self.assertRaisesRegex(ValueError, "completed unit receipts"):
            units.publish("http", plan, work, output, terms)

    def test_unit_identity_and_source_bytes_cannot_change_on_replay(self):
        plan = acquire.plan("coinbase-candles", providers.Selection("BTC-USD", 0, 60, 60))
        work = self.root / "units"
        with patch.object(acquire, "fetch", return_value=(b"[[0,1,3,2,2,1]]", OBSERVED)):
            units.execute("http", plan, work, 0)
        other = acquire.plan("coinbase-candles", providers.Selection("ETH-USD", 0, 60, 60))
        with self.assertRaisesRegex(ValueError, "another plan"):
            units.execute("http", other, work, 0)
        (units.unit_root(work, 0) / "response.bin").write_bytes(b"[]")
        with self.assertRaisesRegex(ValueError, "differs from its receipt"):
            units.execute("http", plan, work, 0)

    def test_frozen_hf_file_tools_publish_original_manifest_without_batch_transport(self):
        commit = "a" * 40
        body = b"PAR1" + b"test" * 8 + b"PAR1"
        item = {"path": "data/a.parquet", "size": len(body), "format": "parquet",
                "url": snapshot.repository_file_url("example/history", commit, "data/a.parquet")}
        plan = {"schema": hf_dataset.PLAN_SCHEMA, "repository": "example/history",
                "requested_revision": "main", "revision": commit, "license": None,
                "partition_index": None, "request": {"includes": ["data/*.parquet"], "markets": [],
                    "start_date": None, "end_date": None}, "selection_bounds": "[start_date,end_date)",
                "download_granularity": "FILE_PARTITION", "coverage": "NOT_ASSERTED", "max_bytes": None,
                "total_bytes": len(body), "files": [item]}
        cache = self.root / "cache"
        work = self.root / "units"
        class Response(io.BytesIO):
            status = 200
            headers = {"Content-Length": str(len(body))}
        with patch.object(snapshot.urllib.request, "urlopen", return_value=Response(body)) as fetch:
            original = units.execute("hf", plan, work, 0, cache)
            self.assertEqual(units.execute("hf", plan, work, 0, cache), original)
            result = units.publish("hf", plan, work, self.root / "published", cache_dir=cache)
        self.assertEqual(fetch.call_count, 1)
        self.assertEqual(result["plan"], plan)
        self.assertEqual(hf_dataset.verify(self.root / "published/selection.json")["files"], 1)

    def test_hub_publication_points_importers_at_completed_original_files(self):
        body = b"original market source bytes"
        commit = "a" * 40
        plan = {"schema_version": 1, "repository": "example/history", "revision": commit,
                "license": "CC0-1.0", "total_bytes": len(body), "files": [{
                    "path": "day/source.bin", "size": len(body),
                    "url": snapshot.repository_file_url("example/history", commit, "day/source.bin")}]}
        class Response(io.BytesIO):
            pass
        with patch.object(snapshot.urllib.request, "urlopen", return_value=Response(body)):
            units.execute("hub", plan, self.root / "work", 0, self.root / "cache")
        result = units.publish("hub", plan, self.root / "work", self.root / "published")
        self.assertEqual((Path(result["source_root"]) / plan["files"][0]["path"]).read_bytes(), body)


if __name__ == "__main__":
    unittest.main()
