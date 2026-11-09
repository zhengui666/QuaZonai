import collections
import json
import math
import sqlite3
from pathlib import Path

DB = '/home/zzy/.local/share/quazonai-ts/data/research/research.sqlite'
TASKS = {'C03': 23721, 'B01': 24033}
OUT = Path(__file__).with_name('summary.json')

def quantile(xs, fraction):
    xs = sorted(xs)
    at = (len(xs) - 1) * fraction
    lo = int(at)
    hi = min(lo + 1, len(xs) - 1)
    return xs[lo] + (xs[hi] - xs[lo]) * (at - lo)

def describe(part):
    assets = part['assets']
    vals = []
    states = collections.Counter()
    counts = []
    for asset in assets:
        metric = asset['metric']
        n = int(metric['counts']['included'])
        counts.append(n)
        ic = metric['pearson_ic']
        states[ic['status']] += 1
        v = ic.get('value')
        if ic['status'] == 'AVAILABLE' and isinstance(v, (float, int)) and math.isfinite(v):
            vals.append((float(v), n))
    v = [v for v, n in vals]
    weighted_count = sum(n for _, n in vals)
    return {'assets_total': len(assets), 'ic_status_counts': dict(states),
        'assets_included_lt_100': sum(n < 100 for n in counts),
        'assets_included_lt_1000': sum(n < 1000 for n in counts),
        'ic_available': len(vals), 'ic_positive': sum(a > 0 for a in v),
        'ic_negative': sum(a < 0 for a in v), 'ic_zero': sum(a == 0 for a in v),
        'ic_p25': quantile(v, .25) if v else None, 'ic_median': quantile(v, .5) if v else None,
        'ic_p75': quantile(v, .75) if v else None,
        'included_count_weighted_mean_asset_ic': sum(v*n for v,n in vals)/weighted_count if weighted_count else None,
        'included_count_of_available_assets': weighted_count,
        'pooled_event_ic': part['metric']['pearson_ic'].get('value'),
        'pooled_included_count': int(part['metric']['counts']['included'])}

def read_primary(db, task):
    rows = db.execute('''SELECT j.value FROM tasks AS t,
        json_each(t.record, '$.state.outcome.result.partitions') AS j
        WHERE t.id = ? AND json_extract(j.value, '$.partition') IN ('TRAIN', 'VALIDATION')''', (task,)).fetchall()
    result = {p['partition']: p for (s,) in rows for p in [json.loads(s)]}
    if set(result) != {'TRAIN', 'VALIDATION'}:
        raise ValueError('primary partition missing')
    return result

def main():
    with sqlite3.connect('file:' + DB + '?mode=ro', uri=True) as db:
        tasks = {name: read_primary(db, id) for name,id in TASKS.items()}
    output = {'source': {'database': DB, 'C03': {'research_id':23690,'trial_id':23719,'task_id':23721},
              'B01': {'research_id':23993,'trial_id':24031,'task_id':24033},
              'selection':'SQL filters partition to TRAIN or VALIDATION before JSON result is returned'},
              'limitations': ['Identical per-asset included counts do not establish identical event keys or pairwise uncertainty.',
                  'Asset-weighted mean is descriptive and is not a pooled Pearson correlation.',
                  'Low-count thresholds are descriptive, not selection rules.',
                  'No IC or count establishes executable returns; event timing and PIT remain unverified.'], 'partitions': {}}
    for key in ('TRAIN', 'VALIDATION'):
        a,b = tasks['C03'][key],tasks['B01'][key]
        am = {x['instrument_id']: int(x['metric']['counts']['included']) for x in a['assets']}
        bm = {x['instrument_id']: int(x['metric']['counts']['included']) for x in b['assets']}
        if len(am) != len(a['assets']) or len(bm) != len(b['assets']):
            raise ValueError('duplicate instrument')
        av = {x['instrument_id']: x['metric']['pearson_ic']['value'] for x in a['assets'] if x['metric']['pearson_ic']['status']=='AVAILABLE'}
        bv = {x['instrument_id']: x['metric']['pearson_ic']['value'] for x in b['assets'] if x['metric']['pearson_ic']['status']=='AVAILABLE'}
        common_ic = av.keys() & bv.keys()
        delta = [float(bv[k])-float(av[k]) for k in common_ic]
        output['partitions'][key] = {'both_ic_available':len(delta), 'both_ic_median_b01_minus_c03':quantile(delta,.5) if delta else None, 'both_ic_b01_higher_count':sum(v>0 for v in delta), 'same_partition_boundaries':(a['start_ns'],a['end_ns'])==(b['start_ns'],b['end_ns']),
             'same_instruments':set(am)==set(bm),'equal_counts_for_common_instruments':sum(am[k]==bm[k] for k in am.keys() & bm.keys()),
             'common_instruments':len(am.keys() & bm.keys()), 'C03':describe(a),'B01':describe(b)}
    assert quantile([1,2,3],.5)==2 and quantile([1,2,3],.25)==1.5
    OUT.write_text(json.dumps(output,ensure_ascii=False,indent=2)+'\n')
    print(json.dumps(output,ensure_ascii=False))
if __name__ == '__main__': main()
