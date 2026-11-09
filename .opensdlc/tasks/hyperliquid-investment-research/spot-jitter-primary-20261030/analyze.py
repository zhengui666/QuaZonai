import bisect, collections, datetime as dt, json
from pathlib import Path
import duckdb

ROOT = Path(__file__).resolve().parent.parent
BATCH = ROOT / 'spot-valuation-batch-20261030'
SECOND = 1_000_000_000
HOUR = 3600 * SECOND
EARLY = SECOND + 100_000
LATE = SECOND + 1_500_000


def measure(rows, end):
    # shortcut: repeated identical messages sharing one receive/event clock are indistinguishable, upgrade if raw message identities become available.
    times = [x[0] for x in rows]
    assert times == sorted(set(times))
    result = dict(starts=len(rows), start_invalid=0, cross_hour=0, early_invalid=0,
                  late_invalid=0, eligible=0, no_update_between=0, bid_changed=0,
                  ask_changed=0, either_changed=0, mid_changed=0, delta_bps=[])
    for t, valid, _, _ in rows:
        if not valid:
            result['start_invalid'] += 1
            continue
        if t + LATE >= end:
            result['cross_hour'] += 1
            continue
        i = bisect.bisect_right(times, t + EARLY) - 1
        j = bisect.bisect_right(times, t + LATE) - 1
        early, late = rows[i], rows[j]
        if not early[1]:
            result['early_invalid'] += 1
        if not late[1]:
            result['late_invalid'] += 1
        if not early[1] or not late[1]:
            continue
        result['eligible'] += 1
        result['no_update_between'] += i == j
        bid_change, ask_change = early[2] != late[2], early[3] != late[3]
        result['bid_changed'] += bid_change
        result['ask_changed'] += ask_change
        result['either_changed'] += bid_change or ask_change
        mid1 = (early[2] + early[3]) / 2
        mid2 = (late[2] + late[3]) / 2
        result['mid_changed'] += mid1 != mid2
        if bid_change or ask_change:
            result['delta_bps'].append(abs(mid2 / mid1 - 1) * 10000)
    return result


# Edge cases: exact endpoint belongs to that endpoint; missing latest never falls back.
t = SECOND
r = measure([(0,True,1.,2.),(t+100_000,True,1.,2.),(t+500_000,True,1.1,2.),
             (t+1_500_000,True,1.2,2.)],2*SECOND)
assert r['eligible'] == 1 and r['cross_hour'] == 3 and r['either_changed'] == 1 and r['bid_changed'] == 1
r = measure([(0,True,1.,2.),(t+500_000,False,None,None)],2*SECOND)
assert r['late_invalid'] == 1 and r['eligible'] == 0 and r['start_invalid'] == 1


def combine(items):
    keys = ('starts','start_invalid','cross_hour','early_invalid','late_invalid','eligible',
            'no_update_between','bid_changed','ask_changed','either_changed','mid_changed')
    out = {k:sum(i[k] for i in items) for k in keys}
    for k in ('no_update_between','bid_changed','ask_changed','either_changed','mid_changed'):
        out[k+'_fraction'] = out[k]/out['eligible'] if out['eligible'] else None
    values = sorted(v for i in items for v in i['delta_bps'])
    def q(p):
        x = (len(values)-1)*p
        i = int(x)
        return values[i] + (values[min(i+1,len(values)-1)]-values[i])*(x-i)
    out['absolute_mid_delta_bps_when_either_changed'] = (
        dict(n=len(values), min=values[0], median=q(.5), p95=q(.95), max=values[-1]) if values else dict(n=0))
    return out

quotes = [json.loads(s) for s in (BATCH/'spot-valuation.jsonl').read_text().splitlines()]
provenance = json.loads((BATCH/'provenance.json').read_text())
assert len(quotes) == len(provenance) == 188
con = duckdb.connect(config={'threads':1, 'memory_limit':'256MB'})
per_asset = collections.defaultdict(list)
for quote, src in zip(quotes, provenance, strict=True):
    asset = src['currency']
    assert quote['instrument_id'] == f'{asset}-USDC-SPOT.HYPERLIQUID'
    end = int((dt.datetime.fromisoformat(src['day']).replace(tzinfo=dt.timezone.utc)+dt.timedelta(days=1)).timestamp())*SECOND
    observations = con.execute('''SELECT received_time, count(distinct symbol), min(symbol), count(distinct event_type), min(event_type), count(distinct event_time), count(*) FILTER (WHERE event_time IS NULL),
       count(*) FILTER (WHERE side IS NULL OR side NOT IN ('bid','ask') OR try_cast(price AS DOUBLE) IS NULL OR try_cast(quantity AS DOUBLE) IS NULL OR NOT isfinite(try_cast(price AS DOUBLE)) OR NOT isfinite(try_cast(quantity AS DOUBLE)) OR try_cast(price AS DOUBLE) <= 0 OR try_cast(quantity AS DOUBLE) <= 0),
       max(try_cast(price AS DOUBLE)) FILTER (WHERE side='bid'), min(try_cast(price AS DOUBLE)) FILTER (WHERE side='ask'),
       count(*) FILTER (WHERE side='bid'), count(*) FILTER (WHERE side='ask'), count(distinct (side,try_cast(price AS DOUBLE)))
       FROM read_parquet(?) WHERE received_time >= ? AND received_time < ? GROUP BY received_time ORDER BY received_time''', [src['file'],end-HOUR,end]).fetchall()
    rows = []
    for t, nsym, sym, ntype, typ, nevent, null_event, bad, bid, ask, nbid, nask, distinct_levels in observations:
        valid = (nsym == 1 and sym in {src['raw_symbol'], f'{asset}_USDC'} and ntype == 1 and typ == 'snapshot' and nevent == 1 and null_event == 0 and bad == 0 and nbid > 0 and nask > 0 and distinct_levels == nbid+nask and bid <= ask)
        rows.append((int(t), valid, float(bid) if valid else None, float(ask) if valid else None))
    assert rows and rows[-1][0] == int(quote['received_ns'])
    per_asset[asset].append(measure(rows, end))

result = {'source':str(BATCH), 'method':'Within each 23 UTC hour, compare latest provider-received snapshot at t+1s+0.1ms and t+1s+1.5ms from each valid snapshot. Invalid latest observations remain invalid. No cross-hour lookup. Duplicate side/price rows or multiple event times in a receive group are invalid. Identical repeated messages at the same event time may be indistinguishable.',
          'early_offset_ns':EARLY, 'late_offset_ns':LATE,
          'all':combine([v for items in per_asset.values() for v in items]),
          'per_asset':{k:combine(v) for k,v in sorted(per_asset.items())},
          'limitations':['Nonrandom first-missing-valuation-assets 23 UTC sample, not representative of market.', 'Source documents missing empty books, so even a recent observed quote may be stale.', 'Provider clock is not local or exchange-node receive time; offsets are assumptions, not measured network jitter.', 'Carried latest observation is not an executable quote or fill.', 'Static instrument mapping is not proven historical point in time.']}
Path(__file__).with_name('summary.json').write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n')
print(json.dumps(result['all'],ensure_ascii=False))
