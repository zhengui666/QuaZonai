"""Offline partition selection, cache, interruption and transport regressions."""

from concurrent.futures import ThreadPoolExecutor
import contextlib
from copy import deepcopy
import csv
import io
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import os
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

    def test_retry_revalidates_preserved_request_before_acquisition_or_publication(self):
        selection = self.plan()
        self.requests.clear()
        for index, requested_revision in enumerate((None, "", " ", 1)):
            with self.subTest(requested_revision=requested_revision):
                output = self.root / f"invalid-request-{index}"
                output.mkdir()
                original = {**selection, "requested_revision": requested_revision}
                request_path = output / "request.json"
                before = hf_dataset.manifest_bytes(original)
                request_path.write_bytes(before)
                self.assertTrue(hf_dataset.same_request(original, selection))
                with patch.object(snapshot, "acquire_cached_file", wraps=snapshot.acquire_cached_file) as acquire, \
                        patch.object(snapshot, "publish_bytes", wraps=snapshot.publish_bytes) as publish:
                    with self.assertRaises(ValueError):
                        hf_dataset.download(selection, self.cache, output)
                    acquire.assert_not_called()
                    publish.assert_not_called()
                self.assertEqual(request_path.read_bytes(), before)
                self.assertFalse((output / "selection.json").exists())
        self.assertEqual(self.requests, [])

    def test_pending_request_reuses_completed_cache_offline_and_keeps_original_revision_alias(self):
        selection = self.plan(revision="original-tag")
        hf_dataset.download(selection, self.cache, self.output)
        pending = self.root / "pending"
        pending.mkdir()
        original = hf_dataset.manifest_bytes(selection)
        (pending / "request.json").write_bytes(original)
        pinned = {**selection, "requested_revision": COMMIT}
        self.requests.clear()
        self.http.side_effect = AssertionError("fixed-plan retry must not access the Hub")
        with patch.object(snapshot, "repository_metadata", side_effect=AssertionError("no ref resolution")), \
                patch.object(snapshot, "file_hash", side_effect=AssertionError("no checksum")):
            result = hf_dataset.download(pinned, self.cache, pending)
            self.assertEqual(hf_dataset.verify(pending / "selection.json")["files"], 1)
        self.assertEqual(result["plan"], selection)
        self.assertEqual((pending / "request.json").read_bytes(), original)
        self.assertEqual((result["downloaded_bytes"], result["cached_files"]), (0, 1))
        self.assertEqual(self.requests, [])

    def test_different_requests_cannot_reuse_pending_or_published_output(self):
        selection = self.plan()
        different_file = hf_dataset.plan(DATASET, includes=["data/c.parquet"])
        candidates = [different_file]
        for change in (lambda p: p.update(license="other-terms"),
                       lambda p: p["request"].update(end_date="2026-09-03"),
                       lambda p: p["files"][0]["partition"].update(markets=["market-a", "market-c"])):
            candidate = deepcopy(selection)
            change(candidate)
            candidates.append(candidate)
        revision = deepcopy(selection)
        revision["revision"] = "b" * 40
        for record in [revision["partition_index"], *revision["files"]]:
            record["url"] = record["url"].replace(COMMIT, revision["revision"])
        candidates.append(revision)
        hf_dataset.download(selection, self.cache, self.output)
        pending = self.root / "pending"
        pending.mkdir()
        (pending / "request.json").write_bytes(hf_dataset.manifest_bytes(selection))
        (pending / ".hf-request.lock").touch()
        self.requests.clear()
        self.http.side_effect = AssertionError("conflicting request must not access the Hub")
        for output in (pending, self.output):
            before = {p.name: p.read_bytes() for p in output.iterdir()}
            for index, candidate in enumerate(candidates):
                with self.subTest(output=output.name, candidate=index):
                    hf_dataset.validate_plan(candidate)
                    self.assertFalse(hf_dataset.same_request(selection, candidate))
                    with patch.object(snapshot, "acquire_cached_file") as acquire, \
                            patch.object(snapshot, "publish_bytes") as publish:
                        with self.assertRaisesRegex(ValueError, "different request"):
                            hf_dataset.download(candidate, self.cache, output)
                        acquire.assert_not_called()
                        publish.assert_not_called()
                    self.assertEqual({p.name: p.read_bytes() for p in output.iterdir()}, before)
        self.assertEqual(self.requests, [])

    def test_completed_retry_rejects_selection_conflicting_with_preserved_request(self):
        selection = self.plan()
        result = hf_dataset.download(selection, self.cache, self.output)
        request_path, manifest_path = self.output / "request.json", self.output / "selection.json"
        original_request = request_path.read_bytes()
        changed = deepcopy(selection)
        changed["license"] = "other-terms"
        altered = {**result, "plan": changed}
        hf_dataset.validate_manifest(altered, check_files=True)
        manifest_path.write_bytes(hf_dataset.manifest_bytes(altered))
        before = manifest_path.read_bytes()
        self.requests.clear()
        self.http.side_effect = AssertionError("conflicting request must not access the Hub")
        with patch.object(snapshot, "acquire_cached_file") as acquire, \
                patch.object(snapshot, "publish_bytes") as publish:
            with self.assertRaisesRegex(ValueError, "different request"):
                hf_dataset.download(changed, self.cache, self.output)
            acquire.assert_not_called()
            publish.assert_not_called()
        self.assertEqual(request_path.read_bytes(), original_request)
        self.assertEqual(manifest_path.read_bytes(), before)
        self.assertEqual(self.requests, [])

    def test_completed_retry_keeps_legacy_manifest_only_output_and_revision_alias(self):
        selection = self.plan(revision="original-tag")
        result = hf_dataset.download(selection, self.cache, self.output)
        request_path, manifest_path = self.output / "request.json", self.output / "selection.json"
        request_path.unlink()
        before = manifest_path.read_bytes()
        pinned = {**selection, "requested_revision": COMMIT}
        self.requests.clear()
        self.http.side_effect = AssertionError("completed retry must not access the Hub")
        with patch.object(snapshot, "repository_metadata", side_effect=AssertionError("no ref resolution")), \
                patch.object(snapshot, "acquire_cached_file") as acquire, \
                patch.object(snapshot, "publish_bytes") as publish:
            self.assertEqual(hf_dataset.download(pinned, self.cache, self.output), result)
            acquire.assert_not_called()
            publish.assert_not_called()
        self.assertFalse(request_path.exists())
        self.assertEqual(manifest_path.read_bytes(), before)
        self.assertEqual(self.requests, [])

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
        self.assertEqual(descriptor["capabilities"], ["convert", "download", "freeze", "plan", "prepare", "verify"])
        dockerfile = Path(__file__).parents[2] / "deploy/docker/Dockerfile"
        self.assertIn("runtimes/data/hf_dataset.py", dockerfile.read_text())


