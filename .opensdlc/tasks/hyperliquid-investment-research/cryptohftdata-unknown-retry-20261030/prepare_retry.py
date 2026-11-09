import json
import subprocess
import sys
import time
from pathlib import Path

import requests
import duckdb

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent / 'spot-valuation-batch-20261030'))
from prepare import convert, definition

items = [row for line in (HERE / 'retry-results.jsonl').read_text().splitlines()
         if (row := json.loads(line))['status'] == 'accessible_object']
assert len(items) == 10 and len({(r['day'], r['currency']) for r in items}) == 10
raw = HERE / 'raw'
raw.mkdir(exist_ok=False)
quotes = []
session = requests.Session()
with (HERE / 'prepare-results.jsonl').open('x') as log:
    for row in items:
        day, asset = row['day'], row['currency']
        result = {'day': day, 'currency': asset}
        compressed = raw / f'{day}-{asset}.parquet.zst'
        parquet = raw / f'{day}-{asset}.parquet'
        try:
            definition(asset)
            with session.get('https://api.cryptohftdata.com/v1/download', params={
                'file': f'hyperliquid_spot/{day}/23/{asset}_USDC_orderbook.parquet'
            }, timeout=30, stream=True) as response:
                result['http_status'] = response.status_code
                if response.status_code != 200:
                    raise ValueError('full file response unavailable')
                with compressed.open('xb') as target:
                    for chunk in response.iter_content(1024 * 1024):
                        target.write(chunk)
            with parquet.open('xb') as target:
                subprocess.run(['zstd', '-d', '-c', str(compressed)], stdout=target, check=True)
            quote, provenance = convert(asset, day, parquet)
            quotes.append({'quote': quote, 'provenance': provenance})
            result['status'] = 'prepared' if quote['bid'] is not None and quote['ask'] is not None else 'invalid_quote'
        except (requests.RequestException, OSError, ValueError, subprocess.SubprocessError, duckdb.Error) as error:
            result.update(status='error', error_type=type(error).__name__, error=str(error)[:200])
        log.write(json.dumps(result) + '\n')
        log.flush()
        time.sleep(1)
quotes.sort(key=lambda r: (r['quote']['instrument_id'], int(r['quote']['received_ns'])))
(HERE / 'spot-valuation.jsonl').write_text(''.join(json.dumps(row['quote']) + '\n' for row in quotes))
(HERE / 'provenance.json').write_text(json.dumps([row['provenance'] for row in quotes], ensure_ascii=False, indent=2) + '\n')
