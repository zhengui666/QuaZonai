#!/usr/bin/env python3
"""Paired whole-process Study CLI / existing public Build API measurements.

Compile and export immutable fixtures first. Run only in a coordinated quiet
window. Every measured stdout must exactly equal the full reference document.
"""
import argparse
import csv
import hashlib
import json
import os
from pathlib import Path
import platform
import statistics
import subprocess
import time


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def file_hashes(root):
    return {str(p.relative_to(root)): digest(p) for p in sorted(root.rglob("*")) if p.is_file()}


def compilers():
    result = []
    for path in Path("/proc").glob("[0-9]*/comm"):
        try:
            name = path.read_text().strip()
        except (FileNotFoundError, ProcessLookupError, PermissionError):
            continue
        if name in {"cargo", "rustc"}:
            result.append({"pid": int(path.parent.name), "name": name})
    return result


def invoke(command, request, output, error, cpu):
    # Files are opened outside the timer; process startup, JSON request reading,
    # catalog/artifact I/O, all business work, stdout serialization and teardown
    # are included. Parsing/validation of the finished stdout is excluded.
    with request.open("rb") as stdin, output.open("wb") as stdout, error.open("wb") as stderr:
        start = time.perf_counter_ns()
        process = subprocess.Popen(
            command, stdin=stdin, stdout=stdout, stderr=stderr, env={},
            preexec_fn=lambda: os.sched_setaffinity(0, {cpu}),
        )
        _, status, usage = os.wait4(process.pid, 0)
        stop = time.perf_counter_ns()
        process.returncode = os.waitstatus_to_exitcode(status)
    if process.returncode != 0:
        raise RuntimeError(f"failed {command}: exit={process.returncode}; stderr={error}")
    return {
        "wall_ns": stop - start,
        "user_seconds": usage.ru_utime,
        "system_seconds": usage.ru_stime,
        "peak_rss_kib": usage.ru_maxrss,
        "output_sha256": digest(output),
        "stderr_sha256": digest(error),
    }, output.read_bytes()


