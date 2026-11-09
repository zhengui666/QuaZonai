"""Read-only event-time availability join; no prices, positions, models or returns."""
import array
import bisect
import collections
import datetime
import json
import pathlib
import sys
import time

import pyarrow as pa
import pyarrow.csv as csv

TASK = pathlib.Path(__file__).resolve().parent
DATA = pathlib.Path('/home/zzy/.local/share/quazonai-ts/data/catalogs')
CONTEXT = DATA / 'hyperliquid-context-source-20261028/attempt-0001'
SOURCE = DATA / 'hyperliquid-flow-prepared-20261028/attempt-0002/source/catalog'
PREPARED = DATA / 'hyperliquid-flow-prepared-20261028/attempt-0002/prepared/catalog-metadata.json'
HOUR = 3600_000_000_000
DAY = 86400_000_000_000


def preceding(events, at):
    index = bisect.bisect_right(events, at) - 1
    if index < 0:
        return None, 'NO_PRECEDING_CONTEXT'
    event = events[index]
    return event, None if at - event <= HOUR else 'CONTEXT_OLDER_THAN_ONE_HOUR'


def main():
    assert preceding([100, 200], 99) == (None, 'NO_PRECEDING_CONTEXT')
    assert preceding([100, 200], 100) == (100, None)
    assert preceding([100, 200], 199) == (100, None)
    assert preceding([100], 100 + HOUR) == (100, None)
    assert preceding([100], 101 + HOUR) == (100, 'CONTEXT_OLDER_THAN_ONE_HOUR')
    begin = time.monotonic()
    manifest = json.loads((CONTEXT / 'context-prices.manifest.json').read_text())
    funding = json.loads((SOURCE / 'funding-rates.manifest.json').read_text())
    boundary = int(json.loads(PREPARED.read_text())['request']['validation_end_ns'])
    start, end = int(manifest['start_ns']), int(manifest['end_ns'])
    assert manifest['research_source_revision'] == funding['source_revision']
    assert (start, end) == (int(funding['start_ns']), int(funding['end_ns']))
    rates = {name: array.array('q') for name in manifest['instrument_ids']}
    previous = None
    rows = 0
    with (SOURCE / 'funding-rates.jsonl').open() as source:
        for line in source:
            row = json.loads(line)
            event, name = int(row['event_ns']), row['instrument_id']
            assert start <= event < end
            key = event, name
            assert previous is None or previous < key
            previous = key
            rates[name].append(event)
            rows += 1
    assert rows == funding['rows'] == 2087913
    totals = {name: dict(events=0, available=0, no_preceding=0, stale=0) for name in ['primary', 'heldout']}
    by_day = {}
    by_coin = {}
    missing_day_coin = {}
    first_examples = []
    max_context_timestamps = 0
    context_rows = 0
    for index, entry in enumerate(manifest['files']):
        name = entry['instrument_id']
        events = array.array('q')
        with csv.open_csv(str(CONTEXT / entry['path']), convert_options=csv.ConvertOptions(
                include_columns=['event_ns'], column_types={'event_ns': pa.int64()})) as reader:
            for batch in reader:
                column = batch.column(0)
                assert column.null_count == 0
                offset = column.offset * 8
                raw = memoryview(column.buffers()[1])[offset:offset + len(column) * 8]
                values = array.array('q')
                values.frombytes(raw)
                if sys.byteorder != 'little':
                    values.byteswap()
                events.extend(values)
        assert len(events) == int(entry['rows'])
        assert events[0] == int(entry['first_event_ns']) and events[-1] == int(entry['last_event_ns'])
        max_context_timestamps = max(max_context_timestamps, len(events))
        context_rows += len(events)
        coin = dict(events=0, available=0, no_preceding=0, stale=0, examples=[])
        by_coin[name] = coin
        for event in rates[name]:
            partition = 'primary' if event < boundary else 'heldout'
            day_ns = event // DAY * DAY
            day = datetime.datetime.fromtimestamp(day_ns // 1_000_000_000, datetime.timezone.utc).date().isoformat()
            daily = by_day.setdefault(day, dict(partition=partition, events=0, available=0, no_preceding=0, stale=0))
            preceding_event, reason = preceding(events, event)
            classification = 'available' if reason is None else ('no_preceding' if preceding_event is None else 'stale')
            for counter in [totals[partition], daily, coin]:
                counter['events'] += 1
                counter[classification] += 1
            if reason is not None:
                key = day, name
                missing = missing_day_coin.setdefault(key, dict(day=day, instrument_id=name, partition=partition,
                                                                  events=0, no_preceding=0, stale=0, first_event_ns=str(event)))
                missing['events'] += 1
                missing[classification] += 1
                example = dict(instrument_id=name, partition=partition, funding_event_ns=str(event),
                               funding_event_utc=datetime.datetime.fromtimestamp(event // 1_000_000_000, datetime.timezone.utc).strftime('%Y-%m-%dT%H:%M:%S') + f'.{event % 1_000_000_000:09d}Z',
                               context_event_ns=None if preceding_event is None else str(preceding_event),
                               age_ns=None if preceding_event is None else str(event - preceding_event), reason=reason)
                if len(coin['examples']) < 3:
                    coin['examples'].append(example)
                first_examples.append(example)
        if (index + 1) % 50 == 0:
            print(json.dumps(dict(coins=index + 1, elapsedSeconds=round(time.monotonic() - begin, 3))), flush=True)
    assert context_rows == int(manifest['rows']) == 119131986
    assert sum(s['events'] for s in totals.values()) == rows
    result = dict(method='LATEST_CONTEXT_EVENT_LE_FUNDING_EVENT_WITH_ONE_HOUR_AGE', clock=manifest['clock'],
                  context_manifest=str(CONTEXT / 'context-prices.manifest.json'), funding_source=str(SOURCE / 'funding-rates.jsonl'),
                  source_revision=manifest['source_revision'], research_source_revision=manifest['research_source_revision'],
                  requested_start_ns=str(start), requested_end_ns=str(end), primary_end_ns=str(boundary),
                  maximum_age_ns=str(HOUR), totals=totals, days=dict(sorted(by_day.items())), coins=by_coin,
                  missing_day_coin=[missing_day_coin[k] for k in sorted(missing_day_coin)],
                  first_missing_examples=sorted(first_examples, key=lambda row: (int(row['funding_event_ns']), row['instrument_id']))[:20],
                  context_rows=context_rows, funding_events=rows, maximum_resident_context_timestamps=max_context_timestamps,
                  elapsedSeconds=round(time.monotonic() - begin, 3),
                  limitations=['EVENT_CLOCK_PROXY_NOT_RECEIVE_AVAILABILITY', 'SOURCE_AVAILABILITY_ONLY_NOT_OWN_POSITION_REQUIREMENT',
                               'NO_RETURNS_IC_EXECUTION_OR_PROFIT_QUALIFICATION', 'NO_FUTURE_FALLBACK_INTERPOLATION_OR_SCOPE_FILTER',
                               'LATEST_OBSERVATION_DOES_NOT_PROVE_EXACT_SETTLEMENT_ORACLE'])
    target = TASK / 'context-funding-asof-coverage.json'
    temporary = target.with_suffix('.tmp')
    temporary.write_text(json.dumps(result, indent=2) + '\n')
    temporary.replace(target)
    print(json.dumps(dict(output=str(target), totals=totals, first_missing_examples=result['first_missing_examples'][:3],
                          missing_day_coin=len(missing_day_coin), elapsedSeconds=result['elapsedSeconds'])), flush=True)


if __name__ == '__main__':
    main()
