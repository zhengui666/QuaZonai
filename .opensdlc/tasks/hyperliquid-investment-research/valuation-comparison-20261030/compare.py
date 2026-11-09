import collections
import json
import sys
from pathlib import Path


def load(path):
    value = json.loads(Path(path).read_text())
    result = value.get('result', value)
    research = result['request']['research']
    end = int(research['validation_end_ns'])
    if int(research['input']['event_end_ns']) > end:
        raise ValueError('input includes final independent period')
    if any(int(day['day_start_ns']) >= end for day in result['days']):
        raise ValueError('day lies in final independent period')
    return result


def summary(result):
    days = result['days']
    reasons = collections.Counter(
        (day['unknown_reason'] or 'UNKNOWN').split(':', 1)[0]
        for day in days if day['equity_usdt'] is None
    )
    metrics = [
        {k: row[k] for k in ('partition', 'regime', 'days', 'valid_return_days')}
        for row in result['metrics'] if row['partition'] in ('TRAIN', 'VALIDATION')
    ]
    return {
        'days': len(days),
        'valued_days': sum(d['equity_usdt'] is not None for d in days),
        'valid_return_days': sum(d['net_return'] is not None for d in days),
        'unvalued_reasons': dict(sorted(reasons.items())),
        'partition_regime_coverage': metrics,
    }


def compare(original, candidate):
    a, b = load(original), load(candidate)
    a_request, b_request = dict(a['request']), dict(b['request'])
    a_request.pop('spot_valuation_source', None)
    b_request.pop('spot_valuation_source', None)
    if a_request != b_request:
        raise ValueError('requests differ beyond spot valuation source')
    # Aggregate checks do not prove fill-by-fill equality.
    fields = ('native_fills', 'native_rejections', 'fees_usdc_equivalent',
              'fees_by_currency', 'funding_usdc', 'small_capital_diagnostics',
              'targets', 'target_asof_ns', 'target_cash_weight', 'skipped_cash', 'skipped_minimum')
    return {
        'original': summary(a), 'spot_quote_valuation': summary(b),
        'aggregate_fields_equal': {k: a.get(k) == b.get(k) for k in fields},
        'note': 'Training and validation only. Missing days are not zero; aggregate equality does not prove fill-by-fill equality. No annualized return inferred.',
    }


if __name__ == '__main__':
    if len(sys.argv) != 3:
        raise SystemExit('usage: compare.py ORIGINAL_JSON COMPLETED_QUOTE_RESULT_JSON')
    try:
        print(json.dumps(compare(*sys.argv[1:]), ensure_ascii=False, indent=2))
    except (OSError, ValueError, KeyError, TypeError, json.JSONDecodeError) as error:
        raise SystemExit(f'comparison unavailable: {error}')