def scientific_checks(kind, value, fixture, fuel_per_prediction):
    fuel = value["consumed_fuel"]
    assert fuel is not None and int(fuel) > 0, "explicit task fuel must be observed"
    case = json.loads((fixture / "case.json").read_text())
    request = json.loads((fixture / f"{kind}-request.json").read_text())
    assert all(m["parameters"]["slow_period"] == 3 for m in request["members"])
    prefix_bars = ([int(cutoff) // 60_000_000_000 for cutoff in case["cutoffs_ns"]]
                   if kind == "study" else [case["rows_per_asset"]])
    predictions = case["assets"] * case["models"] * sum(rows - 2 for rows in prefix_bars)
    assert int(fuel) == predictions * fuel_per_prediction
    if kind == "study":
        assert len(value["frames"]) == len(case["cutoffs_ns"])
        assert [f["cutoff_ns"] for f in value["frames"]] == case["cutoffs_ns"]
        assert all(f["allocation"]["targets"] is not None for f in value["frames"])
        assert all(f["allocation"]["solver_status"] == "OPTIMAL" for f in value["frames"])
        native = value["simulation"]
        assert native is not None and native["native_version"] == "0.67.0"
        assert int(native["orders"]) > 0, "fixture must enter actual native order flow"
        assert int(native["positions"]) > 0, "fixture must have actual native positions"
        assert int(native["consumed_target_points"]) == len(value["frames"])
        assert any(float(a["current_weight"]) > 0 for a in value["frames"][1]["input"]["assets"])
        return {
            "consumed_fuel": fuel,
            "derived_prediction_calls": predictions,
            "per_call_fuel_from_configuration_probe": fuel_per_prediction,
            "allocation_status": "OPTIMAL",
            "frames": len(value["frames"]),
            "native_orders": native["orders"],
            "native_positions": native["positions"],
            "native_consumed_target_points": native["consumed_target_points"],
            "native_summary": native["summary"],
        }
    assert value["allocation"]["targets"] is not None
    assert value["allocation"]["solver_status"] == "OPTIMAL"
    return {"consumed_fuel": fuel, "derived_prediction_calls": predictions,
            "per_call_fuel_from_configuration_probe": fuel_per_prediction,
            "allocation_status": "OPTIMAL", "native_replay": "not part of BuildPortfolio"}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--work-root", required=True, type=Path)
    parser.add_argument("--out", required=True, type=Path)
    parser.add_argument("--warmup-pairs", type=int, default=5)
    parser.add_argument("--pairs", type=int, default=30)
    parser.add_argument("--cpu", type=int)
    parser.add_argument("--validate-only", action="store_true")
    args = parser.parse_args()
    root, out = args.work_root.resolve(), args.out.resolve()
    out.mkdir(exist_ok=False)
    assert args.pairs >= 1 and args.warmup_pairs >= 0
    allowed = sorted(os.sched_getaffinity(0))
    cpu = args.cpu if args.cpu is not None else allowed[0]
    assert cpu in allowed
    before_compilers = compilers()
    if before_compilers and not args.validate_only:
        raise RuntimeError(f"compiler processes present: {before_compilers}")
    inputs = file_hashes(root / "fixtures")
    binaries = {name: digest(root / "bin" / name) for name in (
        "baseline-job", "candidate-job", "baseline-build", "candidate-build",
    )}
    identity = json.loads((root / "source-identity.json").read_text())
    build_identity = json.loads((root / "build-identity.json").read_text())
    assert binaries == build_identity["binaries_sha256"]
    assert binaries["candidate-job"] == identity["candidate_job_sha256"]
    assert binaries["baseline-job"] != binaries["candidate-job"]
    assert binaries["baseline-build"] != binaries["candidate-build"]
    cache_proofs = {}
    for arm, expected_reuse in (("baseline", 0), ("candidate", 16)):
        proof = json.loads((root / f"{arm}-cache-proof.json").read_text())
        assert proof["source_sha256"] == identity[f"{arm}_signal_sha256"]
        assert proof["cache_lookups"] == 32
        assert proof["reused_compiled_identity"] == expected_reuse
        assert proof["compiled_modules"] == 32 - expected_reuse
        assert proof["released_after_task"]
        cache_proofs[arm] = proof
    for key in ("module_source_sha256", "prediction_bits", "remaining_fuel"):
        assert cache_proofs["baseline"][key] == cache_proofs["candidate"][key]
    fuel_costs = {10000 - value for value in cache_proofs["candidate"]["remaining_fuel"]}
    assert len(fuel_costs) == 1
    fuel_per_prediction = next(iter(fuel_costs))
    models = {}
    for case in ("short", "long"):
        fixture = root / "fixtures" / case
        request = json.loads((fixture / "study-request.json").read_text())
        hashes = {
            member["model_artifact_id"]: digest(fixture / "objects" / member["model_artifact_id"])
            for member in request["members"]
        }
        distinct = len(set(hashes.values()))
        assert len(hashes) == 16 and distinct == 16
        assert set(hashes.values()) == set(cache_proofs["candidate"]["module_source_sha256"])
        models[case] = {"members": len(hashes), "distinct_source_hashes": distinct, "sha256": hashes}
    records, summaries = [], {}
    start_utc = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())
    for case in ("short", "long"):
        fixture = root / "fixtures" / case
        for kind in ("study", "build"):
            name = f"{case}_{kind}"
            reference = None
            proof = None
            samples = []
            warmups = 0 if args.validate_only else args.warmup_pairs
            pairs = 1 if args.validate_only else args.pairs
            for index in range(warmups + pairs):
                measured = index >= warmups
                order = ("baseline", "candidate") if index % 2 == 0 else ("candidate", "baseline")
                pair = {"case": name, "pair": index - warmups, "order": ",".join(order)}
                for arm in order:
                    binary = root / "bin" / f"{arm}-{'job' if kind == 'study' else 'build'}"
                    command = (
                        [str(binary), "study-portfolio", "--catalog", str(fixture / "catalog"), "--objects", str(fixture / "objects")]
                        if kind == "study" else
                        [str(binary), "build", str(fixture / "catalog"), str(fixture / "objects")]
                    )
                    output, error = out / f"{name}.{arm}.last.json", out / f"{name}.{arm}.last.stderr"
                    observation, raw = invoke(command, fixture / f"{kind}-request.json", output, error, cpu)
                    value = json.loads(raw)
                    actual_proof = scientific_checks(kind, value, fixture, fuel_per_prediction)
                    if reference is None:
                        reference, proof = raw, actual_proof
                        (out / f"{name}.full-output.json").write_bytes(raw)
                    if raw != reference or actual_proof != proof:
                        raise RuntimeError(f"full stdout or fuel/financial evidence differs: {name}/{index}/{arm}; retained last outputs")
                    pair.update({f"{arm}_{key}": value for key, value in observation.items()})
                if measured:
                    samples.append(pair)
                    records.append(pair)
                    # Durable raw progress, even if a later case is rejected.
                    with (out / "observations.jsonl").open("a") as stream:
                        stream.write(json.dumps(pair) + "\n")
            summary = {"correctness": proof, "full_stdout_sha256": hashlib.sha256(reference).hexdigest()}
            if not args.validate_only:
                for arm in ("baseline", "candidate"):
                    wall = [p[f"{arm}_wall_ns"] / 1e6 for p in samples]
                    rss = [p[f"{arm}_peak_rss_kib"] for p in samples]
                    summary[arm] = {
                        "median_wall_ms": statistics.median(wall),
                        "min_wall_ms": min(wall), "max_wall_ms": max(wall),
                        "median_peak_rss_kib": statistics.median(rss),
                        "min_peak_rss_kib": min(rss), "max_peak_rss_kib": max(rss),
                    }
                summary["ratio_of_wall_medians"] = summary["baseline"]["median_wall_ms"] / summary["candidate"]["median_wall_ms"]
                summary["median_paired_wall_ratio"] = statistics.median(p["baseline_wall_ns"] / p["candidate_wall_ns"] for p in samples)
            summaries[name] = summary
            print(json.dumps({name: summary}), flush=True)
    if inputs != file_hashes(root / "fixtures"):
        raise RuntimeError("frozen input files changed")
    if binaries != {name: digest(root / "bin" / name) for name in binaries}:
        raise RuntimeError("measured binaries changed")
    after_compilers = compilers()
    metadata = {
        "start_utc": start_utc,
        "end_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "mode": "validation only; timing not evidence" if args.validate_only else "paired whole-process benchmark",
        "cpu_affinity": [cpu], "original_allowed_cpus": allowed,
        "cpu_model": next((line.split(":", 1)[1].strip() for line in Path("/proc/cpuinfo").read_text().splitlines() if line.startswith("model name")), "unknown"),
        "cpu_max": Path("/sys/fs/cgroup/cpu.max").read_text().strip(),
        "uname": list(platform.uname()),
        "warmup_pairs": 0 if args.validate_only else args.warmup_pairs,
        "measured_pairs_per_case": 0 if args.validate_only else args.pairs,
        "full_stdout_bytes_equal": True, "no_fields_removed": True,
        "binaries_sha256": binaries, "models": models,
        "source_identity": identity, "build_identity": build_identity,
        "cache_configuration_proofs": cache_proofs,
        "runner_sha256": digest(Path(__file__)),
        "compilers_before": before_compilers, "compilers_after": after_compilers,
        "rss_scope": "per child process peak resident memory from Linux wait4; not compiled-IR heap",
        "limitations": "synthetic two-asset fixed workloads, warm filesystem cache, one allowed CPU; excludes fixture export/compilation and HTTP/service orchestration; no production p95 claim; compile counts are structural expectations, not measured E2E instrumentation",
    }
    (out / "input-sha256.json").write_text(json.dumps(inputs, indent=2) + "\n")
    (out / "metadata.json").write_text(json.dumps(metadata, indent=2) + "\n")
    (out / "summary.json").write_text(json.dumps(summaries, indent=2) + "\n")
    with (out / "results.csv").open("w", newline="") as stream:
        writer = csv.DictWriter(stream, fieldnames=list(records[0]))
        writer.writeheader()
        writer.writerows(records)
    if after_compilers and not args.validate_only:
        raise RuntimeError("compiler present after run; reject this measurement window")


if __name__ == "__main__":
    main()
