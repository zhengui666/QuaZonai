"""Count full-source exact (coin, timestamp) cohorts; never read wallet fields."""
import datetime
import json
import os
import pathlib
import sys
import time

import duckdb


def tally(db, source, parameters, excluded):
    return db.execute(f'''
        WITH cohorts AS (
            SELECT coin, timestamp, count(*) AS n FROM {source}
            WHERE crossed=true AND liquidation_method IS DISTINCT FROM 'backstop'
              AND (direction IS NULL OR NOT list_contains(?, direction))
            GROUP BY coin, timestamp
        )
        SELECT coin, count(*), sum(n), max(n), min(timestamp)::VARCHAR,
               max(timestamp)::VARCHAR,
               sum(CASE WHEN coin IS NULL OR timestamp IS NULL THEN n ELSE 0 END)
        FROM cohorts GROUP BY coin ORDER BY coin
    ''', [*parameters, excluded]).fetchall()


def self_check(db):
    db.execute('''CREATE TEMP TABLE census_check AS SELECT * FROM (VALUES
        ('BTC', TIMESTAMPTZ '2026-01-01 00:00:00+00', true, NULL, NULL),
        ('BTC', TIMESTAMPTZ '2026-01-01 00:00:00+00', true, NULL, NULL),
        ('BTC', TIMESTAMPTZ '2026-01-01 00:00:01+00', true, NULL, NULL),
        ('ETH', TIMESTAMPTZ '2026-01-01 00:00:00+00', true, NULL, NULL),
        ('BTC', TIMESTAMPTZ '2026-01-01 00:00:02+00', true, 'backstop', NULL),
        ('BTC', TIMESTAMPTZ '2026-01-01 00:00:03+00', true, NULL, 'Settlement'),
        ('BTC', TIMESTAMPTZ '2026-01-01 00:00:04+00', false, NULL, NULL)
    ) AS t(coin,timestamp,crossed,liquidation_method,direction)''')
    rows = tally(db, 'census_check', [], ['Settlement'])
    assert [(r[0], r[1], r[2], r[3], r[6]) for r in rows] == [
        ('BTC', 2, 3, 2, 0), ('ETH', 1, 1, 1, 0)]
    db.execute('DROP TABLE census_check')


if __name__ == '__main__':
    root, output = map(pathlib.Path, sys.argv[1:])
    declaration = root/'market-data-processing-market-candidates.json'
    manifest = json.loads(declaration.read_text())
    filters = next(s for s in manifest['profile']['streams'] if s['kind'] == 'trades')
    assert filters['equals'] == {'crossed': True}
    assert filters['not_equals'] == {'liquidation_method': 'backstop'}
    assert set(filters['not_in']) == {'direction'}
    inventory = json.loads((output/'archive-inventory.json').read_text())
    expected = manifest['processing_view']['diagnostic_candidate_market_taker_rows']
    assert inventory['sourceRevision'] == manifest['revision']
    files = inventory['files']
    seen_dates = set()
    for item in files:
        stream, date = item['path'].split('/date=')
        date = date.split('/')[0]
        assert stream in ('perp_fills_all', 'spot_fills')
        assert (stream, date) not in seen_dates
        seen_dates.add((stream, date))
        assert all(datetime.datetime.fromisoformat(item[k]).date().isoformat() == date
                   for k in ('minimumTimestamp', 'maximumTimestamp'))
    db = duckdb.connect()
    db.execute("SET memory_limit='768MB'")
    db.execute('SET threads=2')
    db.execute('SET temp_directory=?', [str(output/'cohort-census-temp')])
    self_check(db)
    started = time.monotonic()
    progress = dict(status='RUNNING', pid=os.getpid(), sourceRevision=manifest['revision'],
                    processingDeclaration=str(declaration), filesCompleted=0, filesTotal=len(files),
                    selectedRows=0, cohortCount=0, maximumCohortRows=0,
                    selfCheck='PASSED', method='FULL_SOURCE_PER_PARQUET_COIN_TIMESTAMP_GROUPING',
                    limitations=['Candidate records precede native identity and numerical conversion checks.',
                                 'Storage estimates exclude metadata, encoding and compression; no cache produced.',
                                 'No factor trials, return calculation or execution qualification.'])
    streams = {}

    def save():
        progress['elapsedSeconds'] = round(time.monotonic()-started, 3)
        progress['estimatedFixedWidthBytes'] = {
            'eventU64VwapF64QuantityF64CountU64': progress['cohortCount']*32,
            'plusExplicitAvailableU64': progress['cohortCount']*40}
        temporary = output/'cohort-census-progress.tmp'
        temporary.write_text(json.dumps(progress, indent=2)+'\n')
        temporary.replace(output/'cohort-census-progress.json')

    with (output/'cohort-census-files.jsonl').open('x') as ledger:
        save()
        try:
            for item in files:
                before = time.monotonic()
                rows = tally(db, 'read_parquet(?)', [str(root/item['path'])],
                             filters['not_in']['direction'])
                assert all(r[6] == 0 for r in rows), 'missing_coin_or_timestamp'
                stream = item['path'].split('/')[0]
                streams.setdefault(stream, set()).update(r[0] for r in rows)
                assert not (streams.get('perp_fills_all', set()) & streams.get('spot_fills', set())), 'cross_stream_coin_overlap'
                record = dict(file=item['path'], selectedRows=sum(r[2] for r in rows),
                              cohortCount=sum(r[1] for r in rows),
                              maximumCohortRows=max((r[3] for r in rows), default=0),
                              elapsedSeconds=round(time.monotonic()-before, 3),
                              coins=[dict(coin=r[0], cohortCount=r[1], selectedRows=r[2],
                                          maximumCohortRows=r[3], firstEvent=r[4], lastEvent=r[5])
                                     for r in rows])
                ledger.write(json.dumps(record)+'\n')
                ledger.flush()
                for field in ('selectedRows', 'cohortCount'):
                    progress[field] += record[field]
                progress['maximumCohortRows'] = max(progress['maximumCohortRows'], record['maximumCohortRows'])
                progress['filesCompleted'] += 1
                progress['lastFile'] = item['path']
                save()
            assert progress['selectedRows'] == expected, 'candidate_row_total_mismatch'
            progress['status'] = 'COMPLETED'
            progress['expectedSelectedRows'] = expected
            progress['streamCoinCounts'] = {k: len(v) for k, v in streams.items()}
            save()
        except BaseException as error:
            progress['status'] = 'FAILED'
            progress['error'] = str(error)
            save()
            raise
    print(json.dumps(progress))
