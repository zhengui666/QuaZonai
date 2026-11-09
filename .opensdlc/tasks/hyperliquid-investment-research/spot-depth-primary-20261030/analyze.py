import collections
import datetime as dt
import json
from decimal import Decimal
from pathlib import Path

import duckdb

ROOT = Path(__file__).resolve().parent.parent
BATCH = ROOT / 'spot-valuation-batch-20261030'
D = Decimal


def vwap(levels, amount):
    left, base = amount, D(0)
    for price, size in levels:
        notional = price * size
        take = min(left, notional)
        base += take / price
        left -= take
        if left <= 0:
            return amount / base
    return None


def stats(values):
    values = sorted(float(v) for v in values)
    if not values:
        return {'n': 0}
    def quantile(p):
        index = (len(values) - 1) * p
        low = int(index)
        return values[low] + (values[min(low + 1, len(values) - 1)] - values[low]) * (index - low)
    return {'n': len(values), 'min': values[0], 'p25': quantile(.25), 'median': quantile(.5), 'p75': quantile(.75), 'max': values[-1]}


# Test both a multi-level fill and insufficient visible depth.
assert vwap([(D(2), D(3)), (D(4), D(1))], D(10)) == D('2.5')
assert vwap([(D(2), D(3))], D(10)) is None
quotes = [json.loads(line) for line in (BATCH / 'spot-valuation.jsonl').read_text().splitlines()]
provenance = json.loads((BATCH / 'provenance.json').read_text())
assert len(quotes) == len(provenance) == 188
con = duckdb.connect()
groups = collections.defaultdict(list)
for quote, src in zip(quotes, provenance, strict=True):
    asset = src['currency']
    assert quote['instrument_id'] == f'{asset}-USDC-SPOT.HYPERLIQUID'
    end_ns = int((dt.datetime.fromisoformat(src['day']).replace(tzinfo=dt.timezone.utc) + dt.timedelta(days=1)).timestamp()) * 1_000_000_000
    received = int(quote['received_ns'])
    assert end_ns - 3600 * 1_000_000_000 <= received < end_ns
    latest = con.execute('select max(received_time) from read_parquet(?) where received_time >= ? and received_time < ?', [src['file'], end_ns - 3600 * 1_000_000_000, end_ns]).fetchone()[0]
    assert latest == received
    rows = con.execute('select symbol, event_type, event_time, side, price, quantity from read_parquet(?) where received_time = ?', [src['file'], received]).fetchall()
    assert {r[0] for r in rows} == {src['source_symbol']} and {r[1] for r in rows} == {'snapshot'}
    assert len({r[2] for r in rows}) == 1
    sides = {'bid': [], 'ask': []}
    for _, _, _, side, price, quantity in rows:
        p, q = D(str(price)), D(str(quantity))
        assert side in sides and p.is_finite() and q.is_finite() and p > 0 and q > 0
        sides[side].append((p, q))
    sides['bid'].sort(reverse=True)
    sides['ask'].sort()
    assert sides['bid'] and sides['ask']
    bid, ask = sides['bid'][0][0], sides['ask'][0][0]
    assert str(bid) == quote['bid'] and str(ask) == quote['ask'] and bid <= ask
    mid = (bid + ask) / 2
    item = {'asset': asset, 'day': src['day'], 'top_spread_bps': float((ask - bid) / mid * 10000)}
    for side in ('bid', 'ask'):
        item[side + '_depth_usdc'] = float(sum((p * q for p, q in sides[side]), D(0)))
    for amount in (10, 30):
        buy = vwap(sides['ask'], D(amount))
        sell = vwap(sides['bid'], D(amount))
        item[f'buy_{amount}_vwap'] = float(buy) if buy is not None else None
        item[f'sell_{amount}_vwap'] = float(sell) if sell is not None else None
        item[f'effective_{amount}_spread_bps'] = float((buy - sell) / mid * 10000) if buy is not None and sell is not None else None
    groups[asset].append(item)


def summarize(items):
    output = {'snapshots': len(items), 'bid_visible_depth_usdc': stats(x['bid_depth_usdc'] for x in items), 'ask_visible_depth_usdc': stats(x['ask_depth_usdc'] for x in items), 'top_spread_bps': stats(x['top_spread_bps'] for x in items)}
    for amount in (10, 30):
        buy = [x for x in items if x[f'buy_{amount}_vwap'] is not None]
        sell = [x for x in items if x[f'sell_{amount}_vwap'] is not None]
        both = [x for x in items if x[f'effective_{amount}_spread_bps'] is not None]
        output[str(amount)] = {'buy_available': len(buy), 'buy_insufficient_depth': len(items)-len(buy), 'sell_available': len(sell), 'sell_insufficient_depth': len(items)-len(sell), 'both_available': len(both), 'effective_spread_bps': stats(x[f'effective_{amount}_spread_bps'] for x in both)}
    return output

all_items = [item for items in groups.values() for item in items]
result = {'source': str(BATCH), 'method': 'Last snapshot received before UTC day-end within 23:00 hour; consume ask ascending for fixed USDC buy and bid descending for independent fixed USDC sell; VWAP = quote notional/base quantity. Effective spread = (buy VWAP - sell VWAP)/top-of-book mid in basis points. Buy and sell are independently sized by quote notional, not an actual same-inventory round trip.', 'all': summarize(all_items), 'per_asset': {key: summarize(value) for key, value in sorted(groups.items())}, 'limitations': ['First missing asset per day convenience sample, not random or a full history.', 'Visible snapshot depth is not assured executable; historical empty-book omissions can leave stale levels.', 'Costs exclude fees, impact, queue, slippage after snapshot, and order-size increment/minimum rules.', 'Provider receive clock and venue event clock are different; no proof of local arrival chronology or PIT mapping.', 'Insufficient displayed depth is unavailable, not zero price or an imputed fill.']}
Path(__file__).with_name('summary.json').write_text(json.dumps(result, ensure_ascii=False, indent=2) + '\n')
print(json.dumps(result['all'], ensure_ascii=False))
