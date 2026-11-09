"""Measure one real import unit with a matching tested native operator."""
import argparse
import json
import pathlib
import resource
import subprocess
import sys
import time


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('binary', type=pathlib.Path)
    parser.add_argument('output', type=pathlib.Path)
    parser.add_argument('--flow-cache', action='store_true')
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    output = args.output.resolve()
    data = pathlib.Path('/home/zzy/.local/share/quazonai-ts/data')
    manifest = data/'hyperliquid-trades-20250728-20261013/market-data-processing-market-candidates.json'
    instruments = data/'imports/hyperliquid-identity-index-20261028/archive-instruments.json'
    command = [str(binary), 'import', '--manifest', str(manifest),
               '--instruments', str(instruments),
               '--cohort-flow-cache' if args.flow_cache else '--trade-ticks',
               '--path-prefix', 'perp_fills_all/date=2026-10-13/',
               '--path-prefix', 'spot_fills/date=2026-10-13/',
               '--native-precision', '--start-seconds', '1791849600',
               '--end-seconds', '1791849900', '--output', str(output/'native')]
    output.mkdir(parents=True, exist_ok=False)
    started = time.perf_counter()
    with (output/'operator.log').open('x') as log:
        completed = subprocess.run(command, stdout=log, stderr=subprocess.STDOUT, check=False)
    elapsed = time.perf_counter() - started
    report_file = output/'native/import-report.json'
    report = json.loads(report_file.read_text()) if report_file.exists() else {}
    files = [p for p in (output/'native').rglob('*') if p.is_file()]
    result = dict(command=command, operatorExitCode=completed.returncode,
                  elapsedSeconds=elapsed,
                  peakChildRssBytes=resource.getrusage(resource.RUSAGE_CHILDREN).ru_maxrss*1024,
                  nativeOutputBytes=sum(p.stat().st_size for p in files),
                  nativeOutputFiles=len(files), status=report.get('status'),
                  sourceRowsScanned=report.get('source_rows'),
                  filteredRows=report.get('filtered_rows'),
                  selectedRows=report.get('selected_rows'),
                  readbackRecords=report.get('readback_records'),
                  representation=report.get('representation', 'NATIVE_TRADE_TICK'),
                  nativeRecordKind='cohorts' if args.flow_cache else 'trades',
                  purpose='Resource measurement only; not a factor trial or full-history result.')
    (output/'measurement.json').write_text(json.dumps(result, indent=2)+'\n')
    print(json.dumps(result))
    if completed.returncode == 0:
        assert report['status'] == 'IMPORTED'
        kind = 'cohorts' if args.flow_cache else 'trades'
        assert report['readback_records'] == sum(report['native_records'][kind].values()) > 0
        if args.flow_cache:
            assert report['representation'] == 'EVENT_TIME_COHORT_FLOW_CACHE_V1'
    sys.exit(completed.returncode)
