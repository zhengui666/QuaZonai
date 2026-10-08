"""Offline partition selection, cache, interruption and transport regressions."""

from concurrent.futures import ThreadPoolExecutor
import contextlib
import csv
import io
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
from pathlib import Path
import tempfile
import threading
import time
import unittest
from unittest.mock import patch

import hf_dataset
import snapshot
import source_plugins


COMMIT = "a" * 40
DATASET = "example/history"
API = f"{snapshot.HUB}/api/datasets/{DATASET}"
BASE = f"{snapshot.HUB}/datasets/{DATASET}/resolve/{COMMIT}/"


class Response(io.BytesIO):
    def __init__(self, body, status=200, headers=None):
        super().__init__(body)
        self.status = status
        self.headers = {"Content-Length": str(len(body)), **(headers or {})}


class HfDatasetTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.cache = self.root / "cache"
        self.output = self.root / "request"
        self.index = {"schema": hf_dataset.INDEX_SCHEMA, "files": [
            {"path": "data/a.parquet", "markets": ["market-a"],
             "start_date": "2026-09-01", "end_date": "2026-09-02", "format": "parquet"},
            {"path": "data/b.parquet", "markets": ["market-a"],
             "start_date": "2026-09-02", "end_date": "2026-09-03", "format": "parquet"},
            {"path": "data/c.parquet", "markets": ["market-b", "market-c"],
             "start_date": "2026-09-01", "end_date": "2026-09-03", "format": "parquet"}]}
        self.content = {f"data/{name}.parquet": b"PAR1" + name.encode() * 32 + b"PAR1" for name in "abc"}
        self.content["README.md"] = b"# Ordinary terms, no license declared"
        self.sync_index()
        self.requests = []
        self.http = patch.object(snapshot.urllib.request, "urlopen", side_effect=self.respond).start()
        self.addCleanup(patch.stopall)

    def sync_index(self):
        self.content["partitions.json"] = json.dumps(self.index).encode()
        self.metadata = {"sha": COMMIT, "private": False, "gated": False, "cardData": {},
                         "siblings": [{"rfilename": p, "size": len(b)} for p, b in self.content.items()]}

    def respond(self, request, timeout):
        self.assertEqual(timeout, 60)
        url = request if isinstance(request, str) else request.full_url
        headers = {} if isinstance(request, str) else dict(request.header_items())
        self.requests.append((url, headers))
        if url == API + f"/revision/{COMMIT}?blobs=true":
            return Response(json.dumps(self.metadata).encode())
        if url.startswith(API):
            return Response(json.dumps({"sha": COMMIT}).encode())
        if url.startswith(BASE):
            body = self.content[url.removeprefix(BASE)]
            if "Range" in headers:
                offset = int(headers["Range"].removeprefix("bytes=").removesuffix("-"))
                return Response(body[offset:], 206, {"Content-Range": f"bytes {offset}-{len(body)-1}/{len(body)}"})
            return Response(body)
        self.fail("unexpected request")

    def plan(self, **kwargs):
        return hf_dataset.plan(DATASET, manifest="partitions.json", markets=["market-a"],
                               start_date="2026-09-01", end_date="2026-09-02", **kwargs)

    def data_requests(self):
        return [(url, headers) for url, headers in self.requests if url.startswith(BASE + "data/")]

    def test_plan_resolves_ordinary_ref_reads_only_real_index_and_requested_files(self):
        selection = self.plan(revision="release/date")
        self.assertEqual(self.requests[0][0], API + "/revision/release%2Fdate")
        self.assertEqual(selection["requested_revision"], "release/date")
        self.assertEqual(selection["revision"], COMMIT)
        self.assertEqual([f["path"] for f in selection["files"]], ["data/a.parquet"])
        self.assertEqual(selection["download_granularity"], "FILE_PARTITION")
        self.assertIsNone(selection["license"])
        self.assertEqual(self.data_requests(), [])
        self.assertFalse(self.cache.exists())

    def test_download_and_overlapping_queries_reuse_one_cache_without_checksums(self):
        selection = self.plan()
        result = hf_dataset.download(selection, self.cache, self.output)
        self.assertEqual(len(self.data_requests()), 1)
        self.assertEqual(result["cached_files"], 0)
        self.assertEqual(result["files"][0]["validation"], "PARQUET_ENVELOPE")
        self.assertNotIn("sha256", json.dumps(result))
        self.assertNotIn("README.md", [f["path"] for f in result["files"]])
        self.assertEqual(Path(result["files"][0]["local_path"]).read_bytes(), self.content["data/a.parquet"])
        self.requests.clear()
        self.assertEqual(hf_dataset.download(selection, self.cache, self.output), result)
        self.assertEqual(self.requests, [])
        next_selection = hf_dataset.plan(DATASET, manifest="partitions.json", markets=["market-a"])
        self.requests.clear()
        next_result = hf_dataset.download(next_selection, self.cache, self.root / "next")
        self.assertEqual(next_result["cached_files"], 1)
        self.assertEqual([url for url, _ in self.data_requests()], [BASE + "data/b.parquet"])
        self.assertEqual(hf_dataset.verify(self.output / "selection.json")["files"], 1)

    def test_file_partition_is_reported_when_selected_market_shares_file(self):
        selection = hf_dataset.plan(DATASET, manifest="partitions.json", markets=["market-c"])
        self.assertEqual([f["path"] for f in selection["files"]], ["data/c.parquet"])
        self.assertEqual(selection["files"][0]["partition"]["markets"], ["market-b", "market-c"])

    def test_unknown_market_and_half_open_date_boundary_are_not_silently_omitted(self):
        with self.assertRaisesRegex(ValueError, "absent"):
            hf_dataset.plan(DATASET, manifest="partitions.json", markets=["market-a", "unknown"])
        selection = hf_dataset.plan(DATASET, manifest="partitions.json", markets=["market-a"],
                                    start_date="2026-09-02", end_date="2026-09-03")
        self.assertEqual([f["path"] for f in selection["files"]], ["data/b.parquet"])
        self.assertEqual(selection["coverage"], "NOT_ASSERTED")
        with self.assertRaisesRegex(ValueError, "no indexed files"):
            hf_dataset.plan(DATASET, manifest="partitions.json", markets=["market-a"],
                            start_date="2026-09-03", end_date="2026-09-04")
        self.assertEqual(self.data_requests(), [])

    def test_explicit_file_selection_does_not_guess_market_dates_or_add_evidence_files(self):
        result = hf_dataset.plan(DATASET, includes=["data/b.parquet"])
        self.assertEqual([f["path"] for f in result["files"]], ["data/b.parquet"])
        self.assertIsNone(result["partition_index"])
        self.assertNotIn(BASE + "partitions.json", [url for url, _ in self.requests])
        self.requests.clear()
        for options in ({}, {"markets": ["market-a"], "includes": ["data/*"]},
                        {"includes": ["data/*"], "start_date": "2026-09-01"},
                        {"manifest": "partitions.json"}):
            with self.subTest(options=options), self.assertRaises(ValueError):
                hf_dataset.plan(DATASET, **options)
        self.assertEqual(self.requests, [])

    def unknown_first_partition(self):
        self.index["files"][0].update(markets=[], market_mapping="UNKNOWN")
        self.sync_index()

    def test_unknown_market_date_selection_preserves_mapping_through_cache_and_verify(self):
        self.unknown_first_partition()
        selection = hf_dataset.plan(DATASET, manifest="partitions.json", includes=["data/a.parquet"],
                                    start_date="2026-09-01", end_date="2026-09-02")
        partition = selection["files"][0]["partition"]
        self.assertEqual(partition["market_mapping"], "UNKNOWN")
        self.assertEqual(partition["markets"], [])
        self.assertEqual(selection["coverage"], "NOT_ASSERTED")
        result = hf_dataset.download(selection, self.cache, self.output)
        self.assertEqual(result["files"][0]["partition"], partition)
        self.assertEqual(hf_dataset.verify(self.output / "selection.json")["files"], 1)
        reused = hf_dataset.download(selection, self.cache, self.root / "reused")
        self.assertEqual(reused["cached_files"], 1)
        self.assertEqual(len(self.data_requests()), 1)
        with self.assertRaisesRegex(ValueError, "exceeding"):
            hf_dataset.plan(DATASET, manifest="partitions.json", includes=["data/a.parquet"], max_bytes=1)

    def test_unknown_market_partition_never_matches_market_or_all_selectors(self):
        self.unknown_first_partition()
        for market in ("market-a", "ALL", "*", "UNKNOWN"):
            with self.subTest(market=market), self.assertRaisesRegex(ValueError, "known membership"):
                hf_dataset.plan(DATASET, manifest="partitions.json", markets=[market],
                                start_date="2026-09-01", end_date="2026-09-02")
        self.assertEqual(self.data_requests(), [])

    def test_unknown_mapping_must_be_explicit_empty_and_cannot_claim_all_markets(self):
        original = dict(self.index["files"][0])
        for update in ({"markets": []}, {"markets": None, "market_mapping": "UNKNOWN"},
                       {"markets": [], "market_mapping": None}, {"markets": [], "market_mapping": "unknown"},
                       {"markets": ["ALL"], "market_mapping": "UNKNOWN"},
                       {"markets": ["*"], "market_mapping": "UNKNOWN"}):
            self.index["files"][0] = {**original, **update}
            self.sync_index()
            with self.subTest(update=update), self.assertRaisesRegex(ValueError, "market mapping"):
                hf_dataset.plan(DATASET, manifest="partitions.json", includes=["data/a.parquet"])
        self.assertEqual(self.data_requests(), [])

    def test_unknown_mapping_outside_date_or_include_does_not_block_known_market(self):
        self.unknown_first_partition()
        for options in ({"start_date": "2026-09-02", "end_date": "2026-09-03"},
                        {"includes": ["data/b.parquet"]}):
            selection = hf_dataset.plan(DATASET, manifest="partitions.json", markets=["market-a"], **options)
            self.assertEqual([f["path"] for f in selection["files"]], ["data/b.parquet"])
            hf_dataset.validate_plan(selection)
        # A literal legacy market identifier never acquires wildcard meaning.
        with self.assertRaisesRegex(ValueError, "absent"):
            hf_dataset.plan(DATASET, manifest="partitions.json", markets=["ALL"], includes=["data/b.parquet"])

    def test_unknown_mapping_handoff_cannot_be_changed_into_market_filter(self):
        self.unknown_first_partition()
        selection = hf_dataset.plan(DATASET, manifest="partitions.json", includes=["data/a.parquet"])
        for change in ("request", "pseudo_all", "missing_marker"):
            altered = json.loads(json.dumps(selection))
            if change == "request":
                altered["request"]["markets"] = ["market-a"]
            elif change == "pseudo_all":
                altered["files"][0]["partition"]["markets"] = ["ALL"]
            else:
                del altered["files"][0]["partition"]["market_mapping"]
            with self.subTest(change=change), self.assertRaises(ValueError):
                hf_dataset.validate_plan(altered)

    def test_budget_and_missing_partition_mapping_fail_before_data_download(self):
        with self.assertRaisesRegex(ValueError, "exceeding"):
            self.plan(max_bytes=1)
        for update in ({"path": "data/missing.parquet"}, {"markets": []},
                       {"start_date": "2026-09-02"}, {"format": "guessed"}, {"path": "../outside"}):
            original = dict(self.index["files"][0])
            self.index["files"][0].update(update)
            self.sync_index()
            with self.subTest(update=update), self.assertRaises(ValueError):
                self.plan()
            self.index["files"][0] = original
        self.assertEqual(self.data_requests(), [])

    def interrupted_response(self, request, timeout):
        url = request if isinstance(request, str) else request.full_url
        if url == BASE + "data/a.parquet":
            self.requests.append((url, dict(request.header_items())))
            class Interrupted(Response):
                def read(self, size):
                    if self.tell():
                        raise OSError("connection interrupted")
                    return super().read(7)
            return Interrupted(self.content["data/a.parquet"])
        return self.respond(request, timeout)

    def test_interruption_preserves_partial_and_retry_uses_exact_http_range(self):
        selection = self.plan()
        self.http.side_effect = self.interrupted_response
        with self.assertRaises(OSError):
            hf_dataset.download(selection, self.cache, self.output)
        root = hf_dataset.cache_root(self.cache, selection)
        self.assertFalse((root / "files/data/a.parquet").exists())
        self.assertEqual((root / "transfers/data/a.parquet/data.partial").stat().st_size, 7)
        self.assertFalse((self.output / "selection.json").exists())
        self.requests.clear()
        self.http.side_effect = self.respond
        result = hf_dataset.download(selection, self.cache, self.output)
        self.assertEqual(self.data_requests()[0][1]["Range"], "bytes=7-")
        self.assertEqual(result["files"][0]["resumed_bytes"], 7)
        self.assertEqual(result["downloaded_bytes"], len(self.content["data/a.parquet"]) - 7)
        self.assertFalse((root / "transfers/data/a.parquet/data.partial").exists())

    def create_partial(self, selection, body):
        root = hf_dataset.cache_root(self.cache, selection)
        partial = snapshot.safe_local(root, "transfers/data/a.parquet/data.partial")
        partial.parent.mkdir(parents=True)
        partial.write_bytes(body)
        snapshot.replace_json(partial.with_name("state.json"),
                              {k: selection["files"][0][k] for k in ("url", "size", "format")})
        return root

    def test_full_size_unpublished_partial_checks_last_byte_then_publishes(self):
        selection = self.plan()
        body = self.content["data/a.parquet"]
        self.create_partial(selection, body)
        result = hf_dataset.download(selection, self.cache, self.output)
        self.assertEqual(self.data_requests()[0][1]["Range"], f"bytes={len(body)-1}-")
        self.assertEqual(result["downloaded_bytes"], 1)

    def test_ignored_range_restarts_without_duplicate_append(self):
        selection = self.plan()
        self.create_partial(selection, self.content["data/a.parquet"][:7])
        def ignored_range(request, timeout):
            if not isinstance(request, str):
                return Response(self.content["data/a.parquet"])
            return self.respond(request, timeout)
        self.http.side_effect = ignored_range
        result = hf_dataset.download(selection, self.cache, self.output)
        self.assertEqual(result["files"][0]["resumed_bytes"], 0)
        self.assertEqual(Path(result["files"][0]["local_path"]).read_bytes(), self.content["data/a.parquet"])

    def test_incorrect_range_keeps_partial_and_does_not_publish(self):
        selection = self.plan()
        root = self.create_partial(selection, self.content["data/a.parquet"][:7])
        self.http.side_effect = lambda *_a, **_kw: Response(b"wrong", 206, {"Content-Range": "bytes 0-4/5"})
        with self.assertRaisesRegex(ValueError, "unexpected byte range"):
            hf_dataset.download(selection, self.cache, self.output)
        self.assertEqual((root / "transfers/data/a.parquet/data.partial").read_bytes(), self.content["data/a.parquet"][:7])
        self.assertFalse((self.output / "selection.json").exists())

    def test_truncation_preserves_bytes_and_oversize_never_publishes(self):
        selection = self.plan()
        original = self.content["data/a.parquet"]
        for body, message in ((original[:7], "truncated"), (original + b"extra", "exceeds")):
            with self.subTest(message=message), tempfile.TemporaryDirectory() as directory:
                self.http.side_effect = lambda *_a, **_kw: Response(body, headers={"Content-Length": None})
                with self.assertRaisesRegex(ValueError, message):
                    hf_dataset.download(selection, Path(directory) / "cache", Path(directory) / "out")
                self.assertFalse(list(Path(directory).rglob("selection.json")))

    def test_format_rejection_preserves_source_bytes_and_retry_can_recover(self):
        selection = self.plan()
        original = self.content["data/a.parquet"]
        self.content["data/a.parquet"] = b"<html>" + b"x" * (len(original) - 6)
        with self.assertRaisesRegex(ValueError, "Parquet header"):
            hf_dataset.download(selection, self.cache, self.output)
        root = hf_dataset.cache_root(self.cache, selection)
        self.assertEqual(len(list(root.rglob("*.rejected-*"))), 1)
        self.assertFalse((root / "files/data/a.parquet").exists())
        self.content["data/a.parquet"] = original
        hf_dataset.download(selection, self.cache, self.output)
        self.assertEqual((root / "files/data/a.parquet").read_bytes(), original)

    def test_existing_corruption_symlinks_and_changed_requests_are_preserved(self):
        selection = self.plan()
        result = hf_dataset.download(selection, self.cache, self.output)
        target = Path(result["files"][0]["local_path"])
        target.write_bytes(b"short")
        with self.assertRaisesRegex(ValueError, "byte size"):
            hf_dataset.download(selection, self.cache, self.root / "next")
        self.assertEqual(target.read_bytes(), b"short")
        target.unlink()
        outside = self.root / "outside"
        outside.write_bytes(self.content["data/a.parquet"])
        target.symlink_to(outside)
        with self.assertRaisesRegex(ValueError, "symlinks"):
            hf_dataset.verify(self.output / "selection.json")
        self.assertEqual(outside.read_bytes(), self.content["data/a.parquet"])
        target.unlink()
        target.write_bytes(self.content["data/a.parquet"])
        different = hf_dataset.plan(DATASET, manifest="partitions.json", markets=["market-a"],
                                    start_date="2026-09-01", end_date="2026-09-03")
        with self.assertRaisesRegex(ValueError, "different request"):
            hf_dataset.download(different, self.cache, self.output)

    def test_duplicate_concurrent_requests_share_one_completed_file(self):
        selection = self.plan()
        def slower_response(request, timeout):
            if not isinstance(request, str):
                time.sleep(0.03)
            return self.respond(request, timeout)
        self.http.side_effect = slower_response
        with ThreadPoolExecutor(max_workers=2) as executor:
            futures = [executor.submit(hf_dataset.download, selection, self.cache, self.root / f"out-{i}")
                       for i in range(2)]
            results = [f.result() for f in futures]
        self.assertEqual(len(self.data_requests()), 1)
        self.assertEqual(sum(r["cached_files"] for r in results), 1)

    def test_same_output_concurrency_and_empty_directory_crash_recover(self):
        selection = self.plan()
        self.output.mkdir()  # Simulate a crash after mkdir, before request publication.
        with ThreadPoolExecutor(max_workers=2) as executor:
            futures = [executor.submit(hf_dataset.download, selection, self.cache, self.output) for _ in range(2)]
            results = [f.result() for f in futures]
        self.assertEqual(results[0], results[1])
        self.assertEqual(len(self.data_requests()), 1)
        self.assertEqual(hf_dataset.verify(self.output / "selection.json")["files"], 1)

    def test_retry_accepts_pinned_revision_alias_and_preserves_original_request(self):
        selection = self.plan()
        self.http.side_effect = self.interrupted_response
        with self.assertRaises(OSError):
            hf_dataset.download(selection, self.cache, self.output)
        self.http.side_effect = self.respond
        pinned = self.plan(revision=COMMIT)
        result = hf_dataset.download(pinned, self.cache, self.output)
        self.assertEqual(result["plan"], selection)
        self.assertEqual(snapshot.fetch_local_manifest(self.output / "request.json"), selection)
        self.assertEqual(hf_dataset.download(pinned, self.cache, self.output), result)

    def test_valid_source_paths_do_not_collide_with_cache_control_names(self):
        self.content.update({"a": b"one", "a.lock/b": b"two", "a.partial/c": b"three"})
        self.sync_index()
        selection = hf_dataset.plan(DATASET, includes=["a", "a.lock/b", "a.partial/c"])
        result = hf_dataset.download(selection, self.cache, self.output)
        self.assertEqual([Path(f["local_path"]).read_bytes() for f in result["files"]], [b"one", b"two", b"three"])

    def test_csv_first_logical_record_supports_multiline_quotes_without_size_cap(self):
        path = self.root / "multiline.csv"
        body = b'"column\nname",other\n1,2\n'
        path.write_bytes(body)
        self.assertEqual(hf_dataset.inspect_file(path, {"size": len(body), "format": "csv"}), "CSV_FIRST_ROW")
        body = b'"' + b'x' * 200_000 + b'",other\n'
        path.write_bytes(body)
        original_limit = csv.field_size_limit()
        with ThreadPoolExecutor(max_workers=2) as executor:
            results = list(executor.map(lambda _: hf_dataset.inspect_file(path, {"size": len(body), "format": "csv"}), range(2)))
        self.assertEqual(results, ["CSV_FIRST_ROW", "CSV_FIRST_ROW"])
        self.assertEqual(csv.field_size_limit(), original_limit)
        body = b'"' + b'x' * snapshot.CHUNK + b'",other\n'
        path.write_bytes(body)
        self.assertEqual(hf_dataset.inspect_file(path, {"size": len(body), "format": "csv"}), "CSV_FIRST_ROW")
        self.assertEqual(path.read_bytes(), body)
        self.assertEqual(csv.field_size_limit(), original_limit)

    def test_final_manifest_is_fully_preserved_and_verified_without_capacity_gate(self):
        selection = self.plan()
        self.requests.clear()
        result = hf_dataset.download(selection, self.cache, self.output)
        self.assertEqual(json.loads((self.output / "selection.json").read_bytes()), result)
        self.assertEqual(hf_dataset.verify(self.output / "selection.json")["files"], len(selection["files"]))
        self.assertEqual(len(self.requests), len(selection["files"]))
        self.requests.clear()
        self.assertEqual(hf_dataset.download(selection, self.cache, self.output), result)
        self.assertEqual(self.requests, [])

    def test_plugin_real_dispatch_and_package_include_new_module(self):
        options = ["--dataset", DATASET, "--manifest", "partitions.json", "--market", "market-a",
                   "--start-date", "2026-09-01", "--end-date", "2026-09-02"]
        with contextlib.redirect_stdout(io.StringIO()) as output:
            self.assertEqual(source_plugins.main(["download", "hf-dataset", *options,
                             "--cache-dir", str(self.cache), "--output", str(self.output)]), 0)
        self.assertEqual(json.loads(output.getvalue())["schema"], hf_dataset.SELECTION_SCHEMA)
        descriptor = next(p for p in source_plugins.plugin_descriptors() if p["id"] == "hf-dataset")
        self.assertEqual(descriptor["capabilities"], ["convert", "download", "plan", "prepare", "verify"])
        dockerfile = Path(__file__).parents[2] / "deploy/docker/Dockerfile"
        self.assertIn("runtimes/data/hf_dataset.py", dockerfile.read_text())


