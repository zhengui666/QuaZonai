"""Normalize original context observations; never interpolate or replace funding rates."""
import datetime
import json
import os
import pathlib
import sys
import time

import pyarrow as pa
import pyarrow.compute as pc
import pyarrow.csv as csv

TASK = pathlib.Path(__file__).resolve().parent
DATA = pathlib.Path('/home/zzy/.local/share/quazonai-ts/data')
RAW = DATA / 'hyperliquid-asset-ctxs-20250728-20261013'
IDENTITY = DATA / 'imports/hyperliquid-identity-index-20261028/archive-instruments.json'
PROGRESS = TASK / 'context-normalization-progress.json'
COLUMNS = ['time', 'coin', 'oracle_px', 'mark_px']
SCHEMA = pa.schema([('event_ns', pa.int64()), ('oracle_px', pa.string()), ('mark_px', pa.string())])
PRICE = pa.decimal128(38, 18)


def save(path, value):
    temporary = path.with_suffix('.tmp')
    temporary.write_text(json.dumps(value, indent=2) + '\n')
    temporary.replace(path)


def observations(batch):
    table = pa.Table.from_batches([batch])
    for name in COLUMNS:
        assert table[name].type == pa.string() and table[name].null_count == 0, name
    # Validate with decimal arithmetic while writing the untouched original strings.
    for name in ['oracle_px', 'mark_px']:
        price = pc.cast(table[name], PRICE, safe=True)
        assert pc.all(pc.greater(price, pa.scalar(0, PRICE))).as_py(), name
    event = pc.cast(pc.strptime(table['time'], format='%Y-%m-%dT%H:%M:%SZ', unit='ns'), pa.int64())
    table = table.append_column('event_ns', event)
    return table.take(pc.sort_indices(table, sort_keys=[('coin', 'ascending'), ('event_ns', 'ascending')]))


def selfcheck():
    source = pa.BufferReader(b'time,coin,oracle_px,mark_px\n2025-07-29T06:00:00Z,VVV,3.0440,3.042500\n2025-07-29T06:01:00Z,VVV,1e-8,0.0000000200\n')
    reader = csv.open_csv(source, convert_options=csv.ConvertOptions(column_types={n: pa.string() for n in COLUMNS}))
    table = observations(next(reader)).select(SCHEMA.names)
    assert table['event_ns'].to_pylist() == [1753768800000000000, 1753768860000000000]
    target = pa.BufferOutputStream()
    target.write(b'event_ns,oracle_px,mark_px\n')
    with csv.CSVWriter(target, SCHEMA, write_options=csv.WriteOptions(include_header=False, quoting_style='none')) as writer:
        writer.write_table(table)
    assert target.getvalue().to_pybytes() == b'event_ns,oracle_px,mark_px\n1753768800000000000,3.0440,3.042500\n1753768860000000000,1e-8,0.0000000200\n'
    for value in ['0', '-1', 'NaN', 'Infinity']:
        bad = pa.record_batch([pa.array(['2025-07-29T06:00:00Z']), pa.array(['VVV']), pa.array([value]), pa.array(['1'])], names=COLUMNS)
        try:
            observations(bad)
        except (AssertionError, pa.ArrowInvalid):
            pass
        else:
            raise AssertionError('Invalid price accepted: ' + value)


