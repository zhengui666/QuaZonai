import datetime
import json
import io
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from hub import relative_path, select_files
from _source_test_fixtures import hub_acquisition


class PublicSelectionTests(unittest.TestCase):
    def test_daily_selection_discovers_every_symbol_and_reports_missing_paths(self):
        profile = {"streams": [{"name": "depth", "kind": "books",
                                "paths": ["depth/{date}/*.parquet"],
                                "date_format": "%Y%m%d"}]}
        files, missing = select_files(
            profile,
            [{"rfilename": "depth/20260101/arbitrary-pair.parquet", "size": 12},
             {"rfilename": "depth/20260101/new-listing.parquet", "size": 13},
             {"rfilename": "depth/20260103/later.parquet", "size": 14},
             {"rfilename": "README.md", "size": 5}],
            datetime.date(2026, 1, 1), datetime.date(2026, 1, 3), None)
        self.assertEqual([f["path"] for f in files], ["README.md",
                         "depth/20260101/arbitrary-pair.parquet",
                         "depth/20260101/new-listing.parquet"])
        self.assertEqual([m["date"] for m in missing], ["2026-01-02"])
        with self.assertRaises(ValueError):
            select_files(profile, [], datetime.date(2026, 1, 1),
                         datetime.date(2026, 1, 2), ["unknown"])

    def test_download_paths_are_relative(self):
        for path in ("../source", "/absolute", "a/../b", "a\\b", ""):
            with self.subTest(path=path), self.assertRaises(ValueError):
                relative_path(path)
        self.assertEqual(relative_path("date=2026-01-01/provider:market.parquet"),
                         "date=2026-01-01/provider:market.parquet")

    def test_explicit_trade_directories_do_not_require_or_download_outcomes(self):
        profile = {"streams": [{"name": "trades", "kind": "trades",
                                "paths": ["perps/{date}.parquet", "spot/{date}.parquet", "outcomes/{date}.parquet"],
                                "date_format": "%Y-%m-%d"}]}
        inventory = [{"rfilename": f"{kind}/2026-01-01.parquet", "size": 12}
                     for kind in ("perps", "spot", "outcomes")]
        start, end = datetime.date(2026, 1, 1), datetime.date(2026, 1, 3)
        files, missing = select_files(profile, inventory, start, end, ["trades"], ["perps/", "spot/"])
        self.assertEqual([f["path"] for f in files], ["perps/2026-01-01.parquet", "spot/2026-01-01.parquet"])
        self.assertEqual([m["pattern"] for m in missing], ["perps/2026-01-02.parquet", "spot/2026-01-02.parquet"])
        _, legacy_missing = select_files(profile, inventory, start, end, ["trades"])
        self.assertEqual(len(legacy_missing), 3, "omitted selector preserves the original all-trades scope")
        for invalid in (["../"], ["spot"], ["unknown/"]):
            with self.subTest(invalid=invalid), self.assertRaises(ValueError):
                select_files(profile, inventory, start, end, ["trades"], invalid)

    def test_frozen_profile_changes_require_distinct_publication(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            revision = "a" * 40
            selection = {"schema_version": 1, "repository": "fixture/source", "revision": revision,
                         "license": "FIXTURE", "total_bytes": 3, "profile": {"streams": []},
                         "files": [{"path": "fills.parquet", "size": 3,
                                    "url": f"https://huggingface.co/datasets/fixture/source/resolve/{revision}/fills.parquet"}]}
            output, cache = root / "output", root / "cache"
            with patch("hub.urllib.request.urlopen", return_value=io.BytesIO(b"raw")):
                previous = hub_acquisition(selection, cache, output)
            manifest = output / "market-data.json"
            original = manifest.read_bytes()
            updated = selection | {"profile": {"streams": [], "research_trade_path_prefixes": ["perps/", "spot/"]}}
            with patch("hub.urllib.request.urlopen", side_effect=AssertionError("completed units must not download")):
                self.assertEqual(hub_acquisition(selection, cache, output), previous)
                with self.assertRaisesRegex(ValueError, "another plan"):
                    hub_acquisition(updated, cache, output)
            self.assertEqual(manifest.read_bytes(), original)
            self.assertEqual(Path(previous["source_root"]).joinpath("fills.parquet").read_bytes(), b"raw")


if __name__ == "__main__":
    unittest.main()
