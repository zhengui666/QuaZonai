#!/usr/bin/env python3
"""Runs the synthetic kernel process with resource accounting, no heavy deps."""
import csv, json, os, pathlib, platform, resource, subprocess, time

root = pathlib.Path(__file__).resolve().parent
def text(path):
    return pathlib.Path(path).read_text().strip()
metadata = {
    'kind': 'synthetic_structural_microbenchmark_not_production_engine_rt',
    'source_revision': 'd7da6bff197bcbf272ba998ae0a15b2c1b25071c',
    'rustc': subprocess.check_output([os.environ.get('RUSTC', 'rustc'),'--version'],text=True).strip(),
    'compile_flags': '--edition=2021 -C opt-level=3 -C debuginfo=1',
    'machine': platform.machine(),
    'cpu_model': next(line.split(':',1)[1].strip() for line in text('/proc/cpuinfo').splitlines() if line.startswith('model name')),
    'cpu_quota': text('/sys/fs/cgroup/cpu.max'),
    'memory_limit_bytes': text('/sys/fs/cgroup/memory.max'),
    'allowed_cpus': sorted(os.sched_getaffinity(0)),
    'pinned_cpu': 0,
    'warmup': 5,
    'samples': 30,
    'order': 'alternates baseline/candidate1/candidate2 then reversed',
    'quantiles': 'nearest-rank median and p95; no production-tail claim',
    'cgroup_cpu_before': text('/sys/fs/cgroup/cpu.stat'),
    'started_utc':time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime()),
}
start=time.monotonic()
with (root/'raw.csv').open('w') as stdout:
    result=subprocess.run(['taskset','-c','0',str(root/'kernels')],stdout=stdout,stderr=subprocess.PIPE,text=True)
metadata['wall_seconds']=time.monotonic()-start
usage=resource.getrusage(resource.RUSAGE_CHILDREN)
metadata.update(exit_code=result.returncode, stderr=result.stderr,
    maxrss_kib=usage.ru_maxrss, cpu_user_seconds=usage.ru_utime,
    cpu_system_seconds=usage.ru_stime, major_page_faults=usage.ru_majflt,
    minor_page_faults=usage.ru_minflt, voluntary_context_switches=usage.ru_nvcsw,
    involuntary_context_switches=usage.ru_nivcsw,
    cgroup_cpu_after=text('/sys/fs/cgroup/cpu.stat'))
assert result.returncode==0, result.stderr
summary=[]
for row in csv.reader((root/'raw.csv').open()):
    if row[0]=='SUMMARY':
        summary.append(dict(kernel=row[1],assets=int(row[2]),rows=int(row[3]),variant=row[4],
          median_ms=float(row[5]),p95_ms=float(row[6]),clone_calls=int(row[7]),
          cloned_target_elements=int(row[8]),coherence_scan_visits=int(row[9]),
          max_scan_visits=int(row[10]),instrument_comparisons=int(row[11])))
(root/'results.json').write_text(json.dumps(dict(metadata=metadata,summary=summary),indent=2)+'\n')
print(json.dumps(dict(metadata=metadata,summary=summary),indent=2))