def normalize(output):
    selfcheck()
    inventory = json.loads((RAW / 'manifest.json').read_text())
    coverage = json.loads((TASK / 'asset-context-actual-coverage.json').read_text())
    identity = json.loads(IDENTITY.read_text())['symbol_ids']
    coins = sorted(coverage['coinSummary'], key=lambda coin: identity[coin])
    assert len(coins) == len({identity[c] for c in coins}) == 206
    assert sum(coverage['coinSummary'][c]['rows'] for c in coins) == 119131986
    start = int(datetime.datetime(2025, 7, 28, tzinfo=datetime.timezone.utc).timestamp()) * 1_000_000_000
    end = int(datetime.datetime(2026, 10, 14, tzinfo=datetime.timezone.utc).timestamp()) * 1_000_000_000
    # No resume or replacement: a failed attempt remains available for inspection.
    output.mkdir(parents=True, exist_ok=False)
    state = dict(pid=os.getpid(), status='RUNNING', output=str(output), filesTotal=len(inventory['files']),
                 filesCompleted=0, rows=0, sourceBytesRead=0, startedAt=datetime.datetime.now(datetime.timezone.utc).isoformat())
    save(PROGRESS, state)
    writers = {}
    assets = {}
    begin = time.monotonic()
    selected = pa.array(coins, pa.string())
    try:
        for index, coin in enumerate(coins):
            name = f'context-prices-{index:03d}.csv'
            sink = (output / name).open('xb')
            sink.write(b'event_ns,oracle_px,mark_px\n')
            writers[coin] = (csv.CSVWriter(sink, SCHEMA, write_options=csv.WriteOptions(include_header=False, quoting_style='none')), sink)
            assets[coin] = dict(instrument_id=identity[coin], path=name, rows=0, first_event_ns=None,
                                last_event_ns=None, gaps_over_hour=[], partial_dates=coverage['coinSummary'][coin]['partialDays'])
        for item in inventory['files']:
            path = RAW / item['path']
            assert path.stat().st_size == item['size'], item['path']
            state['currentFile'] = item['path']
            with pa.input_stream(str(path), compression='lz4') as compressed:
                reader = csv.open_csv(compressed, read_options=csv.ReadOptions(block_size=4 * 1024 * 1024),
                                      convert_options=csv.ConvertOptions(include_columns=COLUMNS, column_types={n: pa.string() for n in COLUMNS}))
                for batch in reader:
                    keep = pc.is_in(batch.column(batch.schema.get_field_index('coin')), value_set=selected)
                    batch = batch.filter(keep)
                    if not batch.num_rows:
                        continue
                    table = observations(batch)
                    runs = pc.run_end_encode(table['coin'].combine_chunks())
                    offset = 0
                    for stop, coin in zip(runs.run_ends.to_pylist(), runs.values.to_pylist()):
                        rows = table.slice(offset, stop - offset).select(SCHEMA.names)
                        event = rows['event_ns'].combine_chunks()
                        first, last = event[0].as_py(), event[-1].as_py()
                        assert start <= first <= last < end
                        previous = assets[coin]['last_event_ns']
                        assert previous is None or int(previous) < first, (coin, previous, first)
                        gaps = pc.subtract(event.slice(1), event.slice(0, len(event) - 1))
                        if len(gaps):
                            assert pc.all(pc.greater(gaps, 0)).as_py(), coin
                        if previous is not None and first - int(previous) > 3600_000_000_000:
                            assets[coin]['gaps_over_hour'].append(dict(previous_event_ns=previous, event_ns=str(first), duration_ns=str(first - int(previous))))
                        locations = pc.indices_nonzero(pc.greater(gaps, 3600_000_000_000)).to_pylist()
                        for location in locations:
                            old, new = event[location].as_py(), event[location + 1].as_py()
                            assets[coin]['gaps_over_hour'].append(dict(previous_event_ns=str(old), event_ns=str(new), duration_ns=str(new - old)))
                        writers[coin][0].write_table(rows)
                        asset = assets[coin]
                        asset['first_event_ns'] = asset['first_event_ns'] or str(first)
                        asset['last_event_ns'] = str(last)
                        asset['rows'] += len(event)
                        state['rows'] += len(event)
                        offset = stop
            state['filesCompleted'] += 1
            state['sourceBytesRead'] += item['size']
            state['elapsedSeconds'] = round(time.monotonic() - begin, 3)
            save(PROGRESS, state)
        for writer, sink in writers.values():
            writer.close()
            sink.close()
        writers.clear()
        for coin, asset in assets.items():
            assert asset['rows'] == coverage['coinSummary'][coin]['rows'], coin
            asset['bytes'] = str((output / asset['path']).stat().st_size)
            asset['rows'] = str(asset['rows'])
        assert state['rows'] == 119131986
        result = dict(schema_version=1, format='EVENT_TIME_CONTEXT_PRICE_CSV_V1', clock='EVENT_TIME_PROXY_UNVERIFIED',
                      source_revision=inventory['revision'], research_source_revision=inventory['revision'],
                      start_ns=str(start), end_ns=str(end), rows=str(state['rows']),
                      instrument_ids=sorted(identity[c] for c in coins),
                      files=[{k: a[k] for k in ['instrument_id', 'path', 'bytes', 'rows', 'first_event_ns', 'last_event_ns']} for a in assets.values()],
                      partial_dates={a['instrument_id']: a['partial_dates'] for a in assets.values() if a['partial_dates']},
                      gaps_over_hour={a['instrument_id']: a['gaps_over_hour'] for a in assets.values() if a['gaps_over_hour']},
                      raw_source_ref=str(RAW), source_files=inventory['files'], missing_dates=['2026-07-08'],
                      source_bytes=state['sourceBytesRead'], bytes=sum(int(a['bytes']) for a in assets.values()),
                      limitations=['REQUESTED_ENVELOPE_NOT_CONTINUOUS_COVERAGE', 'NO_RECEIVE_OR_AVAILABILITY_CLOCK',
                                   'MINUTE_OBSERVATION_NOT_EXACT_SETTLEMENT_ORACLE', 'GAPS_RETAINED_NO_INTERPOLATION',
                                   'CONTEXT_FUNDING_NOT_USED_OR_SUBSTITUTED', 'RAW_DECIMAL_TEXT_PRESERVED'])
        # Completion manifest is published only after all output writers have closed successfully.
        with (output / 'context-prices.manifest.json').open('x') as target:
            target.write(json.dumps(result, indent=2) + '\n')
        state.update(status='COMPLETED', bytes=result['bytes'], manifest=str(output / 'context-prices.manifest.json'))
    except BaseException as error:
        state.update(status='STOPPED_ERROR', error=f'{type(error).__name__}: {error}')
        raise
    finally:
        for writer, sink in writers.values():
            writer.close()
            sink.close()
        state['elapsedSeconds'] = round(time.monotonic() - begin, 3)
        state['updatedAt'] = datetime.datetime.now(datetime.timezone.utc).isoformat()
        save(PROGRESS, state)
    print(json.dumps(state), flush=True)


if __name__ == '__main__':
    if sys.argv[1:] == ['--self-check']:
        selfcheck()
        print('parser/text-preservation self-check passed')
    else:
        normalize(pathlib.Path(sys.argv[1]))
