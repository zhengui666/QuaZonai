import json
import tempfile
from pathlib import Path

import prepare

root = prepare.ROOT
expected = {x['instrument_id'] + ':' + x['received_ns']: x for x in map(json.loads, (root / 'spot-valuation-samples-20261030/spot-valuation.jsonl').read_text().splitlines())}
samples = [
    ('LIQD', '2025-11-01', root / 'cryptohftdata-spot-samples-20261030/LIQD.parquet'),
    ('BUDDY', '2026-01-01', root / 'cryptohftdata-spot-samples-20261030/BUDDY.parquet'),
    ('IZEC', '2026-01-20', root / 'cryptohftdata-izec-samples-20261030/IZEC-2026-01-20.parquet'),
    ('IZEC', '2026-05-29', root / 'cryptohftdata-izec-samples-20261030/IZEC-2026-05-29.parquet'),
]
for asset, day, path in samples:
    quote, provenance = prepare.convert(asset, day, path)
    assert expected[quote['instrument_id'] + ':' + quote['received_ns']] == quote
    assert provenance['day'] == day
with tempfile.TemporaryDirectory(dir=prepare.HERE) as tmp:
    path = Path(tmp) / 'missing-ask.parquet'
    prepare.DB.execute("COPY (SELECT * FROM read_parquet(?) WHERE side = 'bid') TO ? (FORMAT PARQUET)", [str(samples[0][2]), str(path)])
    quote, _ = prepare.convert('LIQD', '2025-11-01', path)
    assert quote['bid'] is not None and quote['ask'] is None
    try:
        prepare.convert('BUDDY', '2025-11-01', samples[0][2])
        raise AssertionError('mismatched symbol accepted')
    except ValueError as exc:
        assert 'symbol mismatch' in str(exc)
print('four existing samples match; missing side and symbol mismatch handled')
