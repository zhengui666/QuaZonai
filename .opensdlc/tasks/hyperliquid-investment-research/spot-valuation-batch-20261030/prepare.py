import datetime as dt
import glob
import json
import re
import subprocess
import time
from decimal import Decimal
from pathlib import Path

import duckdb
import requests

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent
OBS = ROOT / 'cryptohftdata-first-missing-coverage-20261030/observations.jsonl'
DEFS = Path('/home/zzy/.local/share/quazonai-ts/data/imports/hyperliquid-resource-scoped-definitions-unit-20261013-0010/native/catalog/data/instruments')
DB = duckdb.connect()


def definition(asset):
    if not re.fullmatch(r'[A-Za-z0-9]+', asset):
        raise ValueError('unsupported currency name')
    instrument = f'{asset}-USDC-SPOT.HYPERLIQUID'
    paths = glob.glob(str(DEFS / instrument / '*.parquet'))
    if len(paths) != 1:
        raise ValueError('ambiguous instrument definition')
    rows = DB.execute('select id, raw_symbol, base_currency, quote_currency, info from read_parquet(?)', [paths[0]]).fetchall()
    if len(rows) != 1:
        raise ValueError('ambiguous instrument row')
    ident, raw, base, quote, info = rows[0]
    if (ident, base, quote) != (instrument, asset, 'USDC') or json.loads(info).get('name') != raw:
        raise ValueError('instrument definition mismatch')
    return instrument, raw, paths[0]


def convert(asset, day, path):
    instrument, raw, definition_path = definition(asset)
    end_ns = int((dt.datetime.fromisoformat(day).replace(tzinfo=dt.timezone.utc) + dt.timedelta(days=1)).timestamp()) * 1_000_000_000
    start_ns = end_ns - 3600 * 1_000_000_000
    latest = DB.execute('select max(received_time) from read_parquet(?) where received_time >= ? and received_time < ?', [str(path), start_ns, end_ns]).fetchone()[0]
    if latest is None:
        raise ValueError('no snapshot received within target hour')
    book = DB.execute('select symbol, event_type, event_time, side, price, quantity from read_parquet(?) where received_time = ?', [str(path), latest]).fetchall()
    symbols = {r[0] for r in book}
    if len(symbols) != 1 or not symbols <= {raw, f'{asset}_USDC'}:
        raise ValueError('snapshot symbol mismatch')
    if {r[1] for r in book} != {'snapshot'} or len({r[2] for r in book}) != 1:
        raise ValueError('ambiguous snapshot')
    sides = {'bid': [], 'ask': []}
    for _, _, _, side, price, quantity in book:
        try:
            p, q = Decimal(str(price)), Decimal(str(quantity))
            if side not in sides or not p.is_finite() or not q.is_finite() or p <= 0 or q <= 0:
                raise ValueError('invalid level')
            sides[side].append(p)
        except (ArithmeticError, TypeError, ValueError):
            raise ValueError('invalid book level') from None
    bid = max(sides['bid']) if sides['bid'] else None
    ask = min(sides['ask']) if sides['ask'] else None
    if bid is not None and ask is not None and bid > ask:
        bid = ask = None
    quote = {'instrument_id': instrument, 'received_ns': str(latest), 'bid': str(bid) if bid is not None else None, 'ask': str(ask) if ask is not None else None}
    provenance = {'day': day, 'currency': asset, 'source_symbol': next(iter(symbols)), 'raw_symbol': raw, 'definition': definition_path, 'file': str(path), 'mapping': 'static 2026-10-28 definition; historical PIT not established'}
    return quote, provenance


def atomic_json(path, value):
    temporary = path.with_suffix(path.suffix + '.part')
    temporary.write_text(json.dumps(value, ensure_ascii=False) + '\n')
    temporary.replace(path)


def run():
    # Read only complete records because the coverage process can append concurrently.
    raw = OBS.read_bytes().splitlines(keepends=True)
    observations = [json.loads(line) for line in raw if line.endswith(b'\n')]
    candidates = {(o['day'], o['currency']) for o in observations if o['status'] == 'accessible_object' and o['hour_utc'] == 23}
    cache = HERE / 'raw'
    prepared = HERE / 'prepared'
    cache.mkdir(exist_ok=True)
    prepared.mkdir(exist_ok=True)
    session = requests.Session()
    journal = HERE / 'attempts.jsonl'
    with journal.open('a') as log:
        for day, asset in sorted(candidates):
            if not re.fullmatch(r'\d{4}-\d{2}-\d{2}', day) or not re.fullmatch(r'[A-Za-z0-9]+', asset):
                raise ValueError('invalid candidate identifier')
            dt.date.fromisoformat(day)
            key = f'{day}-{asset}'
            result = prepared / f'{key}.json'
            if result.exists():
                continue
            raw_path = cache / f'{key}.parquet.zst'
            parquet = cache / f'{key}.parquet'
            item = {'day': day, 'currency': asset}
            try:
                definition(asset)
                if not raw_path.exists():
                    with session.get('https://api.cryptohftdata.com/v1/download', params={'file': f'hyperliquid_spot/{day}/23/{asset}_USDC_orderbook.parquet'}, timeout=30, stream=True) as response:
                        item['http_status'] = response.status_code
                        if response.status_code != 200:
                            item['status'] = 'not_found' if response.status_code == 404 else 'download_unavailable'
                            log.write(json.dumps(item) + '\n'); log.flush()
                            continue
                        tmp = raw_path.with_suffix('.part')
                        with tmp.open('wb') as target:
                            for chunk in response.iter_content(1024 * 1024):
                                target.write(chunk)
                        tmp.replace(raw_path)
                if not parquet.exists():
                    tmp = parquet.with_suffix('.part')
                    with tmp.open('wb') as target:
                        subprocess.run(['zstd', '-d', '-c', str(raw_path)], stdout=target, check=True)
                    tmp.replace(parquet)
                quote, provenance = convert(asset, day, parquet)
                atomic_json(result, {'quote': quote, 'provenance': provenance})
                item['status'] = 'prepared'
            except (requests.RequestException, OSError, ValueError, subprocess.SubprocessError, duckdb.Error) as error:
                item.update(status='error', error_type=type(error).__name__, error=str(error)[:250])
            finally:
                time.sleep(1)
            log.write(json.dumps(item) + '\n'); log.flush()
    rows = [json.loads(p.read_text()) for p in prepared.glob('*.json')]
    rows.sort(key=lambda x: (x['quote']['instrument_id'], int(x['quote']['received_ns'])))
    seen = [(r['quote']['instrument_id'], r['quote']['received_ns']) for r in rows]
    if len(set(seen)) != len(seen):
        raise ValueError('duplicate instrument and receive time')
    output = HERE / 'spot-valuation.jsonl'
    tmp = output.with_suffix('.part')
    tmp.write_text(''.join(json.dumps(r['quote']) + '\n' for r in rows))
    tmp.replace(output)
    atomic_json(HERE / 'provenance.json', [r['provenance'] for r in rows])
    print(f'prepared {len(rows)} snapshots from {len(candidates)} accessible first-missing asset days')


if __name__ == '__main__':
    run()
