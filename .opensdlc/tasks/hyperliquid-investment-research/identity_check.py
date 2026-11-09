"""Check source trade keys on declared diagnostic days, not full-history deduplication."""
import json
import pathlib
import sys

import duckdb


if __name__ == '__main__':
    root, output = map(pathlib.Path, sys.argv[1:])
    dates = ['2025-07-28', '2026-03-06', '2026-10-13']
    coverage = json.loads((output.parent / 'market-coverage.json').read_text())
    db = duckdb.connect()
    db.execute("SET memory_limit='1GB'")
    db.execute('SET threads=1')
    db.execute('SET temp_directory=?', [str(output.parent / 'identity-temp')])
    results = []
    for date in dates:
        files = [str(root / stream / f'date={date}' / 'fills.parquet')
                 for stream in ['perp_fills_all', 'spot_fills']]
        rows = db.execute('''
            WITH identities AS (
                SELECT filename, coin, timestamp, trade_id, count(*) AS n,
                       min(price) <> max(price) OR min(size) <> max(size)
                         OR min(side) <> max(side) AS conflicting
                FROM read_parquet(?, filename=true, union_by_name=true)
                WHERE crossed = true
                GROUP BY filename, coin, timestamp, trade_id
            )
            SELECT filename, sum(n), count(*), count(*) FILTER (WHERE n > 1),
                   sum(n - 1), count(*) FILTER (WHERE conflicting)
            FROM identities GROUP BY filename ORDER BY filename
        ''', [files]).fetchall()
        for filename, selected, keys, repeated, excess, conflicts in rows:
            relative = str(pathlib.Path(filename).relative_to(root))
            expected = sum(row['takerRows'] for row in coverage['marketsByDay']
                           if row['file'] == relative)
            assert selected == expected and selected == keys + excess
            results.append(dict(file=relative, selectedRows=selected, distinctKeys=keys,
                                repeatedKeys=repeated, excessRows=excess, conflictingKeys=conflicts))
        print(date, json.dumps(results[-2:]), flush=True)
    with output.open('x') as handle:
        json.dump({'dates': dates, 'key': ['source_timestamp', 'coin', 'trade_id'],
                   'limitations': ['Diagnostic days only; not full-history deduplication.',
                                   'Source timestamp to official block_time mapping is not independently established.',
                                   'Conflicts compare price, size and side only.'], 'results': results},
                  handle, ensure_ascii=False, indent=2)
        handle.write('\n')
