"""Resume public historical funding rates; original responses, no account queries."""
import datetime
import fcntl
import http.client
import json
import math
import os
import pathlib
import signal
import time
import urllib.error
import urllib.parse
import urllib.request

TASK = pathlib.Path(__file__).resolve().parent
ROOT = pathlib.Path('/home/zzy/.local/share/quazonai-ts/data/hyperliquid-funding-20250728-20261013')
START, END = 1753660800000, 1791936000000
URL = 'https://api.hyperliquid.xyz/info'


def save(path, value):
    temporary = path.with_suffix(path.suffix + '.tmp')
    temporary.write_text(json.dumps(value, ensure_ascii=False, indent=2) + '\n')
    temporary.replace(path)


def page(rows, coin, cursor):
    if rows is None:
        raise ValueError('NULL_RESPONSE')
    if not isinstance(rows, list):
        raise ValueError('NON_LIST_RESPONSE')
    for row in rows:
        if row.get('coin') != coin or not isinstance(row.get('time'), int):
            raise ValueError('INVALID_COIN_OR_TIME')
        if not cursor <= row['time'] < END:
            raise ValueError('TIME_OUTSIDE_REQUEST')
        if row.get('fundingRate') is None:
            raise ValueError('NULL_FUNDING_RATE')
    times = [r['time'] for r in rows]
    if times != sorted(set(times)):
        raise ValueError('NON_UNIQUE_OR_UNSORTED_TIMES')
    return max(times) + 1 if times else END


def main():
    stop_requested = False
    def request_stop(_signal, _frame):
        nonlocal stop_requested
        stop_requested = True
    signal.signal(signal.SIGTERM, request_stop)
    signal.signal(signal.SIGINT, request_stop)
    assert page([], 'BTC', START) == END
    assert page([{'coin': 'BTC', 'time': START, 'fundingRate': '0'}], 'BTC', START) == START + 1
    ROOT.mkdir(parents=True, exist_ok=True)
    lock = (ROOT / 'writer.lock').open('a')
    fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
    coverage = json.loads((TASK / 'market-coverage.json').read_text())
    coins = sorted({r['coin'] for r in coverage['marketsByDay'] if r['file'].startswith('perp_fills_all/')})
    assert len(coins) == 206
    manifest = {'endpoint': URL, 'type': 'fundingHistory', 'startTime': START,
                'endTime': END - 1, 'coins': coins, 'sourceRevision': coverage['sourceRevision'],
                'identitySource': str(TASK / 'market-coverage.json'),
                'limitations': ['Rates only; oracle and actual funding payments missing.',
                                'Empty response is not proof of no funding obligations.']}
    manifest_path = ROOT / 'manifest.json'
    if manifest_path.exists():
        assert json.loads(manifest_path.read_text()) == manifest
    else:
        save(manifest_path, manifest)
    progress = {'pid': os.getpid(), 'status': 'RUNNING', 'coinsTotal': len(coins), 'coinsCompleted': 0,
                'root': str(ROOT), 'coins': {}}
    try:
        for coin in coins:
            if stop_requested:
                progress['status'] = 'STOPPED_BY_SIGNAL'
                return
            directory = ROOT / urllib.parse.quote(coin, safe='')
            directory.mkdir(exist_ok=True)
            cursor, rows_count, times = START, 0, []
            for original in sorted(directory.glob('*.json')):
                record = json.loads(original.read_text())
                assert record['request'] == dict(type='fundingHistory', coin=coin, startTime=cursor, endTime=END - 1)
                rows = record['response']
                cursor = page(rows, coin, cursor)
                rows_count += len(rows)
                times.extend(r['time'] for r in rows)
            state = {'cursor': cursor, 'rows': rows_count, 'status': 'RUNNING'}
            progress['coins'][coin] = state
            while cursor < END:
                if stop_requested:
                    progress['status'] = 'STOPPED_BY_SIGNAL'
                    return
                progress['currentCoin'] = coin
                progress['updatedAt'] = datetime.datetime.now(datetime.timezone.utc).isoformat()
                save(TASK / 'funding-progress.json', progress)
                params = dict(type='fundingHistory', coin=coin, startTime=cursor, endTime=END - 1)
                target = directory / f'{cursor:016d}.json'
                raw_path = target.with_suffix('.response')
                started = time.monotonic()
                if raw_path.exists():
                    raw = raw_path.read_bytes()
                else:
                    for attempt in range(5):
                        if stop_requested:
                            progress['status'] = 'STOPPED_BY_SIGNAL'
                            return
                        request = urllib.request.Request(URL, data=json.dumps(params).encode(), headers={'Content-Type': 'application/json'})
                        try:
                            with urllib.request.urlopen(request, timeout=45) as response:
                                raw = response.read()
                            break
                        except (urllib.error.HTTPError, urllib.error.URLError, TimeoutError, http.client.HTTPException) as error:
                            code = getattr(error, 'code', None)
                            detail = {'request': params, 'attempt': attempt + 1, 'code': code,
                                      'error': str(error)}
                            if isinstance(error, urllib.error.HTTPError):
                                detail['body'] = error.read().decode('utf-8', errors='replace')
                            save(directory / f'{cursor:016d}.attempt-{time.time_ns()}-{attempt + 1}.error', detail)
                            if code is not None and code not in (429, 500, 502, 503, 504):
                                raise
                            if attempt == 4:
                                raise
                            time.sleep(5 * (2 ** attempt))
                    # Keep exact response bytes before parsing/validation, including unsupported/null.
                    with raw_path.open('xb') as original:
                        original.write(raw)
                rows = json.loads(raw)
                next_cursor = page(rows, coin, cursor)
                save(target, {'request': params, 'response': rows})
                rows_count += len(rows)
                times.extend(r['time'] for r in rows)
                cursor = next_cursor
                state.update(cursor=cursor, rows=rows_count)
                # Space request starts; network time already consumes the rate interval.
                time.sleep(max(0, (20 + math.ceil(len(rows) / 20)) * 0.081 - (time.monotonic() - started)))
            hours = {t // 3600000 for t in times}
            state.update(status='EMPTY' if not times else 'COMPLETED',
                         firstTime=min(times) if times else None, lastTime=max(times) if times else None,
                         observedHours=len(hours), missingWindowHours=(END - START) // 3600000 - len(hours),
                         missingHoursMeaning='Includes prelisting/postdelisting; not all are source gaps.')
            save(directory / 'coverage.summary', state)
            progress['coinsCompleted'] += 1
            save(TASK / 'funding-progress.json', progress)
        progress['status'] = 'COMPLETED'
    except Exception as error:
        progress.update(status='STOPPED_ERROR', error=f'{type(error).__name__}: {error}')
        raise
    finally:
        progress['updatedAt'] = datetime.datetime.now(datetime.timezone.utc).isoformat()
        save(TASK / 'funding-progress.json', progress)


if __name__ == '__main__':
    main()
