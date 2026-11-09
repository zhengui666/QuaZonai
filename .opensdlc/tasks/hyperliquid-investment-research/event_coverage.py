"""Count algorithm/forced-flow annotations, without publishing trader identifiers."""
import datetime
import json
import pathlib
import sys

import duckdb


if __name__ == '__main__':
    root, output = map(pathlib.Path, sys.argv[1:])
    expected = json.loads((output.parent / 'coverage-summary.json').read_text())
    db = duckdb.connect()
    db.execute("SET memory_limit='1GB'")
    db.execute('SET threads=2')
    db.execute('SET temp_directory=?', [str(output.parent / 'event-coverage-temp')])
    records = db.execute('''
        SELECT filename, coin, count(*) AS taker_rows,
               count(*) FILTER (WHERE twap_id IS NOT NULL) AS twap_annotated_rows,
               count(*) FILTER (WHERE twap_id = 0) AS twap_zero_rows,
               count(DISTINCT twap_id) AS distinct_twap_ids,
               count(*) FILTER (WHERE is_liquidation = true) AS liquidation_rows,
               count(*) FILTER (WHERE is_liquidation IS NULL) AS liquidation_unknown_rows,
               count(*) FILTER (WHERE fee IS NULL OR fee_token IS NULL) AS fee_unknown_rows
        FROM read_parquet(?, filename=true, union_by_name=true)
        WHERE crossed = true
        GROUP BY filename, coin ORDER BY filename, coin
    ''', [[str(root / 'perp_fills_all/date=*/fills.parquet'),
           str(root / 'spot_fills/date=*/fills.parquet')]]).fetchall()
    assert sum(row[2] for row in records) == sum(expected[k]['takerRows'] for k in
                                               ['perp_fills_all', 'spot_fills'])
    keys = ['takerRows', 'twapAnnotatedRows', 'twapZeroRows', 'distinctTwapIds',
            'liquidationRows', 'liquidationUnknownRows', 'feeUnknownRows']
    rows = [dict(file=str(pathlib.Path(row[0]).relative_to(root)), coin=row[1],
                 **dict(zip(keys, row[2:], strict=True))) for row in records]
    totals = {}
    for stream in ['perp_fills_all', 'spot_fills']:
        subset = [row for row in rows if row['file'].startswith(stream + '/')]
        totals[stream] = {key: sum(row[key] for row in subset) for key in keys
                          if key != 'distinctTwapIds'}
        totals[stream]['marketDaysWithTwap'] = sum(row['twapAnnotatedRows'] > 0 for row in subset)
        totals[stream]['marketDaysWithLiquidations'] = sum(row['liquidationRows'] > 0 for row in subset)
    result = {'observedAt': datetime.datetime.now(datetime.timezone.utc).isoformat(),
              'limitations': ['Annotations are not proof of historical availability.',
                              'Counts precede trade-identity deduplication.',
                              'Distinct TWAP counts are local to each market/day; do not sum as unique orders.',
                              'Null liquidation flags remain unknown, not false.'],
              'totals': totals, 'marketsByDay': rows}
    with output.open('x') as handle:
        json.dump(result, handle, ensure_ascii=False, separators=(',', ':'))
        handle.write('\n')
    print(json.dumps(totals))
