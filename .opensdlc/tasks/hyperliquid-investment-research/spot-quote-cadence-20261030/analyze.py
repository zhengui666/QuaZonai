import bisect, collections, datetime as dt, json, math
from pathlib import Path
import duckdb

ROOT = Path(__file__).resolve().parent.parent
BATCH = ROOT / 'spot-valuation-batch-20261030'
SECOND = 1_000_000_000
HOUR = 3600 * SECOND


def stats(values):
    values = sorted(values)
    if not values:
        return {'n': 0}
    def q(p):
        x = (len(values)-1)*p
        i = int(x)
        return values[i] + (values[min(i+1, len(values)-1)] - values[i])*(x-i)
    return {'n': len(values), 'min': values[0], 'p25': q(.25), 'median': q(.5), 'p75': q(.75), 'p95': q(.95), 'max': values[-1]}


def measure(snapshots, end):
    # Each row is (receive_ns, valid, bid, ask); invalid most-recent observations are not skipped.
    out = {'snapshots':len(snapshots), 'invalid':0, 'interval_ms':[], 'age_ms':[], 'eligible':0, 'cross_hour':0,
           'target_invalid':0, 'no_new_snapshot':0, 'bid_changed':0, 'ask_changed':0, 'either_changed':0}
    times = [x[0] for x in snapshots]
    assert times == sorted(set(times))
    for i, (t, valid, bid, ask) in enumerate(snapshots):
        if i and valid and snapshots[i-1][1]:
            out['interval_ms'].append((t-snapshots[i-1][0])/1e6)
        if not valid:
            out['invalid'] += 1
            continue
        target = t + SECOND
        if target >= end:
            out['cross_hour'] += 1
            continue
        j = bisect.bisect_right(times, target)-1
        later_t, later_valid, later_bid, later_ask = snapshots[j]
        if not later_valid:
            out['target_invalid'] += 1
            continue
        out['eligible'] += 1
        out['age_ms'].append((target-later_t)/1e6)
        out['no_new_snapshot'] += int(j == i)
        changed_bid = bid != later_bid
        changed_ask = ask != later_ask
        out['bid_changed'] += int(changed_bid)
        out['ask_changed'] += int(changed_ask)
        out['either_changed'] += int(changed_bid or changed_ask)
    return out


# An invalid latest quote is unavailable; exactly-on-horizon updates are visible.
test = measure([(0,True,1,2),(SECOND,False,None,None),(2*SECOND,True,1,3)], 4*SECOND)
assert (test['eligible'], test['invalid'], test['target_invalid'], test['no_new_snapshot']) == (1,1,1,1)
test = measure([(0,True,1,2),(SECOND,True,1,3)], 3*SECOND)
assert test['either_changed'] == 1 and test['ask_changed'] == 1 and test['no_new_snapshot'] == 1


def combine(items):
    scalar = ['snapshots','invalid','eligible','cross_hour','target_invalid','no_new_snapshot','bid_changed','ask_changed','either_changed']
    result = {k:sum(x[k] for x in items) for k in scalar}
    result['consecutive_valid_interval_ms'] = stats([v for x in items for v in x['interval_ms']])
    result['last_observation_age_at_1s_ms'] = stats([v for x in items for v in x['age_ms']])
    for k in ('no_new_snapshot','bid_changed','ask_changed','either_changed'):
        result[k+'_fraction'] = result[k]/result['eligible'] if result['eligible'] else None
    return result


quotes = [json.loads(s) for s in (BATCH/'spot-valuation.jsonl').read_text().splitlines()]
provenance = json.loads((BATCH/'provenance.json').read_text())
assert len(quotes) == len(provenance) == 188
con = duckdb.connect(config={'threads':1, 'memory_limit':'256MB'})
per_asset = collections.defaultdict(list)
for quote, src in zip(quotes, provenance, strict=True):
    asset = src['currency']
    assert quote['instrument_id'] == f'{asset}-USDC-SPOT.HYPERLIQUID'
    end = int((dt.datetime.fromisoformat(src['day']).replace(tzinfo=dt.timezone.utc)+dt.timedelta(days=1)).timestamp())*SECOND
    rows = con.execute('''SELECT received_time, count(distinct symbol), min(symbol), count(distinct event_type), min(event_type), count(distinct event_time), count(*) FILTER (WHERE event_time IS NULL),
       count(*) FILTER (WHERE side IS NULL OR side NOT IN ('bid','ask') OR try_cast(price AS DOUBLE) IS NULL OR try_cast(quantity AS DOUBLE) IS NULL OR NOT isfinite(try_cast(price AS DOUBLE)) OR NOT isfinite(try_cast(quantity AS DOUBLE)) OR try_cast(price AS DOUBLE) <= 0 OR try_cast(quantity AS DOUBLE) <= 0),
       max(try_cast(price AS DOUBLE)) FILTER (WHERE side='bid'), min(try_cast(price AS DOUBLE)) FILTER (WHERE side='ask'),
       count(*) FILTER (WHERE side='bid'), count(*) FILTER (WHERE side='ask'), count(distinct (side,try_cast(price AS DOUBLE)))
       FROM read_parquet(?) WHERE received_time >= ? AND received_time < ? GROUP BY received_time ORDER BY received_time''', [src['file'],end-HOUR,end]).fetchall()
    snapshots = []
    for t, nsymbol, symbol, ntype, typ, nevent, null_event, bad, bid, ask, nbid, nask, distinct_levels in rows:
        valid = (nsymbol == 1 and symbol in {src['raw_symbol'], f'{asset}_USDC'} and ntype == 1 and typ == 'snapshot' and nevent == 1 and null_event == 0 and bad == 0 and nbid > 0 and nask > 0 and distinct_levels == nbid+nask and bid <= ask)
        snapshots.append((int(t), valid, float(bid) if valid else None, float(ask) if valid else None))
    assert snapshots and snapshots[-1][0] == int(quote['received_ns'])
    per_asset[asset].append(measure(snapshots, end))

result = {'source':str(BATCH), 'method':'Within each 23 UTC hour, group book rows by provider received_time; a start snapshot and its most recent observation at or before start+1s must both be valid. The last observation is carried only for measurement, not treated as executable; no cross-hour lookups. Duplicate side/price rows or multiple event times in a receive group make it invalid; exact duplicate full messages at the same event time may be indistinguishable.',
          'all':combine([x for xs in per_asset.values() for x in xs]), 'per_asset':{k:combine(v) for k,v in sorted(per_asset.items())},
          'limitations':['Convenience sample of first missing valuation assets at 23 UTC, not random or representative.', 'Provider reports historical missing empty books: apparently recent snapshots can retain stale levels.', 'Provider receive time is not local node receive time; cannot validate network jitter or actual order fills.', 'Observation at or before the horizon is not a guarantee that the quote remained executable.', 'Static instrument mapping is not historical PIT proof.']}
Path(__file__).with_name('summary.json').write_text(json.dumps(result, ensure_ascii=False, indent=2)+'\n')
print(json.dumps(result['all'], ensure_ascii=False))
