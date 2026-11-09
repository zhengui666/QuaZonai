"""Run native full-history composition once all finite day imports finish."""
import json
import pathlib
import resource
import subprocess
import sys
import time

from import_flow_days import INSTRUMENTS, MANIFEST, save


def main():
    binary, stage, output = map(pathlib.Path, sys.argv[1:])
    state = json.loads((stage / 'progress.json').read_text())
    assert state['status'] == 'COMPLETED' and not state['errors']
    assert state['daysCompleted'] == 443 and state['selectedRowsCompleted'] == 1308222597
    assert state['cohortsCompleted'] == 614393279
    units = json.loads((stage / 'completed-units.json').read_text())
    assert len(units) == len(set(units)) == 443
    assert units == [u['output'] for u in sorted(state['units'], key=lambda u: u['date'])]
    command = [str(binary.resolve(strict=True)), 'compose-cohorts', '--manifest', str(MANIFEST),
               '--instruments', str(INSTRUMENTS), '--output', str(output)]
    for unit in units:
        command.extend(['--units', unit])
    output.parent.mkdir(parents=True, exist_ok=True)
    started = time.perf_counter()
    with output.with_suffix('.compose.log').open('x') as log:
        child = subprocess.run(command, stdout=log, stderr=subprocess.STDOUT)
    save(output.with_suffix('.compose-result.json'), dict(
        status='COMPLETED' if child.returncode == 0 else 'STOPPED_ERROR',
        exitCode=child.returncode, elapsedSeconds=round(time.perf_counter() - started, 3),
        peakRssBytes=resource.getrusage(resource.RUSAGE_CHILDREN).ru_maxrss * 1024,
        sourceStage=str(stage), output=str(output), command=command))
    raise SystemExit(child.returncode)


if __name__ == '__main__':
    main()
