"""Finite parallel native day imports; native composition validates the full result."""
import argparse
import concurrent.futures
import datetime as dt
import fcntl
import json
import os
import pathlib
import subprocess
import time

TASK = pathlib.Path(__file__).resolve().parent
DATA = pathlib.Path('/home/zzy/.local/share/quazonai-ts/data')
SOURCE = DATA / 'hyperliquid-trades-20250728-20261013'
MANIFEST = SOURCE / 'market-data-processing-market-candidates.json'
INSTRUMENTS = DATA / 'imports/hyperliquid-identity-index-20261028/archive-instruments.json'


def save(path, value):
    temporary = path.with_suffix(path.suffix + '.tmp')
    temporary.write_text(json.dumps(value, indent=2) + '\n')
    temporary.replace(path)


def units():
    days = {}
    for line in (TASK / 'cohort-census-files.jsonl').open():
        row = json.loads(line)
        date = row['file'].split('date=')[1].split('/')[0]
        day = days.setdefault(date, dict(date=date, selectedRows=0, cohorts=0, files=[]))
        day['selectedRows'] += row['selectedRows']
        day['cohorts'] += row['cohortCount']
        day['files'].append(row['file'])
    source = json.loads(MANIFEST.read_text())
    start = dt.datetime.fromisoformat(source['start_date']).replace(tzinfo=dt.timezone.utc)
    ordered = [days[key] for key in sorted(days)]
    assert len(ordered) == (dt.date.fromisoformat(source['end_date']) - start.date()).days == 443
    for index, day in enumerate(ordered):
        assert day['date'] == (start + dt.timedelta(days=index)).date().isoformat()
        day['startSeconds'] = int(start.timestamp()) + index * 86400
        day['endSeconds'] = day['startSeconds'] + 86400
        day['files'].sort()
        assert day['files'] == [f'{prefix}/date={day["date"]}/fills.parquet'
                                for prefix in ['perp_fills_all', 'spot_fills']]
    summary = json.loads((TASK / 'cohort-census-progress.json').read_text())
    assert sum(day['selectedRows'] for day in ordered) == summary['selectedRows'] == 1308222597
    assert sum(day['cohorts'] for day in ordered) == summary['cohortCount'] == 614393279
    return ordered, source


def completed(output, unit, source):
    report = json.loads((output / 'import-report.json').read_text())
    cache = json.loads((output / 'catalog/cohort-cache.json').read_text())
    assert report['status'] == 'IMPORTED' and cache['representation'] == 'EVENT_TIME_COHORT_FLOW_CACHE_V1'
    assert report['source_manifest'] == source
    assert cache['source_revision'] == source['revision']
    assert int(cache['event_start_ns']) == unit['startSeconds'] * 1_000_000_000
    assert int(cache['event_end_ns']) == unit['endSeconds'] * 1_000_000_000
    assert report['outside_window_rows'] == 0
    assert report['selected_rows'] == int(cache['trade_count']) == unit['selectedRows']
    assert report['readback_records'] == int(cache['cohort_count']) == unit['cohorts']
    selected_files = sorted(entry['path'] for entry in source['files'] if entry.get('stream')
                            and any(entry['path'].startswith(prefix) for prefix in report['selected_path_prefixes']))
    assert selected_files == unit['files'], 'Native selected files differ from the complete day plan'
    assert not report['rejected_rows'] and not report['missing_symbols'] and report['duplicate_trades'] == 0
    for entry in cache['files']:
        assert (output / 'catalog' / entry['path']).stat().st_size == int(entry['bytes']) == int(entry['cohort_count']) * 40
    return dict(date=unit['date'], status='COMPLETED', output=str(output),
                selectedRows=unit['selectedRows'], cohorts=unit['cohorts'])


