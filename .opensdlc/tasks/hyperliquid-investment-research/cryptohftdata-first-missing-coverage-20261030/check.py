import datetime as dt
import json
import time
from pathlib import Path

import requests

HERE = Path(__file__).resolve().parent
SOURCE = HERE.parent / 'economic-primary-L01-small-capital-current.json'
OUT = HERE / 'observations.jsonl'
records = json.loads(SOURCE.read_text())['result']['days']
done = set()
if OUT.exists():
    done = {(r['day'], r['currency']) for r in map(json.loads, OUT.read_text().splitlines())}
session = requests.Session()
with OUT.open('a') as sink:
    for row in records:
        reason = row.get('unknown_reason') or ''
        if not reason.startswith('BALANCE_FX_UNAVAILABLE:'):
            continue
        parts = dict(part.split('=', 1) for part in reason.split(':')[1:] if '=' in part)
        day = dt.datetime.fromtimestamp(int(row['day_start_ns']) // 1_000_000_000, dt.timezone.utc).date().isoformat()
        currency = parts['currency']
        if (day, currency) in done:
            continue
        item = {'day': day, 'currency': currency, 'hour_utc': 23}
        if day < '2025-09-29':
            item['status'] = 'before_provider_start'
        else:
            try:
                with session.get('https://api.cryptohftdata.com/v1/download', params={
                    'file': f'hyperliquid_spot/{day}/23/{currency}_USDC_orderbook.parquet'
                }, headers={'Range': 'bytes=0-31'}, timeout=15, stream=True) as response:
                    item['http_status'] = response.status_code
                    item['status'] = 'accessible_object' if response.status_code in (200, 206) else 'not_found' if response.status_code == 404 else 'unknown'
            except requests.RequestException as exc:
                item.update(status='unknown', error_type=type(exc).__name__)
            time.sleep(1)
        sink.write(json.dumps(item) + '\n')
        sink.flush()
        print(json.dumps(item), flush=True)
