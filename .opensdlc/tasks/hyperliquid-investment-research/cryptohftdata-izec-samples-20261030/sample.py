import datetime
import json
import subprocess
from pathlib import Path

import duckdb
import requests

directory = Path(__file__).parent
out = []
session = requests.Session()
db = duckdb.connect()
for day in ('2026-01-20', '2026-05-29'):
    item = {'asset': 'IZEC', 'day': day, 'hour': 23}
    try:
        response = session.get('https://api.cryptohftdata.com/v1/download', params={
            'file': f'hyperliquid_spot/{day}/23/IZEC_USDC_orderbook.parquet'}, timeout=30)
        item['http_status'] = response.status_code
        response.raise_for_status()
        path = directory / f'IZEC-{day}.parquet'
        path.write_bytes(subprocess.run(['zstd', '-d', '-c'], input=response.content,
                                        stdout=subprocess.PIPE, check=True).stdout)
        groups = db.execute('''SELECT received_time, event_time, any_value(symbol),
            count(*) FILTER (WHERE side='bid' AND try_cast(quantity AS double)>0) bids,
            count(*) FILTER (WHERE side='ask' AND try_cast(quantity AS double)>0) asks,
            max(try_cast(price AS double)) FILTER (WHERE side='bid' AND try_cast(quantity AS double)>0) bid,
            min(try_cast(price AS double)) FILTER (WHERE side='ask' AND try_cast(quantity AS double)>0) ask
            FROM read_parquet(?) GROUP BY 1,2 ORDER BY 1''', [str(path)]).fetchall()
        target = int((datetime.datetime.fromisoformat(day).replace(tzinfo=datetime.timezone.utc)
                      + datetime.timedelta(days=1)).timestamp() * 1e9) - 1
        valid = lambda g: bool(g[3] and g[4] and g[5] and g[6] and 0 < g[5] <= g[6])
        eligible = [g for g in groups if g[0] <= target]
        item.update(groups=len(groups), valid_groups=sum(map(valid, groups)))
        if eligible:
            received, event, symbol, bids, asks, bid, ask = eligible[-1]
            item['last_before_day_end'] = {'received_time_ns': received, 'event_time_ms': event,
                'symbol': symbol, 'bid_levels': bids, 'ask_levels': asks, 'best_bid': bid, 'best_ask': ask,
                'valid_two_sided': valid(eligible[-1]), 'age_seconds': (target - received) / 1e9}
    except requests.RequestException as exc:
        item['error_type'] = type(exc).__name__
    out.append(item)
    print(json.dumps(item), flush=True)
(directory / 'report.json').write_text(json.dumps(out, indent=2) + '\n')