class HfOfflineFreezeTest(unittest.TestCase):
    """Existing original fixtures only; acquisition, network and hashing fail closed."""

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.cache = self.root / "cache"
        self.output = self.root / "new-request"
        self.plan_path = self.root / "original-plan.json"
        self.body = b"PAR1existing-offline-fixturePAR1"
        files = [{"path": f"data/{name}.parquet", "size": len(self.body), "format": "parquet",
                  "url": snapshot.repository_file_url(DATASET, COMMIT, f"data/{name}.parquet")}
                 for name in "ab"]
        self.plan = {"schema": hf_dataset.PLAN_SCHEMA, "repository": DATASET, "revision": COMMIT,
                     "requested_revision": "original-tag", "license": None, "partition_index": None,
                     "request": {"includes": [f["path"] for f in files], "markets": [],
                                 "start_date": None, "end_date": None},
                     "selection_bounds": "[start_date,end_date)", "download_granularity": "FILE_PARTITION",
                     "coverage": "NOT_ASSERTED", "max_bytes": None,
                     "total_bytes": sum(f["size"] for f in files), "files": files}
        self.plan_path.write_bytes(hf_dataset.manifest_bytes(self.plan))
        self.cache_root = hf_dataset.cache_root(self.cache, self.plan)
        self.originals, self.targets = [], []
        for item in files:
            original = self.root / Path(item["path"]).name
            original.write_bytes(self.body)
            target = self.cache_root / "files" / item["path"]
            target.parent.mkdir(parents=True, exist_ok=True)
            os.link(original, target)
            self.originals.append(original)
            self.targets.append(target)
        self.forbidden = []
        for owner, method in ((snapshot, "repository_metadata"), (snapshot, "acquire_cached_file"),
                              (snapshot, "fetch_bytes"), (snapshot, "file_hash"),
                              (snapshot.urllib.request, "urlopen"), (snapshot.hashlib, "sha256")):
            mock = patch.object(owner, method, side_effect=AssertionError(f"freeze must not call {method}"))
            self.forbidden.append(mock.start())
            self.addCleanup(mock.stop)

    def freeze(self, output=None):
        return hf_dataset.freeze(self.plan_path, self.cache, output or self.output)

    def test_real_cli_freezes_existing_hard_links_without_network_hash_or_data_copy(self):
        before = [hf_dataset.freeze_observation(p) for p in self.originals + self.targets]
        with contextlib.redirect_stdout(io.StringIO()) as output:
            self.assertEqual(source_plugins.main(["freeze", "hf-dataset", "--plan", str(self.plan_path),
                             "--cache-dir", str(self.cache), "--output", str(self.output)]), 0)
        result = json.loads(output.getvalue())
        self.assertEqual(result["plan"], self.plan)
        self.assertEqual(result["downloaded_bytes"], 0)
        self.assertEqual(result["cached_files"], 2)
        self.assertEqual(result["cache_root"], str(self.cache_root))
        self.assertEqual(json.loads((self.output / "request.json").read_bytes()), self.plan)
        self.assertEqual(json.loads((self.output / "selection.json").read_bytes()), result)
        self.assertEqual(hf_dataset.verify(self.output / "selection.json")["files"], 2)
        for item, original, target in zip(result["files"], self.originals, self.targets):
            self.assertEqual(item["local_path"], str(target))
            self.assertEqual((item["cached"], item["resumed_bytes"], item["validation"]),
                             (True, 0, "PARQUET_ENVELOPE"))
            self.assertTrue(original.samefile(target))
            self.assertEqual(original.stat().st_nlink, 2)
        self.assertEqual(before, [hf_dataset.freeze_observation(p) for p in self.originals + self.targets])
        self.assertEqual(set(p.name for p in self.output.iterdir()),
                         {"request.json", "selection.json", ".hf-request.lock"})
        self.assertEqual(sorted(p for p in self.cache_root.rglob("*.parquet") if p.is_file()), self.targets)
        for mock in self.forbidden:
            mock.assert_not_called()
        descriptor = next(p for p in source_plugins.plugin_descriptors() if p["id"] == "hf-dataset")
        self.assertIn("freeze", descriptor["capabilities"])
        self.assertEqual(descriptor["public_network_operations"], ["download", "plan"])
        self.assertFalse(source_plugins.PLUGINS["hf-dataset"].capabilities["freeze"].public_network)

    def test_missing_complete_file_never_adopts_partial_or_publishes_manifest(self):
        self.targets[-1].unlink()
        partial = self.cache_root / "transfers/data/b.parquet/data.partial"
        partial.parent.mkdir(parents=True)
        partial.write_bytes(self.body)
        with self.assertRaisesRegex(ValueError, "input is missing.*b.parquet"):
            self.freeze()
        self.assertFalse((self.output / "selection.json").exists())
        self.assertFalse((self.output / "request.json").exists())
        self.assertEqual(partial.read_bytes(), self.body)
        self.assertFalse(self.targets[-1].exists())

    def test_bad_size_parquet_header_footer_and_nonregular_source_fail_without_publication(self):
        for name, body in (("size", b"short"), ("header", b"FAIL" + self.body[4:]),
                           ("footer", self.body[:-4] + b"FAIL")):
            self.targets[0].write_bytes(body)
            output = self.root / name
            with self.subTest(name=name), self.assertRaises(ValueError):
                self.freeze(output)
            self.assertFalse((output / "selection.json").exists())
            self.assertEqual(self.targets[0].read_bytes(), body)
        self.targets[0].unlink()
        self.targets[0].mkdir()
        with self.assertRaisesRegex(ValueError, "regular file"):
            self.freeze()
        self.assertFalse((self.output / "selection.json").exists())

    def test_plan_file_cache_file_and_cache_ancestor_symlinks_fail(self):
        linked_plan = self.root / "linked-plan.json"
        linked_plan.symlink_to(self.plan_path)
        with self.assertRaisesRegex(ValueError, "symlinks"):
            hf_dataset.freeze(linked_plan, self.cache, self.output)
        self.targets[0].unlink()
        self.targets[0].symlink_to(self.originals[0])
        with self.assertRaisesRegex(ValueError, "symlinks"):
            self.freeze()
        self.targets[0].unlink()
        os.link(self.originals[0], self.targets[0])
        alias = self.root / "cache-alias"
        alias.symlink_to(self.cache, target_is_directory=True)
        with self.assertRaisesRegex(ValueError, "symlinks"):
            hf_dataset.freeze(self.plan_path, alias, self.root / "ancestor-request")
        self.assertEqual(self.originals[0].read_bytes(), self.body)

    def test_existing_cache_state_must_match_original_source_identity(self):
        state = self.cache_root / "transfers/data/a.parquet/state.json"
        state.parent.mkdir(parents=True)
        identity = {key: self.plan["files"][0][key] for key in ("url", "size", "format")}
        for key, value in (("url", "https://example.invalid/other"), ("size", 1), ("format", "opaque")):
            state.write_text(json.dumps({**identity, key: value}))
            output = self.root / key
            with self.subTest(key=key), self.assertRaisesRegex(ValueError, "different file metadata"):
                self.freeze(output)
            self.assertFalse((output / "selection.json").exists())
            self.assertEqual(json.loads(state.read_bytes()), {**identity, key: value})
        state.write_text(json.dumps(identity))
        self.assertEqual(self.freeze()["cached_files"], 2)

    def test_plan_source_conflict_or_arbitrary_local_path_is_not_accepted(self):
        for change in ("url", "revision", "local_path"):
            plan = json.loads(json.dumps(self.plan))
            if change == "url":
                plan["files"][0]["url"] = "https://example.invalid/other"
            elif change == "revision":
                plan["revision"] = "main"
            else:
                plan["files"][0]["local_path"] = str(self.originals[0])
            self.plan_path.write_bytes(hf_dataset.manifest_bytes(plan))
            output = self.root / change
            with self.subTest(change=change), self.assertRaises(ValueError):
                self.freeze(output)
            self.assertFalse((output / "selection.json").exists())

    def test_input_mutation_during_or_after_inspection_is_not_published(self):
        inspect = hf_dataset.inspect_file
        for name in ("source", "plan", "earlier-file"):
            self.targets[0].write_bytes(self.body)
            self.plan_path.write_bytes(hf_dataset.manifest_bytes(self.plan))
            def mutate(path, item):
                result = inspect(path, item)
                if name == "plan":
                    self.plan_path.write_bytes(self.plan_path.read_bytes() + b" ")
                elif path == self.targets[0] and name == "source":
                    path.write_bytes(self.body.replace(b"existing", b"modified"))
                elif path == self.targets[1] and name == "earlier-file":
                    self.targets[0].write_bytes(self.body.replace(b"existing", b"modified"))
                return result
            output = self.root / name
            with self.subTest(name=name), patch.object(hf_dataset, "inspect_file", side_effect=mutate), \
                    self.assertRaisesRegex(ValueError, "input changed"):
                self.freeze(output)
            self.assertFalse((output / "selection.json").exists())
            self.assertFalse((output / "request.json").exists())

    def test_mutation_between_request_and_selection_publication_keeps_selection_absent(self):
        publish = snapshot.publish_bytes
        def mutate(path, body):
            publish(path, body)
            if path.name == "request.json":
                self.targets[0].write_bytes(self.body.replace(b"existing", b"modified"))
        with patch.object(snapshot, "publish_bytes", side_effect=mutate), \
                self.assertRaisesRegex(ValueError, "input changed"):
            self.freeze()
        self.assertFalse((self.output / "selection.json").exists())
        self.assertEqual(json.loads((self.output / "request.json").read_bytes()), self.plan)

    def test_existing_output_even_empty_or_prior_success_fails_without_overwrite(self):
        self.output.mkdir()
        with self.assertRaisesRegex(ValueError, "must be a new directory"):
            self.freeze()
        self.assertEqual(list(self.output.iterdir()), [])
        other = self.root / "success"
        self.freeze(other)
        original = (other / "selection.json").read_bytes()
        with self.assertRaisesRegex(ValueError, "must be a new directory"):
            self.freeze(other)
        self.assertEqual((other / "selection.json").read_bytes(), original)

    def assert_writer_waits_for_publication(self, boundary):
        attempted, acquired, allow_write = threading.Event(), threading.Event(), threading.Event()
        observations, errors = [], []
        lock = self.cache_root / "transfers/data/a.parquet/lock"
        def writer():
            try:
                attempted.set()
                with snapshot.cache_lock(lock):
                    observations.append((self.output / "selection.json").exists())
                    acquired.set()
                    if allow_write.wait(2):
                        self.targets[0].write_bytes(b"FAIL" + self.body[4:])
            except Exception as error:
                errors.append(error)
        thread = threading.Thread(target=writer, daemon=True)
        started = False
        def start_writer():
            nonlocal started
            if not started:
                started = True
                thread.start()
                self.assertTrue(attempted.wait(2))
                self.assertFalse(acquired.wait(0.1), "selected cache lock released before publication")
        original_observe, original_publish = hf_dataset.freeze_observation, snapshot.publish_bytes
        def observe(path):
            result = original_observe(path)
            if (boundary == "last-stat" and path == self.targets[1]
                    and (self.output / "request.json").exists()):
                # a was already checked in this last pass; a cooperating
                # writer must still be blocked while b's stat returns.
                start_writer()
            return result
        def publish(path, body):
            if boundary == "publish" and path.name == "selection.json":
                # All final stats have returned, but publication has not run.
                start_writer()
            return original_publish(path, body)
        try:
            with patch.object(hf_dataset, "freeze_observation", side_effect=observe), \
                    patch.object(snapshot, "publish_bytes", side_effect=publish):
                self.assertEqual(self.freeze()["cached_files"], 2)
            self.assertTrue(started)
            self.assertTrue(acquired.wait(2), "selected cache lock was not released after publication")
            self.assertEqual(observations, [True])
            self.assertEqual(hf_dataset.verify(self.output / "selection.json")["files"], 2)
        finally:
            allow_write.set()
            if started:
                thread.join(timeout=2)
        self.assertFalse(thread.is_alive())
        self.assertEqual(errors, [])
        # Publication is not a promise of permanent source immutability.
        self.assertEqual(self.targets[0].read_bytes(), b"FAIL" + self.body[4:])

    def test_earlier_file_lock_is_held_through_later_files_final_stat(self):
        self.assert_writer_waits_for_publication("last-stat")

    def test_all_file_locks_are_held_between_final_stat_and_atomic_publication(self):
        self.assert_writer_waits_for_publication("publish")

    def test_all_file_locks_release_on_inspection_or_publication_failure(self):
        original_inspect, original_publish = hf_dataset.inspect_file, snapshot.publish_bytes
        for failure in ("inspect", "publish"):
            output = self.root / failure
            def inspect(path, item):
                if failure == "inspect" and path == self.targets[-1]:
                    raise ValueError("injected inspection failure")
                return original_inspect(path, item)
            def publish(path, body):
                if failure == "publish" and path.name == "selection.json":
                    raise OSError("injected publication failure")
                return original_publish(path, body)
            with self.subTest(failure=failure), patch.object(hf_dataset, "inspect_file", side_effect=inspect), \
                    patch.object(snapshot, "publish_bytes", side_effect=publish), self.assertRaises((ValueError, OSError)):
                self.freeze(output)
            self.assertFalse((output / "selection.json").exists())
            released = []
            def check_release():
                for item in self.plan["files"]:
                    lock = self.cache_root / "transfers" / item["path"] / "lock"
                    with snapshot.cache_lock(lock):
                        released.append(item["path"])
            thread = threading.Thread(target=check_release, daemon=True)
            thread.start()
            thread.join(timeout=2)
            self.assertFalse(thread.is_alive(), "freeze leaked a selected cache lock")
            self.assertEqual(released, [item["path"] for item in self.plan["files"]])

    def test_overlapping_reversed_plans_acquire_locks_in_same_order_without_deadlock(self):
        reverse_plan = {**self.plan, "files": list(reversed(self.plan["files"]))}
        reverse_path = self.root / "reverse-plan.json"
        reverse_path.write_bytes(hf_dataset.manifest_bytes(reverse_plan))
        lock_orders, results, errors = {}, [], []
        guard, start = threading.Lock(), threading.Barrier(2)
        original_lock = snapshot.cache_lock
        @contextlib.contextmanager
        def lock(path):
            if path.name == "lock":
                with guard:
                    lock_orders.setdefault(threading.current_thread().name, []).append(str(path))
            with original_lock(path):
                yield
        def run(path, name):
            try:
                start.wait(timeout=2)
                results.append(hf_dataset.freeze(path, self.cache, self.root / name))
            except Exception as error:
                errors.append(error)
        threads = [threading.Thread(target=run, args=(path, f"concurrent-{index}"),
                                    name=f"freeze-{index}", daemon=True)
                   for index, path in enumerate((self.plan_path, reverse_path))]
        with patch.object(snapshot, "cache_lock", side_effect=lock):
            for thread in threads:
                thread.start()
            for thread in threads:
                thread.join(timeout=3)
        self.assertFalse(any(thread.is_alive() for thread in threads), "inconsistent lock order deadlocked")
        self.assertEqual(errors, [])
        self.assertEqual(len(results), 2)
        self.assertTrue(all(result["cached_files"] == 2 for result in results))
        self.assertEqual(lock_orders["freeze-0"], lock_orders["freeze-1"])
        self.assertEqual(lock_orders["freeze-0"], sorted(lock_orders["freeze-0"]))


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
