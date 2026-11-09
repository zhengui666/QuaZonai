"""Partition all observed source records into bounded, minute-aligned units."""
import datetime
import json
import pathlib
import sys

import duckdb


if __name__ == '__main__':
    root, output = map(pathlib.Path, sys.argv[1:])
    processing_path = root/'market-data-processing-market-candidates.json'
    manifest = json.loads(processing_path.read_text())
    filters = next(s for s in manifest['profile']['streams'] if s['kind'] == 'trades')
    assert filters['equals'] == {'crossed': True}
    assert filters['not_equals'] == {'liquidation_method': 'backstop'}
    assert set(filters['not_in']) == {'direction'}
    expected_rows = manifest['processing_view']['diagnostic_candidate_market_taker_rows']
    inventory = json.loads((output.parent/'archive-inventory.json').read_text())
    for item in inventory['files']:
        date = item['path'].split('date=')[1].split('/')[0]
        assert all(datetime.datetime.fromisoformat(item[k]).date().isoformat() == date
                   for k in ['minimumTimestamp', 'maximumTimestamp'])
    db = duckdb.connect()
    db.execute("SET memory_limit='1GB'")
    db.execute('SET threads=2')
    rows = db.execute('''
        SELECT epoch_ms(timestamp)//60000 AS minute, count(*) AS rows
        FROM read_parquet(?, union_by_name=true)
        WHERE crossed=true AND liquidation_method IS DISTINCT FROM 'backstop'
          AND (direction IS NULL OR NOT list_contains(?, direction))
        GROUP BY minute ORDER BY minute
    ''', [[str(root/'perp_fills_all/date=*/fills.parquet'),
           str(root/'spot_fills/date=*/fills.parquet')], filters['not_in']['direction']]).fetchall()
    assert sum(n for _, n in rows) == expected_rows
    days = {}
    for minute, count in rows:
        assert count <= 1_000_000, 'minute_exceeds_unit_limit_requires_finer_partition'
        days.setdefault(minute//1440, []).append((minute*60, count))
    start = datetime.datetime.fromisoformat(manifest['start_date']).replace(tzinfo=datetime.timezone.utc)
    end = datetime.datetime.fromisoformat(manifest['end_date']).replace(tzinfo=datetime.timezone.utc)
    units = []
    for day in range(int(start.timestamp())//86400, int(end.timestamp())//86400):
        date = datetime.datetime.fromtimestamp(day*86400, datetime.timezone.utc).date().isoformat()
        prefixes = [f'{stream}/date={date}/' for stream in ['perp_fills_all', 'spot_fills']]
        begin, selected = day*86400, 0
        for second, count in days.get(day, []):
            if selected+count > 1_000_000:
                units.append(dict(startSeconds=begin, endSeconds=second,
                                  selectedSourceRows=selected, pathPrefixes=prefixes))
                begin, selected = second, 0
            selected += count
        units.append(dict(startSeconds=begin, endSeconds=(day+1)*86400,
                          selectedSourceRows=selected, pathPrefixes=prefixes))
    assert sum(u['selectedSourceRows'] for u in units) == expected_rows
    assert units[0]['startSeconds'] == int(start.timestamp())
    assert units[-1]['endSeconds'] == int(end.timestamp())
    assert all(a['endSeconds'] == b['startSeconds'] for a, b in zip(units, units[1:]))
    observed_minutes = {minute for minute, _ in rows}
    missing = sorted(set(range(int(start.timestamp())//60, int(end.timestamp())//60))-observed_minutes)
    gaps = []
    for minute in missing:
        if gaps and gaps[-1]['endSeconds'] == minute*60:
            gaps[-1]['endSeconds'] += 60
        else:
            gaps.append(dict(startSeconds=minute*60, endSeconds=(minute+1)*60))
    result = dict(status='RESOURCE_PLAN_ONLY_NATIVE_PREPARATION_AND_RESEARCH_PENDING',
                  processingDeclaration=str(processing_path),
                  sourceRevision=manifest['revision'], unitRowLimit=1_000_000,
                  selectedRows=expected_rows, unitCount=len(units),
                  maximumMinuteRows=max(n for _, n in rows),
                  minutesWithoutSelectedRows=(end-start).days*1440-len(rows),
                  intervalsWithoutSelectedRows=gaps,
                  limitations=['Known protocol classes excluded; remaining records are unqualified market candidates.',
                               'Source row counts precede native trade-identity checks.',
                               'Empty minutes do not create synthetic market observations.',
                               'Conversion plan is not a full-history factor or execution result.'],
                  units=units)
    with output.open('x') as handle:
        json.dump(result, handle, indent=2)
        handle.write('\n')
    print(json.dumps({k:v for k,v in result.items() if k != 'units'}))
