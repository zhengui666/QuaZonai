#!/usr/bin/env python3
"""Run the built release harness, retain raw pairs and exact source identity."""
import argparse
import csv
import hashlib
import json
import os
from pathlib import Path
import platform
import resource
import statistics
import subprocess
import time
import tomllib

parser = argparse.ArgumentParser()
parser.add_argument("--binary", type=Path, required=True)
parser.add_argument("--rustc", default="rustc")
parser.add_argument("--build-identity", type=Path, required=True)
args = parser.parse_args()
directory = Path(__file__).resolve().parent
repo = directory.parents[4]
source = repo / "apps/job/src/signals.rs"
inputs = {
    "source_sha256": hashlib.sha256(source.read_bytes()).hexdigest(),
    "harness_sha256": hashlib.sha256((directory / "src/main.rs").read_bytes()).hexdigest(),
    "lock_sha256": hashlib.sha256((directory / "Cargo.lock").read_bytes()).hexdigest(),
}
build_identity = json.loads(args.build_identity.read_text())
assert inputs == build_identity, "benchmark source changed since the recorded build"
cpu = min(os.sched_getaffinity(0))
start = time.perf_counter()
with (directory / "results.csv").open("w") as stream:
    result = subprocess.run(["taskset", "-c", str(cpu), str(args.binary.resolve())],
                            stdout=stream, stderr=subprocess.PIPE, text=True)
usage = resource.getrusage(resource.RUSAGE_CHILDREN)
resources = {"exit_code": result.returncode, "wall_seconds": time.perf_counter() - start,
             "cpu_user_seconds": usage.ru_utime, "cpu_system_seconds": usage.ru_stime,
             "peak_rss_kib_whole_harness": usage.ru_maxrss, "stderr": result.stderr}
(directory / "resource.json").write_text(json.dumps(resources, indent=2) + "\n")
result.check_returncode()
rows = list(csv.DictReader((directory / "results.csv").open()))
summary = {}
for case in dict.fromkeys(row["case"] for row in rows):
    selected = [row for row in rows if row["case"] == case]
    fresh = [int(row["fresh_ns"]) / 1000 for row in selected]
    cached = [int(row["cached_ns"]) / 1000 for row in selected]
    summary[case] = {
        "pairs": len(selected),
        "fresh_median_us": statistics.median(fresh),
        "cached_median_us": statistics.median(cached),
        "ratio_of_medians": statistics.median(fresh) / statistics.median(cached),
        "median_paired_ratio": statistics.median(a / b for a, b in zip(fresh, cached)),
        "fresh_min_max_us": [min(fresh), max(fresh)],
        "cached_min_max_us": [min(cached), max(cached)],
        "module_requests_per_arm": int(selected[0]["module_calls"]),
        "observed_cached_reuse_per_arm": int(selected[0]["reused_modules"]),
        "fresh_compilations_per_arm": int(selected[0]["module_calls"]),
        "cached_compilations_per_arm": int(selected[0]["module_calls"]) - int(selected[0]["reused_modules"]),
        "fresh_instances_per_arm": int(selected[0]["models"]) * int(selected[0]["cutoffs"]) * int(selected[0]["assets"]),
        "predictions_per_arm": int(selected[0]["models"]) * int(selected[0]["cutoffs"]) * int(selected[0]["assets"]) * int(selected[0]["predictions_per_instance"]),
        "checksum": selected[0]["checksum"], "fuel": selected[0]["fuel"],
        "all_recorded_checksums_and_fuel_equal": len({(row["checksum"], row["fuel"]) for row in selected}) == 1,
    }
lock = tomllib.loads((directory / "Cargo.lock").read_text())
project = tomllib.loads((repo / "Cargo.lock").read_text())
project_versions = {(p["name"], p["version"]) for p in project["package"]}
extra = [(p["name"], p["version"]) for p in lock["package"]
         if p["name"] != "qz-signals-cache-bench" and (p["name"], p["version"]) not in project_versions]
metadata = {
    "base_commit": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=repo, text=True).strip(),
    **inputs,
    "binary_sha256": hashlib.sha256(args.binary.read_bytes()).hexdigest(),
    "runner_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
    "build_script_sha256": hashlib.sha256((directory / "build.sh").read_bytes()).hexdigest(),
    "source_has_uncommitted_changes": bool(subprocess.check_output(["git", "diff", "--", str(source)], cwd=repo)),
    "rustc": subprocess.check_output([args.rustc, "--version"], text=True).strip(),
    "kernel": platform.release(), "machine": platform.machine(), "affinity_cpu": cpu,
    "cpu_model": next((line.split(":", 1)[1].strip() for line in Path("/proc/cpuinfo").read_text().splitlines() if line.startswith("model name")), "unavailable"),
    "cgroup_cpu_max": Path("/sys/fs/cgroup/cpu.max").read_text().strip(),
    "profile": "standalone release, all dependencies optimized, production source included directly",
    "warmup_pairs_per_case": 5, "measured_pairs_per_case": 30,
    "untimed_validation": "every prediction bit and every remaining fuel value checked against uncached arm; each prediction also matches analytic reset-state model output",
    "dependency_versions_missing_from_project_lock": extra,
    "measurement_scope": "stateful synthetic Wasmi cache kernel; excludes catalog, covariance, simulation, orchestration and I/O",
}
assert not extra, extra
(directory / "metadata.json").write_text(json.dumps(metadata, indent=2) + "\n")
(directory / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
print(json.dumps(summary, indent=2))
