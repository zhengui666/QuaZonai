"""Past-only BTC daily regimes for coverage planning; no factor or account returns."""
import collections
import datetime as dt
import decimal
import json
import pathlib
import time

import duckdb

TASK = pathlib.Path(__file__).resolve().parent
SOURCE = pathlib.Path('/home/zzy/.local/share/quazonai-ts/data/hyperliquid-trades-20250728-20261013')


def regime(closes, index):
    # UTC day index uses only completed days index-1 and index-31.
    if index < 31 or closes[index - 1] is None or closes[index - 31] is None:
        return 'UNKNOWN', None
    change = closes[index - 1] / closes[index - 31] - 1
    return ('BULL' if change > decimal.Decimal('.05') else
            'BEAR' if change < decimal.Decimal('-.05') else 'RANGE'), change


def main():
    unit = decimal.Decimal(1)
    assert regime([unit] * 32, 30) == ('UNKNOWN', None)
    assert regime([unit] * 32, 31) == ('RANGE', decimal.Decimal(0))
    prices = [unit] * 32
    prices[30] = decimal.Decimal('1.06')
    assert regime(prices, 31)[0] == 'BULL'
    prices[30] = decimal.Decimal('.94')
    assert regime(prices, 31)[0] == 'BEAR'
    manifest = json.loads((SOURCE / 'market-data-processing-market-candidates.json').read_text())
    profile = next(s for s in manifest['profile']['streams'] if s['kind'] == 'trades')
    assert profile['equals'] == {'crossed': True}
    assert profile['not_equals'] == {'liquidation_method': 'backstop'}
    paths = sorted(f['path'] for f in manifest['files']
                   if f.get('stream') == profile['name'] and f['path'].startswith('perp_fills_all/'))
    assert len(paths) == 443
    start = dt.date.fromisoformat(manifest['start_date'])
    end = dt.date.fromisoformat(manifest['end_date'])
    assert (end - start).days == len(paths)
    ledger = TASK / 'btc-daily-closes.jsonl'
    assert not ledger.exists(), 'Existing run must be inspected; do not overwrite.'
    db = duckdb.connect()
    db.execute("SET memory_limit='512MB'")
    db.execute('SET threads=2')
    closes, dates, details = [], [], []
    began = time.perf_counter()
    with decimal.localcontext() as context, ledger.open('x') as output:
        context.prec = 80
        for index, path in enumerate(paths):
            date = path.split('date=')[1].split('/')[0]
            assert date == (start + dt.timedelta(days=index)).isoformat()
            rows = db.execute('''
                SELECT epoch_ms(timestamp), price, size FROM read_parquet(?)
                WHERE coin='BTC' AND crossed=true
                  AND liquidation_method IS DISTINCT FROM 'backstop'
                  AND (direction IS NULL OR NOT list_contains(?, direction))
                QUALIFY timestamp=max(timestamp) OVER ()
            ''', [str(SOURCE / path), profile['not_in']['direction']]).fetchall()
            value, last = None, None
            if rows:
                last = rows[0][0]
                assert all(t == last and p > 0 and q > 0 for t, p, q in rows)
                assert dt.datetime.fromtimestamp(last / 1000, dt.timezone.utc).date().isoformat() == date
                value = sum((p * q for _, p, q in rows), decimal.Decimal(0)) / sum(
                    (q for _, _, q in rows), decimal.Decimal(0))
            closes.append(value)
            dates.append(date)
            state, change = regime(closes, index)
            row = dict(date=date, sourceFile=path, lastEventMs=last,
                       lastCohortRows=len(rows), lastCohortVwap=str(value) if value else None,
                       regimeAtUtcDayStart=state,
                       trailingCompleted30DayReturn=str(change) if change is not None else None)
            details.append(row)
            output.write(json.dumps(row) + '\n')
            output.flush()
            if (index + 1) % 20 == 0:
                print(json.dumps({'filesCompleted': index + 1, 'filesTotal': len(paths),
                                  'elapsedSeconds': round(time.perf_counter() - began, 3)}), flush=True)
    summary = dict(status='COMPLETED', sourceRevision=manifest['revision'],
                   definition='At UTC day D, BTC last complete event-cohort VWAP on D-1 / D-31 - 1; >+5% BULL, <-5% BEAR, otherwise RANGE.',
                   start=manifest['start_date'], endExclusive=manifest['end_date'],
                   days=len(dates), daysByRegime=dict(collections.Counter(r['regimeAtUtcDayStart'] for r in details)),
                   missingBtcDays=[r['date'] for r in details if r['lastCohortVwap'] is None],
                   elapsedSeconds=round(time.perf_counter() - began, 3), selfCheck='PASSED',
                   limitations=['Coverage planning only; not factor, strategy or account performance.',
                                'Original event clock is an unverified availability proxy.',
                                'Raw decimal BTC marks, not native instrument qualification or executable prices.',
                                'Initial 31 days lack the required past endpoint; no regime filled in.'])
    (TASK / 'daily-regime-summary.json').write_text(json.dumps(summary, indent=2) + '\n')
    print(json.dumps(summary), flush=True)


if __name__ == '__main__':
    main()
