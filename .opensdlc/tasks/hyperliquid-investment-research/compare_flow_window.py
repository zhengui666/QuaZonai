"""Compare every real five-minute flow frame with the original decimal rows."""
import collections
import decimal
import json
import pathlib
import struct

import duckdb

TASK = pathlib.Path(__file__).resolve().parent
DATA = pathlib.Path('/home/zzy/.local/share/quazonai-ts/data')
SOURCE = DATA / 'hyperliquid-trades-20250728-20261013'
OUTPUT = DATA / 'imports/hyperliquid-flow-cache-unit-20261013-0000/native'
FRAME = struct.Struct('<QddQd')


def main():
    assert FRAME.size == 40
    processing = json.loads((SOURCE / 'market-data-processing-market-candidates.json').read_text())
    profile = next(s for s in processing['profile']['streams'] if s['kind'] == 'trades')
    identities = json.loads((DATA / 'imports/hyperliquid-identity-index-20261028/archive-instruments.json').read_text())['symbol_ids']
    paths = [str(SOURCE / stream / 'date=2026-10-13/fills.parquet')
             for stream in ['perp_fills_all', 'spot_fills']]
    db = duckdb.connect()
    db.execute("SET memory_limit='512MB'")
    db.execute('SET threads=2')
    rows = db.execute('''
        SELECT coin, epoch_ms(timestamp), price, size, side
        FROM read_parquet(?, union_by_name=true)
        WHERE epoch_ms(timestamp)>=1791849600000 AND epoch_ms(timestamp)<1791849900000
          AND crossed=true AND liquidation_method IS DISTINCT FROM 'backstop'
          AND (direction IS NULL OR NOT list_contains(?, direction))
    ''', [paths, profile['not_in']['direction']]).fetchall()
    expected = {}
    with decimal.localcontext() as context:
        context.prec = 80
        for coin, event_ms, price, quantity, side in rows:
            assert price > 0 and quantity > 0 and side in ('buy', 'sell')
            key = (identities[coin], event_ms * 1_000_000)
            state = expected.setdefault(key, [decimal.Decimal(0), decimal.Decimal(0), 0, decimal.Decimal(0)])
            state[0] += price * quantity
            state[1] += quantity
            state[2] += 1
            state[3] += quantity if side == 'buy' else -quantity
        wanted = {key: (float(cash / quantity), float(quantity), count, float(signed))
                  for key, (cash, quantity, count, signed) in expected.items()}
    manifest = json.loads((OUTPUT / 'catalog/cohort-cache.json').read_text())
    report = json.loads((OUTPUT / 'import-report.json').read_text())
    assert manifest['representation'] == 'EVENT_TIME_COHORT_FLOW_CACHE_V1'
    assert manifest['source_revision'] == processing['revision']
    assert report['status'] == 'IMPORTED' and report['selected_rows'] == len(rows) == 29756
    assert int(manifest['trade_count']) == len(rows)
    assert int(manifest['cohort_count']) == report['readback_records'] == len(wanted)
    seen, actual_trades, balanced = set(), 0, 0
    signs = collections.Counter()
    for entry in manifest['files']:
        with (OUTPUT / 'catalog' / entry['path']).open('rb') as stream:
            while raw := stream.read(FRAME.size):
                assert len(raw) == FRAME.size
                event, price, quantity, count, signed = FRAME.unpack(raw)
                key = (entry['instrument_id'], event)
                assert key not in seen and key in wanted
                assert (price, quantity, count, signed) == wanted[key], ('Native/source frame differs', key)
                seen.add(key)
                actual_trades += count
                balanced += signed == 0
                signs['positive' if signed > 0 else 'negative' if signed < 0 else 'zero'] += 1
    assert seen == set(wanted) and actual_trades == len(rows)
    result = dict(status='MATCHED', sourceRevision=processing['revision'],
                  localBuildRevision='0e508195ff5c1c500aae4fcec99df7942317d1c4',
                  sourceWindowStartSeconds=1791849600, sourceWindowEndSeconds=1791849900,
                  selectedSourceRows=len(rows), cohortsCompared=len(seen),
                  instruments=len(manifest['files']), balancedCohorts=balanced, signedCohorts=dict(signs),
                  comparison='Exact event/count and IEEE-f64 equality for VWAP, quantity and BUYER-minus-SELLER quantity after independent decimal reduction.',
                  limitations=['Real five-minute conversion only; full history not converted.',
                               'No factor trial or prediction result; no execution or profitability conclusion.',
                               'Current definitions and event-availability proxy remain unqualified.'])
    (TASK / 'flow-window-source-comparison.json').write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps(result))


if __name__ == '__main__':
    main()
