import datetime as dt
import glob
import json
from decimal import Decimal, InvalidOperation
from pathlib import Path

import duckdb

root = Path(__file__).resolve().parent.parent
definitions = Path('/home/zzy/.local/share/quazonai-ts/data/imports/hyperliquid-resource-scoped-definitions-unit-20261013-0010/native/catalog/data/instruments')
samples = [
    ('LIQD', '2025-11-01', root / 'cryptohftdata-spot-samples-20261030/LIQD.parquet'),
    ('BUDDY', '2026-01-01', root / 'cryptohftdata-spot-samples-20261030/BUDDY.parquet'),
    ('IZEC', '2026-01-20', root / 'cryptohftdata-izec-samples-20261030/IZEC-2026-01-20.parquet'),
    ('IZEC', '2026-05-29', root / 'cryptohftdata-izec-samples-20261030/IZEC-2026-05-29.parquet'),
]
db = duckdb.connect()
output, provenance = [], []
for asset, day, path in samples:
    instrument = f'{asset}-USDC-SPOT.HYPERLIQUID'
    paths = glob.glob(str(definitions / instrument / '*.parquet'))
    if len(paths) != 1:
        raise ValueError(f'{instrument}: ambiguous instrument definition')
    rows = db.execute('select id, raw_symbol, base_currency, quote_currency, info from read_parquet(?)', [paths[0]]).fetchall()
    if len(rows) != 1:
        raise ValueError(f'{instrument}: ambiguous instrument rows')
    ident, raw, base, quote, info = rows[0]
    if (ident, base, quote) != (instrument, asset, 'USDC'):
        raise ValueError(f'{instrument}: incompatible instrument definition')
    meta = json.loads(info)
    if meta.get('name') != raw:
        raise ValueError(f'{instrument}: inconsistent raw metadata')
    end = dt.datetime.fromisoformat(day).replace(tzinfo=dt.timezone.utc) + dt.timedelta(days=1)
    end_ns = int(end.timestamp()) * 1_000_000_000
    latest = db.execute('select max(received_time) from read_parquet(?) where received_time < ?', [str(path), end_ns]).fetchone()[0]
    if latest is None:
        raise ValueError(f'{instrument} {day}: no eligible snapshot')
    book = db.execute('select symbol, event_type, event_time, side, price, quantity from read_parquet(?) where received_time = ?', [str(path), latest]).fetchall()
    symbols = {r[0] for r in book}
    if len(symbols) != 1 or not symbols <= {raw, f'{base}_{quote}'}:
        raise ValueError(f'{instrument} {day}: symbol mismatch {symbols}')
    if {r[1] for r in book} != {'snapshot'} or len({r[2] for r in book}) != 1:
        raise ValueError(f'{instrument} {day}: ambiguous snapshot')
    sides = {'bid': [], 'ask': []}
    for _, _, _, side, price, quantity in book:
        try:
            p, q = Decimal(price), Decimal(quantity)
            if side not in sides or not p.is_finite() or not q.is_finite() or p <= 0 or q <= 0:
                raise ValueError('invalid level')
            sides[side].append(p)
        except (InvalidOperation, TypeError, ValueError):
            raise ValueError(f'{instrument} {day}: invalid book level')
    bid = max(sides['bid']) if sides['bid'] else None
    ask = min(sides['ask']) if sides['ask'] else None
    if bid is not None and ask is not None and bid > ask:
        bid = ask = None
    output.append({'instrument_id': instrument, 'received_ns': str(latest), 'bid': str(bid) if bid is not None else None, 'ask': str(ask) if ask is not None else None})
    provenance.append({'instrument_id': instrument, 'day_utc': day, 'sample': str(path), 'definition': paths[0], 'source_symbol': next(iter(symbols)), 'raw_symbol': raw, 'received_ns': str(latest), 'mapping': '2026-10-28 static instrument definition; historical PIT not established'})

output.sort(key=lambda x: (x['instrument_id'], int(x['received_ns'])))
if len(output) != 4 or len({(x['instrument_id'], x['received_ns']) for x in output}) != len(output):
    raise ValueError('unexpected sample count or duplicate timestamp')
(Path(__file__).parent / 'spot-valuation.jsonl').write_text(''.join(json.dumps(x) + '\n' for x in output))
(Path(__file__).parent / 'provenance.json').write_text(json.dumps(provenance, indent=2) + '\n')
print(f'converted {len(output)} snapshots; latest invalid or missing side remains unavailable')
