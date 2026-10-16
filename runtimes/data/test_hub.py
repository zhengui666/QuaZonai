import datetime
import unittest

from hub import relative_path, select_files


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


if __name__ == "__main__":
    unittest.main()
