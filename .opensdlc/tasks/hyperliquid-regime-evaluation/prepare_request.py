"""Bind an existing factor request to the already computed BTC daily ledger."""
import collections
import datetime as dt
import decimal
import json
import pathlib
import sys

DAY_NS = 86_400_000_000_000


def state(previous, lookback):
    if previous is None or lookback is None:
        return 'UNKNOWN'
    with decimal.localcontext() as context:
        context.prec = 1024
        left = decimal.Decimal(previous['price']) * 100
        right = decimal.Decimal(lookback['price'])
        return 'BULL' if left > right * 105 else 'BEAR' if left < right * 95 else 'RANGE'


def mark(row):
    if row is None or row['lastCohortVwap'] is None:
        return None
    assert row['lastEventMs'] is not None
    price = row['lastCohortVwap']
    assert len(price) <= 512 and decimal.Decimal(price) > 0
    return dict(event_ns=str(row['lastEventMs'] * 1_000_000), price=price)


def self_check():
    def price(value):
        return dict(price=value)
    assert state(None, price('100')) == 'UNKNOWN'
    assert state(price('105'), price('100')) == 'RANGE'
    assert state(price('95'), price('100')) == 'RANGE'
    assert state(price('105.0000000000000000000000000001'), price('100')) == 'BULL'
    assert state(price('94.9999999999999999999999999999'), price('100')) == 'BEAR'


if __name__ == '__main__':
    self_check()
    if sys.argv[1:] == ['--self-check']:
        print('BTC endpoint comparison self-check passed')
        sys.exit(0)
    request_path, summary_path, ledger_path, output = map(pathlib.Path, sys.argv[1:])
    request = json.loads(request_path.read_text())
    summary = json.loads(summary_path.read_text())
    assert summary['status'] == 'COMPLETED'
    assert summary['sourceRevision'] == request['input']['source_revision']
    rows = [json.loads(line) for line in ledger_path.read_text().splitlines()]
    start = dt.date.fromisoformat(summary['start'])
    assert len(rows) == summary['days']
    assert (dt.date.fromisoformat(summary['endExclusive']) - start).days == len(rows)
    days, counts = [], collections.Counter()
    for index, row in enumerate(rows):
        date = start + dt.timedelta(days=index)
        assert row['date'] == date.isoformat()
        event = int(dt.datetime.combine(date, dt.time(), dt.timezone.utc).timestamp()) * 1_000_000_000
        previous = mark(rows[index-1]) if index >= 1 else None
        lookback = mark(rows[index-31]) if index >= 31 else None
        regime = state(previous, lookback)
        assert regime == row['regimeAtUtcDayStart'], 'daily classification differs from original ledger'
        if int(request['input']['event_start_ns']) <= event < int(request['input']['event_end_ns']):
            days.append(dict(day_start_ns=str(event), previous=previous, lookback=lookback))
            counts[regime] += 1
    assert days and int(days[0]['day_start_ns']) == int(request['input']['event_start_ns'])
    assert int(days[-1]['day_start_ns']) + DAY_NS == int(request['input']['event_end_ns'])
    request['evaluation_regimes'] = dict(kind='BTC_COMPLETED_30_DAY_RETURN_V1', days=days)
    with output.open('x') as handle:
        json.dump(request, handle, indent=2)
        handle.write('\n')
    print(json.dumps(dict(days=len(days), daysByRegime=counts, instruments=len(request['input']['instrument_ids']),
                         sourceRevision=request['input']['source_revision'], output=str(output))))
