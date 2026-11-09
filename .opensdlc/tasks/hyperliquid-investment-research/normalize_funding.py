"""Merge completed public funding pages into the native chronological input."""
import decimal
import heapq
import json
import pathlib
import sys
import urllib.parse

DATA = pathlib.Path('/home/zzy/.local/share/quazonai-ts/data')
RAW = DATA / 'hyperliquid-funding-20250728-20261013'
IDENTITY = DATA / 'imports/hyperliquid-identity-index-20261028/archive-instruments.json'


def event(row, coin, instrument, start, end):
    assert row['coin'] == coin and type(row['time']) is int
    assert start <= row['time'] < end
    rate = row['fundingRate']
    assert isinstance(rate, str) and decimal.Decimal(rate).is_finite()
    return dict(instrument_id=instrument, event_ns=str(row['time'] * 1_000_000), rate=rate)


def pages(coin, instrument, manifest, coverage):
    cursor, count = manifest['startTime'], 0
    end = manifest['endTime'] + 1
    directory = RAW / urllib.parse.quote(coin, safe='')
    for path in sorted(directory.glob('*.json')):
        record = json.loads(path.read_text())
        assert record['request'] == dict(type='fundingHistory', coin=coin,
                                         startTime=cursor, endTime=end - 1)
        rows = record['response']
        assert isinstance(rows, list)
        previous = cursor - 1
        for row in rows:
            result = event(row, coin, instrument, cursor, end)
            assert row['time'] > previous
            previous = row['time']
            count += 1
            yield result
        cursor = previous + 1 if rows else end
    assert cursor == end and count == coverage['rows']


def normalize(output):
    manifest = json.loads((RAW / 'manifest.json').read_text())
    identities = json.loads(IDENTITY.read_text())['symbol_ids']
    coins = manifest['coins']
    assert len(coins) == len(set(coins)) == 206
    coverage = {}
    for coin in coins:
        path = RAW / urllib.parse.quote(coin, safe='') / 'coverage.summary'
        state = json.loads(path.read_text())
        assert state['status'] in ('COMPLETED', 'EMPTY'), f'Incomplete funding: {coin}'
        assert state['cursor'] == manifest['endTime'] + 1
        coverage[coin] = state
    # A new directory retains interrupted outputs and never replaces published inputs.
    output.mkdir(parents=True, exist_ok=False)
    streams = [pages(coin, identities[coin], manifest, coverage[coin]) for coin in coins]
    count, previous = 0, None
    with (output / 'funding-rates.jsonl').open('x') as target:
        for row in heapq.merge(*streams, key=lambda row: (int(row['event_ns']), row['instrument_id'])):
            key = int(row['event_ns']), row['instrument_id']
            assert previous is None or previous < key
            previous = key
            target.write(json.dumps(row, separators=(',', ':')) + '\n')
            count += 1
    assert count == sum(state['rows'] for state in coverage.values())
    result = dict(source_revision=manifest['sourceRevision'],
                  start_ns=str(manifest['startTime'] * 1_000_000),
                  end_ns=str((manifest['endTime'] + 1) * 1_000_000),
                  instruments=sorted(identities[coin] for coin in coins), rows=count,
                  missing_instruments=sorted(identities[coin] for coin in coins if not coverage[coin]['rows']),
                  raw_source_ref=str(RAW), instrument_coverage={identities[c]: coverage[c] for c in coins},
                  limitations=manifest['limitations'])
    (output / 'funding-rates.manifest.json').write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps(dict(output=str(output), rows=count, missing_instruments=result['missing_instruments'])))


if __name__ == '__main__':
    normalize(pathlib.Path(sys.argv[1]))
