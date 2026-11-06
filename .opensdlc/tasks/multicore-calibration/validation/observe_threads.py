#!/usr/bin/env python3
"""Separate observability run; polling is deliberately excluded from timing trials."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import time

p = argparse.ArgumentParser()
p.add_argument("--binary", type=Path, required=True)
p.add_argument("--input", type=Path, required=True)
p.add_argument("--threads", type=int, help="omit to observe the production default")
p.add_argument("--out", type=Path, required=True)
a = p.parse_args()
a.out.mkdir(parents=True, exist_ok=True)
observed = set()
peak = 0
peak_calibration = 0
with a.input.open("rb") as inp, (a.out / "stdout.json").open("wb") as out, (a.out / "stderr.txt").open("wb") as err:
    env = {"PATH": os.defpath}
    if a.threads is not None:
        env["QZ_CALIBRATION_THREADS"] = str(a.threads)
    proc = subprocess.Popen([str(a.binary.resolve()), "fit-ensemble"], stdin=inp, stdout=out, stderr=err, env=env)
    while proc.poll() is None:
        names = []
        for path in Path(f"/proc/{proc.pid}/task").glob("*/comm"):
            try:
                names.append(path.read_text().strip())
            except FileNotFoundError:
                pass
        observed.update(names)
        peak = max(peak, len(names))
        peak_calibration = max(peak_calibration, names.count("qz-calibration"))
        time.sleep(0.001)
report = {"exit": proc.returncode, "requested_threads": a.threads,
          "peak_total_threads": peak, "peak_spawned_calibration_threads": peak_calibration,
          "thread_names": sorted(observed), "input_sha256": hashlib.sha256(a.input.read_bytes()).hexdigest(),
          "scope": "Separate thread-observability run, not a timing or memory benchmark"}
(a.out / "report.json").write_text(json.dumps(report, indent=2) + "\n")
print(json.dumps(report))
