"""Aggregate full-archive market coverage without wallet/order identifiers."""
import datetime
import json
import pathlib
import sys

import duckdb


if __name__ == '__main__':
    root, output = map(pathlib.Path, sys.argv[1:])
    inventory = json.loads((output.parent / 'archive-inventory.json').read_text())
    expected = sum(row['rows'] for row in inventory['files'])
    db = duckdb.connect()
    db.execute("SET memory_limit='1GB'")
    db.execute('SET threads=2')
    db.execute('SET temp_directory=?', [str(output.parent / 'coverage-temp')])
    rows = db.execute('''
        SELECT filename, coin, count(*) AS raw_rows,
               count(*) FILTER (WHERE crossed = true) AS taker_rows,
               count(*) FILTER (WHERE crossed = true AND side = 'buy') AS taker_buy_rows,
               count(*) FILTER (WHERE crossed = true AND side = 'sell') AS taker_sell_rows,
               count(*) FILTER (WHERE crossed IS NULL OR coin IS NULL OR timestamp IS NULL) AS missing_required_rows,
               min(timestamp)::VARCHAR AS first_event, max(timestamp)::VARCHAR AS last_event
        FROM read_parquet(?, filename=true, union_by_name=true)
        GROUP BY filename, coin ORDER BY filename, coin
    ''', [[str(root / 'perp_fills_all/date=*/fills.parquet'),
           str(root / 'spot_fills/date=*/fills.parquet')]]).fetchall()
    assert sum(row[2] for row in rows) == expected, 'coverage_row_count_mismatch'
    result = {'observedAt': datetime.datetime.now(datetime.timezone.utc).isoformat(),
              'sourceRevision': inventory['sourceRevision'], 'rawRows': expected,
              'method': 'FULL_ARCHIVE_COLUMNS_COIN_CROSSED_SIDE_TIMESTAMP',
              'limitations': ['Taker rows are source selections, not deduplicated trade identities.',
                              'Per-day first/last events do not prove continuous market coverage.',
                              'No receive-time, execution or return calculation.'], 'marketsByDay': []}
    for filename, coin, raw, taker, buy, sell, missing, first, last in rows:
        result['marketsByDay'].append({'file': str(pathlib.Path(filename).relative_to(root)),
            'coin': coin, 'rawRows': raw, 'takerRows': taker, 'takerBuyRows': buy,
            'takerSellRows': sell, 'missingRequiredRows': missing,
            'firstEvent': first, 'lastEvent': last})
    with output.open('x') as handle:
        json.dump(result, handle, ensure_ascii=False, indent=2)
        handle.write('\n')
    print(json.dumps({'rawRows': expected, 'marketDays': len(rows),
                      'takerRows': sum(row[3] for row in rows)}))