def run_unit(binary, root, unit, source):
    directory = root / 'units' / unit['date']
    directory.mkdir(parents=True, exist_ok=True)
    artifacts = sorted(directory.glob('attempt-*'))
    for running in directory.glob('attempt-*.running.json'):
        pid = json.loads(running.read_text())['pid']
        command = pathlib.Path(f'/proc/{pid}/cmdline')
        output = directory / running.name.removesuffix('.running.json')
        if command.exists() and str(output).encode() in command.read_bytes().split(b'\0'):
            raise RuntimeError('Original native unit process remains active')
    for attempt in (path for path in artifacts if path.is_dir()):
        if (attempt / 'catalog/cohort-cache.json').exists():
            return completed(attempt, unit, source)
    number = max((int(path.name.split('.')[0].split('-')[1]) for path in artifacts), default=0) + 1
    output = directory / f'attempt-{number:04d}'
    command = [str(binary), 'import', '--manifest', str(MANIFEST), '--instruments', str(INSTRUMENTS),
               '--cohort-flow-cache', '--native-precision',
               '--start-seconds', str(unit['startSeconds']), '--end-seconds', str(unit['endSeconds']),
               '--max-trade-rows', str(unit['selectedRows']),
               '--max-output-bytes', str(unit['cohorts'] * 40 + 10 * 1024 ** 2),
               '--output', str(output)]
    for prefix in ['perp_fills_all', 'spot_fills']:
        command.extend(['--path-prefix', f'{prefix}/date={unit["date"]}/'])
    started = time.perf_counter()
    with output.with_suffix('.log').open('x') as log:
        child = subprocess.Popen(command, stdout=log, stderr=subprocess.STDOUT)
        save(output.with_suffix('.running.json'), dict(pid=child.pid, command=command))
        code = child.wait()
    if code != 0:
        raise RuntimeError(f'Native unit {unit["date"]} exited {code}; original output retained')
    result = completed(output, unit, source)
    result['elapsedSeconds'] = round(time.perf_counter() - started, 3)
    save(output.with_suffix('.result.json'), result)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('binary', type=pathlib.Path)
    parser.add_argument('root', type=pathlib.Path)
    parser.add_argument('--workers', type=int, default=4)
    args = parser.parse_args()
    assert args.workers > 0
    binary = args.binary.resolve(strict=True)
    root = args.root.resolve()
    root.mkdir(parents=True, exist_ok=True)
    lock = (root / 'writer.lock').open('a')
    fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
    work, source = units()
    plan = dict(sourceRevision=source['revision'], processingManifest=str(MANIFEST),
                representation='EVENT_TIME_COHORT_FLOW_CACHE_V1', units=work)
    plan_file = root / 'plan.json'
    if plan_file.exists():
        assert json.loads(plan_file.read_text()) == plan
    else:
        save(plan_file, plan)
    state = dict(pid=os.getpid(), status='RUNNING', workers=args.workers, daysTotal=len(work),
                 daysCompleted=0, selectedRowsCompleted=0, cohortsCompleted=0, units=[], errors=[])
    progress = root / 'progress.json'
    pending = iter(work)
    began = time.perf_counter()
    with concurrent.futures.ThreadPoolExecutor(max_workers=args.workers) as pool:
        active = {}
        for unit in work[:args.workers]:
            next(pending)
            active[pool.submit(run_unit, binary, root, unit, source)] = unit['date']
        while active:
            state['activeDates'] = sorted(active.values())
            save(progress, state)
            done, _ = concurrent.futures.wait(active, return_when=concurrent.futures.FIRST_COMPLETED)
            for future in done:
                date = active.pop(future)
                try:
                    result = future.result()
                    state['units'].append(result)
                    state['daysCompleted'] += 1
                    state['selectedRowsCompleted'] += result['selectedRows']
                    state['cohortsCompleted'] += result['cohorts']
                except Exception as error:
                    state['errors'].append(dict(date=date, error=f'{type(error).__name__}: {error}'))
            if not state['errors']:
                for _ in done:
                    unit = next(pending, None)
                    if unit is not None:
                        active[pool.submit(run_unit, binary, root, unit, source)] = unit['date']
            state['elapsedSeconds'] = round(time.perf_counter() - began, 3)
            save(progress, state)
    state['activeDates'] = []
    state['status'] = 'STOPPED_ERROR' if state['errors'] else 'COMPLETED'
    if not state['errors']:
        assert state['daysCompleted'] == 443 and state['selectedRowsCompleted'] == 1308222597
        assert state['cohortsCompleted'] == 614393279
        save(root / 'completed-units.json', [u['output'] for u in sorted(state['units'], key=lambda u: u['date'])])
    save(progress, state)
    print(json.dumps({k: v for k, v in state.items() if k != 'units'}), flush=True)
    if state['errors']:
        raise SystemExit(1)


if __name__ == '__main__':
    main()
