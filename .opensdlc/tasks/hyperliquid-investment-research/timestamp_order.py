"""Check candidate per-coin source-order timestamps in bounded Arrow batches."""
import json
import os
import pathlib
import sys
import time

import duckdb
import pyarrow as pa
import pyarrow.compute as pc
import pyarrow.parquet as pq


def batch_stats(db, table):
    db.register('order_batch', table)
    return db.execute('''
        WITH numbered AS (SELECT *, row_number() OVER () AS ordinal FROM order_batch),
        adjacent AS (SELECT *, lag(event_ms) OVER (PARTITION BY coin ORDER BY ordinal) AS previous
                     FROM numbered)
        SELECT coin, count(*), count(*) FILTER (WHERE event_ms < previous),
               arg_min(event_ms, ordinal), arg_max(event_ms, ordinal), min(event_ms), max(event_ms)
        FROM adjacent GROUP BY coin
    ''').fetchall()


def merge(target, rows):
    for coin, count, reversals, first, last, minimum, maximum in rows:
        assert coin is not None and first is not None and last is not None
        previous = target.get(coin)
        boundary = int(previous is not None and first < previous['lastEventMs'])
        if previous is None:
            previous = target[coin] = dict(rows=0, adjacentReversals=0,
                                          firstEventMs=first, lastEventMs=last,
                                          minimumEventMs=minimum, maximumEventMs=maximum)
        previous['rows'] += count
        previous['adjacentReversals'] += reversals+boundary
        previous['lastEventMs'] = last
        previous['minimumEventMs'] = min(previous['minimumEventMs'], minimum)
        previous['maximumEventMs'] = max(previous['maximumEventMs'], maximum)


def self_check(db):
    stats = {}
    merge(stats, batch_stats(db, pa.table({'coin': ['A', 'B', 'A'], 'event_ms': [2, 3, 1]})))
    merge(stats, batch_stats(db, pa.table({'coin': ['B', 'A', 'A'], 'event_ms': [2, 1, 4]})))
    assert stats['A']['adjacentReversals'] == 1 and stats['B']['adjacentReversals'] == 1
    assert stats['A']['rows'] == 4 and stats['A']['firstEventMs'] == 2 and stats['A']['lastEventMs'] == 4


if __name__ == '__main__':
    root, output = map(pathlib.Path, sys.argv[1:])
    manifest = json.loads((root/'market-data-processing-market-candidates.json').read_text())
    filters = next(s for s in manifest['profile']['streams'] if s['kind'] == 'trades')
    assert filters['equals'] == {'crossed': True}
    assert filters['not_equals'] == {'liquidation_method': 'backstop'}
    assert set(filters['not_in']) == {'direction'}
    files = json.loads((output/'archive-inventory.json').read_text())['files']
    census = [json.loads(s) for s in (output/'cohort-census-files.jsonl').read_text().splitlines()]
    stress = max(census, key=lambda r: r['maximumCohortRows'])['file']
    sample = list(dict.fromkeys([files[0]['path'], files[-1]['path'], stress,
                                'perp_fills_all/date=2026-10-13/fills.parquet',
                                'spot_fills/date=2025-07-28/fills.parquet']))
    expected = {r['file']: r['selectedRows'] for r in census}
    db = duckdb.connect()
    db.execute("SET memory_limit='512MB'")
    db.execute('SET threads=2')
    self_check(db)
    started = time.monotonic()
    results = {}
    progress = dict(status='RUNNING', pid=os.getpid(), sourceRevision=manifest['revision'],
                    filesCompleted=0, filesTotal=len(files), sampleFiles=sample, selfCheck='PASSED',
                    method='PYARROW_FILTER_THEN_DUCKDB_PER_COIN_SOURCE_ORDINAL_LAG',
                    limitations=['Checks source row ordering only; no numerical, identity or availability qualification.',
                                 'No factor trials or cache generated.'])

    def save():
        progress['elapsedSeconds'] = round(time.monotonic()-started, 3)
        progress['selectedRows'] = sum(r['selectedRows'] for r in results.values())
        progress['adjacentReversalsWithinFiles'] = sum(r['adjacentReversals'] for r in results.values())
        progress['filesWithReversals'] = [p for p, r in results.items() if r['adjacentReversals']]
        temp = output/'timestamp-order-progress.tmp'
        temp.write_text(json.dumps(progress, indent=2)+'\n')
        temp.replace(output/'timestamp-order-progress.json')

    with (output/'timestamp-order-files.jsonl').open('x') as ledger:
        save()
        try:
            ordered = sample+[f['path'] for f in files if f['path'] not in sample]
            for path in ordered:
                before = time.monotonic()
                coins = {}
                for batch in pq.ParquetFile(root/path).iter_batches(batch_size=262144,
                        columns=['coin', 'timestamp', 'crossed', 'direction', 'liquidation_method']):
                    selected = pc.and_(pc.fill_null(pc.equal(batch.column('crossed'), True), False),
                                       pc.invert(pc.fill_null(pc.equal(batch.column('liquidation_method'), 'backstop'), False)))
                    selected = pc.and_(selected, pc.invert(pc.is_in(batch.column('direction'),
                                value_set=pa.array(filters['not_in']['direction']))))
                    filtered = batch.filter(selected)
                    if filtered.num_rows:
                        assert filtered.column('timestamp').null_count == 0
                        table = pa.table({'coin': filtered.column('coin'),
                                          'event_ms': pc.cast(filtered.column('timestamp'), pa.int64())})
                        merge(coins, batch_stats(db, table))
                result = dict(file=path, selectedRows=sum(c['rows'] for c in coins.values()),
                              adjacentReversals=sum(c['adjacentReversals'] for c in coins.values()),
                              elapsedSeconds=round(time.monotonic()-before, 3), coins=coins)
                assert result['selectedRows'] == expected[path], 'candidate_filter_count_mismatch'
                results[path] = result
                ledger.write(json.dumps(result)+'\n')
                ledger.flush()
                progress['filesCompleted'] = len(results)
                progress['lastFile'] = path
                if len(results) == len(sample):
                    progress['sampleElapsedSeconds'] = round(time.monotonic()-started, 3)
                    progress['sampleAdjacentReversals'] = sum(r['adjacentReversals'] for r in results.values())
                save()
            chronological = {}
            boundary_reversals = 0
            for path in sorted(results):
                for coin, stats in results[path]['coins'].items():
                    if coin in chronological and stats['firstEventMs'] < chronological[coin]:
                        boundary_reversals += 1
                    chronological[coin] = stats['lastEventMs']
            progress['acrossFileAdjacentReversals'] = boundary_reversals
            assert progress['selectedRows'] == manifest['processing_view']['diagnostic_candidate_market_taker_rows']
            progress['status'] = 'COMPLETED'
            save()
        except BaseException as error:
            progress.update(status='FAILED', error=str(error))
            save()
            raise
    print(json.dumps(progress))
