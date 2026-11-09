import collections
import json
from decimal import Decimal
from pathlib import Path
import duckdb

root = Path(__file__).resolve().parent.parent
batch = root / 'spot-valuation-batch-20261030'
quotes = [json.loads(s) for s in (batch / 'spot-valuation.jsonl').read_text().splitlines()]
provenance = json.loads((batch / 'provenance.json').read_text())
assert len(quotes) == len(provenance) == 188
by_asset = collections.defaultdict(list)
lag_by_asset = collections.defaultdict(list)
price_pairs = collections.defaultdict(collections.Counter)
missing_lag = 0
con = duckdb.connect()
for quote, source in zip(quotes, provenance, strict=True):
    instrument = quote['instrument_id']
    assert instrument == f"{source['currency']}-USDC-SPOT.HYPERLIQUID"
    bid, ask = Decimal(quote['bid']), Decimal(quote['ask'])
    assert bid > 0 and ask >= bid
    spread = (ask - bid) / ((ask + bid) / 2) * 10000
    by_asset[source['currency']].append(float(spread))
    price_pairs[source['currency']].update([(str(bid), str(ask))])
    events = con.execute('select distinct event_time from read_parquet(?) where received_time = ?', [source['file'], int(quote['received_ns'])]).fetchall()
    if len(events) != 1 or events[0][0] is None:
        missing_lag += 1
    else:
        lag_by_asset[source['currency']].append((int(quote['received_ns']) - int(events[0][0]) * 1_000_000) / 1_000_000)

def stats(xs):
    xs = sorted(xs)
    def q(p):
        i = (len(xs)-1)*p
        lo = int(i)
        return xs[lo] + (xs[min(lo+1, len(xs)-1)]-xs[lo])*(i-lo)
    return {'n': len(xs), 'min': xs[0], 'p25': q(.25), 'median': q(.5), 'p75': q(.75), 'max': xs[-1]}

assert sum(map(len, by_asset.values())) == 188
result = {'source': str(batch / 'spot-valuation.jsonl'), 'definition': '10000*(ask-bid)/((ask+bid)/2)', 'unit': 'basis points', 'quantile_method': 'linear interpolation in sorted sample', 'per_asset_spread_bps': {k: stats(v) for k,v in sorted(by_asset.items())}, 'all_spread_bps': stats([v for vals in by_asset.values() for v in vals]), 'quote_repetition': {k: {'unique_pairs': len(v), 'max_pair_occurrences': v.most_common(1)[0][1]} for k, v in sorted(price_pairs.items())}, 'receive_minus_event_ms': {'all': stats([v for vals in lag_by_asset.values() for v in vals]), 'per_asset': {k: stats(v) for k,v in sorted(lag_by_asset.items())}, 'missing_or_ambiguous': missing_lag, 'negative_count': sum(v < 0 for vals in lag_by_asset.values() for v in vals)}, 'limitations': ['Convenience sample: last snapshot at day end for first missing valuation asset, not random and not all balances or periods.', 'Displayed spread excludes fees, slippage, size/depth and market impact; top-of-book is not a guaranteed executable round trip.', 'Provider notes historical empty-book omissions can leave stale levels.', 'Receive and event clocks are different systems; their difference is not measured local network latency.', 'Static 2026-10-28 instrument mapping is not verified point in time.']}
(root / 'spot-spread-primary-20261030' / 'summary.json').write_text(json.dumps(result, indent=2, ensure_ascii=False) + '\n')
print(json.dumps(result, ensure_ascii=False))
