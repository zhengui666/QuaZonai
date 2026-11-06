#!/usr/bin/env python3
"""Controlled software fixtures for original QuaZonai native CLI contracts.

These are generated observations, not archived market evidence or profitability
claims. All requests use the existing public JSON contracts unchanged.
"""
import argparse
import copy
import hashlib
import json
from pathlib import Path


def request_metadata(groups, assets, train_end=1):
    end = (groups + 5) * 10
    return {
        "schema_version": 1,
        "forecast": {
            "schema_version": 1,
            "selection": {
                "schema_version": 1,
                "bar_types": [f"ASSET{i}.SIM-1-MINUTE-LAST-EXTERNAL" for i in range(assets)],
                "event_start_ns": "0", "event_end_ns": str(end),
                "decision_cutoff_ns": str(end + 10), "maximum_rows": 0,
            },
            "parameters": {"schema_version": 1, "fast_period": 2,
                           "slow_period": 3, "label_horizon_observations": 1},
        },
        "train_end_ns": str(train_end), "validation_end_ns": str(end - 1),
    }


def point(group, asset, score, label, partition="VALIDATION", overlap=1):
    event = (group + 1) * 10
    return {
        "observation": {
            "instrument_id": f"ASSET{asset}.SIM", "ordinal": group,
            "event_ns": str(event), "available_ns": str(event),
            "forecast": score, "forecast_reason": None,
            "label_return": label, "label_available_ns": str(event + overlap * 10),
            "label_reason": None,
        },
        "partition": partition, "label_event_ns": str(event + overlap * 10),
        "exclusion_reason": None,
    }


def review_fixture(groups=96, assets=12, scope="POOLED_TIME_SERIES", mode="regular", block=1):
    points = []
    for group in range(groups):
        count = assets if mode != "irregular" else 3 + (group * 7) % max(assets - 2, 1)
        if mode == "skewed":
            count = assets if group == groups // 2 else 3
        for asset in range(count):
            score = ((group * 7 + asset * 11) % 23 - 11) / 8
            label = ((group * 13 + asset * 5) % 29 - 14) / 1024
            if mode == "constant":
                score = 1.0
            if mode == "ties_signed_zero":
                score = [-0.0, 0.0, 1.0, 1.0, -2.0][(group + asset) % 5]
            p = point(group, asset, score, label, overlap=3 if mode == "overlap" else 1)
            if mode == "excluded" and (group + asset) % 7 == 0:
                p["observation"]["forecast"] = None
                p["observation"]["forecast_reason"] = "INDICATOR_WARMUP"
                p["exclusion_reason"] = "INDICATOR_WARMUP"
            if mode == "partitioned" and group % 3 == 0:
                p["partition"] = "HELDOUT"
            points.append(p)
    if mode == "duplicate":
        points.insert(2, copy.deepcopy(points[0]))
    if mode == "missing":
        points[0]["observation"]["forecast"] = None
    report = {"schema_version": 1, "request": request_metadata(groups, assets),
              "native_versions": {}, "methods": {}, "consumed_fuel": None,
              "points": points, "partitions": []}
    digest = hashlib.sha256(json.dumps(report, sort_keys=True).encode()).hexdigest()
    return {"schema_version": 1, "source_digest": "sha256:" + digest,
            "analysis": {"kind": "FACTOR", "evidence": report,
                         "partition": "VALIDATION", "scope": scope, "block_length": block}}


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--out", type=Path, default=Path(__file__).parent / "fixtures")
    p.add_argument("--bench", action="store_true")
    args = p.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)
    cases = []
    modes = ["regular", "irregular", "ties_signed_zero", "constant", "excluded", "partitioned", "overlap", "duplicate", "missing"]
    for scope in ["POOLED_TIME_SERIES", "CROSS_SECTIONAL"]:
        for mode in modes:
            cases.append((f"review-{scope.lower()}-{mode}", "review-science",
                          review_fixture(scope=scope, mode=mode)))
        for n in [0, 8, 59, 60]:
            cases.append((f"review-{scope.lower()}-groups-{n}", "review-science",
                          review_fixture(groups=n, scope=scope)))
    if args.bench:
        for scope in ["POOLED_TIME_SERIES", "CROSS_SECTIONAL"]:
            cases.append((f"bench-review-{scope.lower()}", "review-science",
                          review_fixture(240, 40, scope, "irregular")))
        cases.append(("bench-review-skewed-pooled", "review-science",
                      review_fixture(96, 6000, "POOLED_TIME_SERIES", "skewed")))
    manifest = []
    for name, operation, value in cases:
        payload = json.dumps(value, separators=(",", ":"), allow_nan=False).encode()
        (args.out / (name + ".json")).write_bytes(payload)
        manifest.append({"name": name, "operation": operation, "file": name + ".json",
                         "input_sha256": hashlib.sha256(payload).hexdigest(), "input_bytes": len(payload)})
    (args.out / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    print(json.dumps({"cases": len(cases), "output": str(args.out)}))


if __name__ == "__main__":
    main()