class RealHttpTransportTest(unittest.TestCase):
    """Real urllib sockets against a disposable loopback server, no public source download."""

    def test_real_redirect_interruption_resume_and_cached_reuse(self):
        body = b"PAR1" + b"fixture-source-bytes" * 5 + b"PAR1"
        index = json.dumps({"schema": hf_dataset.INDEX_SCHEMA, "files": [
            {"path": "data/a.parquet", "markets": ["market-a"], "start_date": "2026-09-01",
             "end_date": "2026-09-02", "format": "parquet"}]}).encode()
        requests = []
        metadata = {"sha": COMMIT, "private": False, "gated": False, "siblings": [
            {"rfilename": "data/a.parquet", "size": len(body)},
            {"rfilename": "partitions.json", "size": len(index)}]}

        class Handler(BaseHTTPRequestHandler):
            def log_message(self, *_args):
                pass

            def do_GET(self):
                if self.path.startswith("/api/datasets/"):
                    payload = json.dumps(metadata if "blobs=true" in self.path else {"sha": COMMIT}).encode()
                elif self.path.endswith("/partitions.json"):
                    payload = index
                elif self.path.endswith("/data/a.parquet"):
                    self.send_response(302)
                    self.send_header("Location", "/cdn/a.parquet")
                    self.end_headers()
                    return
                elif self.path == "/cdn/a.parquet":
                    byte_range = self.headers.get("Range")
                    requests.append(byte_range)
                    offset = int(byte_range.removeprefix("bytes=").removesuffix("-")) if byte_range else 0
                    self.send_response(206 if byte_range else 200)
                    self.send_header("Content-Length", str(len(body) - offset))
                    if byte_range:
                        self.send_header("Content-Range", f"bytes {offset}-{len(body)-1}/{len(body)}")
                    self.end_headers()
                    # First transfer disconnects with seven actual bytes persisted.
                    self.wfile.write(body[offset:] if byte_range else body[:7])
                    self.wfile.flush()
                    self.close_connection = True
                    return
                else:
                    self.send_error(404)
                    return
                self.send_response(200)
                self.send_header("Content-Length", str(len(payload)))
                self.end_headers()
                self.wfile.write(payload)

        server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        try:
            with tempfile.TemporaryDirectory() as directory, patch.object(snapshot, "HUB", f"http://127.0.0.1:{server.server_port}"):
                root = Path(directory)
                plan = hf_dataset.plan(DATASET, manifest="partitions.json", markets=["market-a"])
                with self.assertRaisesRegex(ValueError, "truncated"):
                    hf_dataset.download(plan, root / "cache", root / "request")
                result = hf_dataset.download(plan, root / "cache", root / "request")
                self.assertEqual(requests, [None, "bytes=7-"])
                self.assertEqual(Path(result["files"][0]["local_path"]).read_bytes(), body)
                cached = hf_dataset.download(plan, root / "cache", root / "overlap")
                self.assertEqual(cached["cached_files"], 1)
                self.assertEqual(len(requests), 2)
                self.assertEqual(hf_dataset.verify(root / "overlap/selection.json")["files"], 1)
        finally:
            server.shutdown()
            server.server_close()
            thread.join(timeout=5)



