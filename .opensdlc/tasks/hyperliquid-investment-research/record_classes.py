"""Count original record classes; retain all raw files and zero identifiers."""
import json
import pathlib
import sys
import duckdb

if __name__ == '__main__':
    root, output = map(pathlib.Path, sys.argv[1:])
    db = duckdb.connect()
    db.execute("SET memory_limit='1GB'")
    db.execute('SET threads=2')
    rows = db.execute('''
        SELECT filename, direction, count(*) AS rows,
               count(*) FILTER (WHERE trade_id=0) AS zero_ids,
               count(*) FILTER (WHERE trade_id IS NULL) AS missing_ids
        FROM read_parquet(?, filename=true, union_by_name=true)
        WHERE crossed=true
        GROUP BY filename,direction ORDER BY filename,direction
    ''', [[str(root/'perp_fills_all/date=*/fills.parquet'),
           str(root/'spot_fills/date=*/fills.parquet')]]).fetchall()
    expected = json.loads((output.parent/'market-coverage.json').read_text())['marketsByDay']
    assert sum(r[2] for r in rows) == sum(r['takerRows'] for r in expected)
    records = [dict(file=str(pathlib.Path(f).relative_to(root)), direction=d,
                    rows=n, zeroIds=z, missingIds=m) for f,d,n,z,m in rows]
    totals = {}
    for r in records:
        key = r['file'].split('/')[0] + ':' + str(r['direction'])
        group = totals.setdefault(key, dict(rows=0,zeroIds=0,missingIds=0))
        for field in group:
            group[field] += r[field]
    with output.open('x') as handle:
        json.dump(dict(method='FULL_ARCHIVE_TAKER_DIRECTION_ID_COUNTS',
                       limitations=['Record labels do not independently prove execution semantics.'],
                       totals=totals, records=records), handle, ensure_ascii=False, indent=2)
        handle.write('\n')
    print(json.dumps(totals))
