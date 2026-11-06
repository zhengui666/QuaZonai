#!/usr/bin/env python3
"""Controlled native ensemble requests; generated software fixtures, not market evidence."""
import argparse
import hashlib
import json
import math
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


def ensemble_fixture(groups=6000, assets=4, members=8, mode="regular"):
    end = (groups + 1) * 10
    meta = request_metadata(groups + 3, assets, train_end=end)
    reports = []
    for member in range(members):
        points = []
        for group in range(groups):
            for asset in range(assets):
                score = (math.sin(group * (member + 1) * 0.013)
                         + math.cos(group * (member + 3) * 0.007) + asset * 0.01)
                label = (math.sin(group * 0.013) * 0.001
                         + math.cos(group * 0.021) * 0.0003 + asset * 0.00001)
                if mode == "constant_last_member" and member == members - 1:
                    score = 1.0
                points.append(point(group, asset, score, label, "TRAIN"))
        reports.append({"factor_version_id": f"software-fixture-{member}",
                        "source_evidence_ref": f"controlled-generated-{member}",
                        "report": {"schema_version": 1, "request": meta,
                                   "native_versions": {}, "methods": {}, "consumed_fuel": None,
                                   "points": points, "partitions": []}})
    folds = [{"train_end_ns": str(groups * 3), "validation_start_ns": str(groups * 3),
              "validation_end_ns": str(groups * 6)},
             {"train_end_ns": str(groups * 6), "validation_start_ns": str(groups * 6),
              "validation_end_ns": str(groups * 9)}]
    return {"schema_version": 1, "fit_end_ns": str(end), "folds": folds, "members": reports}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, default=Path(__file__).parent / "fixtures")
    parser.add_argument("--bench", action="store_true")
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)
    cases = [("ensemble-small", ensemble_fixture(80, 3, 3)),
             ("ensemble-constant-last-member", ensemble_fixture(80, 3, 3, "constant_last_member"))]
    if args.bench:
        cases.append(("bench-ensemble", ensemble_fixture()))
        cases.append(("bench-ensemble-single-asset", ensemble_fixture(24000, 1, 8)))
    manifest = []
    for name, value in cases:
        payload = json.dumps(value, separators=(",", ":"), allow_nan=False).encode()
        (args.out / (name + ".json")).write_bytes(payload)
        manifest.append({"name": name, "operation": "fit-ensemble", "file": name + ".json",
                         "input_sha256": hashlib.sha256(payload).hexdigest(), "input_bytes": len(payload)})
    (args.out / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    print(json.dumps({"cases": len(cases), "output": str(args.out)}))


if __name__ == "__main__":
    main()