class HfArchiveMetadataTest(unittest.TestCase):
    """Replay existing 42-file evidence only; every network/data read is disabled."""

    def setUp(self):
        fixture_path = Path(__file__).with_name("fixtures") / "hf_archive_42_metadata.json"
        self.fixture = json.loads(fixture_path.read_text())
        self.index_body = json.dumps(self.fixture["index"]).encode()
        self.available = {f["path"]: {"size": f["size"]} for f in self.fixture["index"]["files"]}
        self.available["partitions.json"] = {"size": len(self.index_body)}
        self.metadata = patch.object(snapshot, "repository_metadata", return_value=(
            self.fixture["revision"], self.available, {"cardData": {"license": "cc-by-4.0"}}, "EVIDENCE_ONLY")).start()
        self.index_fetch = patch.object(snapshot, "fetch_bytes", side_effect=self.read_index).start()
        self.network = patch.object(snapshot.urllib.request, "urlopen", side_effect=AssertionError("no network allowed")).start()
        self.addCleanup(patch.stopall)

    def read_index(self, url, limit):
        self.assertEqual(url, snapshot.repository_file_url(
            self.fixture["repository"], self.fixture["revision"], "partitions.json"))
        self.assertEqual(limit, hf_dataset.MAX_INDEX_BYTES)
        return self.index_body

    def plan(self, **options):
        return hf_dataset.plan(self.fixture["repository"], revision=self.fixture["revision"],
                               manifest="partitions.json", **options)

    def test_exact_archive_inventory_can_be_date_selected_without_invented_markets(self):
        plan = self.plan(start_date="2022-11-01", end_date="2026-10-04", max_bytes=21470666882)
        self.assertEqual(len(plan["files"]), 42)
        self.assertEqual(plan["total_bytes"], self.fixture["selected_original_bytes"])
        self.assertEqual(len({f["path"] for f in plan["files"]}), 42)
        self.assertTrue(all(f["partition"]["markets"] == [] and
                            f["partition"]["market_mapping"] == "UNKNOWN" for f in plan["files"]))
        self.assertEqual(plan["coverage"], "NOT_ASSERTED")
        hf_dataset.validate_plan(plan)
        self.network.assert_not_called()

    def test_narrow_date_still_requires_entire_monthly_file_budget(self):
        options = {"start_date": "2025-12-15", "end_date": "2025-12-16"}
        with self.assertRaisesRegex(ValueError, "5132274521 bytes"):
            self.plan(max_bytes=5132274520, **options)
        plan = self.plan(max_bytes=5132274521, **options)
        self.assertEqual([(f["path"], f["size"]) for f in plan["files"]],
                         [("order_filled/year=2025/month=12.parquet", 5132274521)])
        self.assertEqual(plan["files"][0]["partition"]["start_date"], "2025-12-01")
        hf_dataset.validate_plan(plan)
        self.network.assert_not_called()

    def test_only_actual_archived_dates_or_explicit_files_are_selected(self):
        for market in ("ANY_REAL_MARKET", "ALL", "*", "UNKNOWN"):
            with self.subTest(market=market), self.assertRaisesRegex(ValueError, "known membership"):
                self.plan(markets=[market], start_date="2026-10-03", end_date="2026-10-04")
        with self.assertRaisesRegex(ValueError, "no indexed files"):
            self.plan(start_date="2026-09-26", end_date="2026-09-27")
        plan = self.plan(includes=["OrderFilled/2026_04_29.parquet"], max_bytes=304621469)
        self.assertEqual(plan["total_bytes"], 304621469)
        hf_dataset.validate_plan(plan)
        self.network.assert_not_called()


if __name__ == "__main__":
    unittest.main()
