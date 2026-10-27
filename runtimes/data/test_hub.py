import datetime
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from hub import acquire, relative_path, select_files


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

    def test_additive_profile_hint_preserves_existing_default_acquisition(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "fills.parquet").write_bytes(b"raw")
            selection = {"profile": {"repository": "fixture/source", "revision": "fixed", "streams": []},
                         "revision": "fixed", "files": [{"path": "fills.parquet", "size": 3}], "missing": []}
            previous = selection | {"retrieved_at": "2026-01-01T00:00:00Z"}
            manifest = root / "market-data.json"
            manifest.write_text(json.dumps(previous))
            original = manifest.read_bytes()
            updated = selection | {"profile": selection["profile"] | {"research_trade_path_prefixes": ["perps/", "spot/"]}}
            with patch("hub.urllib.request.urlopen", side_effect=AssertionError("existing files must not download")):
                self.assertEqual(acquire(updated, root), previous)
            self.assertEqual(manifest.read_bytes(), original)
            for changed in (updated | {"selected_path_prefixes": ["perps/"]},
                            updated | {"profile": updated["profile"] | {"streams": [{"name": "different"}]}}):
                with self.assertRaises(ValueError):
                    acquire(changed, root)
            self.assertEqual((root / "fills.parquet").read_bytes(), b"raw")


if __name__ == "__main__":
    unittest.main()
