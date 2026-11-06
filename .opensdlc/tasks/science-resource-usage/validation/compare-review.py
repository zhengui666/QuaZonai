#!/usr/bin/env python3
"""Run actual native CLI binaries against identical controlled JSON requests.

Records process wall time, total CPU and peak resident memory using wait4.
Alternates binary order on each repetition. It never treats failed runs or
different complete results as a performance success.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import statistics
import subprocess
import time


def digest(path):
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--binary", action="append", required=True, help="LABEL=/absolute/path/to/job")
    p.add_argument("--fixtures", type=Path, default=Path(__file__).parent / "fixtures")
    p.add_argument("--out", type=Path, required=True)
    p.add_argument("--case", action="append", default=[])
    p.add_argument("--repeats", type=int, default=1)
    p.add_argument("--timeout", type=float, default=0,
                   help="seconds, 0 uses blocking wait4 for precise benchmark wall time")
    args = p.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)
    binaries = []
    for spec in args.binary:
        label, value = spec.split("=", 1)
        path = value
        env = {"PATH": os.defpath, "RUST_BACKTRACE": "0"}
        binary = Path(path).resolve(strict=True)
        binaries.append({"label": label, "path": str(binary), "sha256": digest(binary), "env": env})
    manifest = json.loads((args.fixtures / "manifest.json").read_text())
    manifest = [case for case in manifest if not args.case or any(s in case["name"] for s in args.case)]
    records = []
    equivalent = True
    expected = {}
    for rep in range(args.repeats):
        for case in manifest:
            path = args.fixtures / case["file"]
            if digest(path) != case["input_sha256"]:
                raise ValueError(f"fixture changed: {path}")
            for binary in (binaries if rep % 2 == 0 else list(reversed(binaries))):
                prefix = args.out / f'{case["name"]}-{binary["label"]}-{rep}'
                metrics = prefix.with_suffix(".time.json")
                stdout = prefix.with_suffix(".stdout.json")
                stderr = prefix.with_suffix(".stderr.txt")
                cmd = [binary["path"], case["operation"]]
                start = time.perf_counter()
                with path.open("rb") as inp, stdout.open("wb") as out, stderr.open("wb") as err:
                    process = subprocess.Popen(cmd, stdin=inp, stdout=out, stderr=err, env=binary["env"])
                    while args.timeout:
                        pid, status, usage = os.wait4(process.pid, os.WNOHANG)
                        if pid:
                            process.returncode = os.waitstatus_to_exitcode(status)
                            break
                        if time.perf_counter() - start > args.timeout:
                            process.kill()
                            _, status, _ = os.wait4(process.pid, 0)
                            process.returncode = os.waitstatus_to_exitcode(status)
                            raise TimeoutError(f'CLI timed out: {case["name"]} / {binary["label"]}')
                        time.sleep(0.005)
                    if not args.timeout:
                        _, status, usage = os.wait4(process.pid, 0)
                        process.returncode = os.waitstatus_to_exitcode(status)
                precise_wall = time.perf_counter() - start
                measured = {"wall_s": precise_wall, "user_s": usage.ru_utime,
                            "system_s": usage.ru_stime, "peak_rss_kib": usage.ru_maxrss,
                            "exit": process.returncode}
                metrics.write_text(json.dumps(measured) + "\n")
                output = stdout.read_bytes()
                error = stderr.read_bytes()
                result = (process.returncode, output, error if process.returncode else b"")
                if case["name"] not in expected:
                    expected[case["name"]] = result
                equal = result == expected[case["name"]]
                equivalent &= equal
                record = {"case": case["name"], "binary": binary["label"], "repeat": rep,
                          "input_sha256": case["input_sha256"], "stdout_sha256": hashlib.sha256(output).hexdigest(),
                          "precise_wall_s": precise_wall, "equal_complete_result": equal, **measured}
                record["cpu_s"] = measured["user_s"] + measured["system_s"]
                records.append(record)
                (args.out / "records.json").write_text(json.dumps(records, indent=2) + "\n")
                print(json.dumps(record), flush=True)
    summary = []
    for case in manifest:
        for binary in binaries:
            runs = [r for r in records if r["case"] == case["name"] and r["binary"] == binary["label"]]
            summary.append({"case": case["name"], "binary": binary["label"],
                            "runs": len(runs), "all_success": all(r["exit"] == 0 for r in runs),
                            **{key + "_median": statistics.median(r[key] for r in runs)
                               for key in ["precise_wall_s", "cpu_s", "peak_rss_kib"]}})
    result = {"binaries": binaries, "all_complete_results_equal": equivalent,
              "summary": summary, "records": records}
    (args.out / "report.json").write_text(json.dumps(result, indent=2) + "\n")
    if not equivalent:
        raise SystemExit("complete results differ; inspect records and stdout/stderr artifacts")


if __name__ == "__main__":
    main()
