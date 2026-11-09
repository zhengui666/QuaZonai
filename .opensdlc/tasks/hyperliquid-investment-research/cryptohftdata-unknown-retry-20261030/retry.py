import json
import time
from pathlib import Path

import requests

HERE = Path(__file__).resolve().parent
source = HERE.parent / 'cryptohftdata-first-missing-coverage-20261030/observations.jsonl'
items = [row for line in source.read_text().splitlines() if (row := json.loads(line))['status'] == 'unknown']
assert len(items) == 10 and len({(row['day'], row['currency']) for row in items}) == 10
session = requests.Session()
with (HERE / 'retry-results.jsonl').open('x') as out:
    for row in items:
        result = {key: row[key] for key in ('day', 'currency', 'hour_utc')}
        try:
            with session.get('https://api.cryptohftdata.com/v1/download', params={
                'file': f"hyperliquid_spot/{row['day']}/23/{row['currency']}_USDC_orderbook.parquet"
            }, headers={'Range': 'bytes=0-31'}, timeout=15, stream=True) as response:
                result['http_status'] = response.status_code
                result['status'] = ('accessible_object' if response.status_code in (200, 206)
                                    else 'not_found' if response.status_code == 404
                                    else 'rate_limited' if response.status_code == 429 else 'unknown')
        except requests.RequestException as exc:
            result.update(status='unknown', error_type=type(exc).__name__)
        out.write(json.dumps(result) + '\n')
        out.flush()
        time.sleep(1)
