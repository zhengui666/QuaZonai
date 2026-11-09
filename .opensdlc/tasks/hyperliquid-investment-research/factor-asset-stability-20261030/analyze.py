import importlib.util
import json
import math
import sqlite3
import statistics
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location('compare', ROOT / 'factor-asset-compare-20261030' / 'compare.py')
compare = importlib.util.module_from_spec(spec)
spec.loader.exec_module(compare)

def pearson(xs, ys):
    if len(xs) < 2:
        return None
    xm, ym = statistics.mean(xs), statistics.mean(ys)
    xx = sum((x-xm)**2 for x in xs)
    yy = sum((y-ym)**2 for y in ys)
    return sum((x-xm)*(y-ym) for x,y in zip(xs,ys,strict=True)) / math.sqrt(xx*yy) if xx and yy else None

assert abs(pearson([1,2,3], [2,4,6]) - 1) < 1e-12
assert pearson([1,1], [2,3]) is None

def assets(part):
    return {a['instrument_id']: (a['metric']['pearson_ic'], int(a['metric']['counts']['included'])) for a in part['assets']}

def describe(train, validation, min_count):
    keys = train.keys() & validation.keys()
    pairs = []
    for key in keys:
        t, tn = train[key]
        v, vn = validation[key]
        if min(tn,vn) < min_count or t['status'] != 'AVAILABLE' or v['status'] != 'AVAILABLE':
            continue
        x, y = t.get('value'), v.get('value')
        if isinstance(x,(int,float)) and isinstance(y,(int,float)) and math.isfinite(x) and math.isfinite(y):
            pairs.append((x,y))
    nonzero = [(x,y) for x,y in pairs if x != 0 and y != 0]
    signs = {'train_positive_validation_positive':sum(x>0 and y>0 for x,y in nonzero),
             'train_positive_validation_negative':sum(x>0 and y<0 for x,y in nonzero),
             'train_negative_validation_positive':sum(x<0 and y>0 for x,y in nonzero),
             'train_negative_validation_negative':sum(x<0 and y<0 for x,y in nonzero)}
    return {'assets_with_ic_both_periods':len(pairs), 'nonzero_sign_pairs':len(nonzero), 'sign_counts':signs,
            'same_sign_count':signs['train_positive_validation_positive']+signs['train_negative_validation_negative'],
            'cross_asset_pearson_ic':pearson([x for x,y in pairs],[y for x,y in pairs]),
            'median_train_ic':statistics.median(x for x,y in pairs) if pairs else None,
            'median_validation_ic':statistics.median(y for x,y in pairs) if pairs else None}

with sqlite3.connect('file:' + compare.DB + '?mode=ro', uri=True) as db:
    data = {name:compare.read_primary(db,task) for name,task in compare.TASKS.items()}
output = {'method':'Pair each instrument TRAIN and VALIDATION per-asset Pearson IC. All available pairs and a descriptive >=1000 included events in each period subset; threshold mirrors prior sample-size report, not a trading rule.',
          'source':{'database':compare.DB,'tasks':compare.TASKS,'selection':'SQL filters partition to TRAIN or VALIDATION before JSON result is returned'},
          'limitations':['Per-asset IC estimates vary in precision and serial correlation makes event counts an unreliable effective sample size.', 'Cross-asset IC correlation and sign agreement are descriptive, do not prove statistical stability or executable returns.', 'Each filter changes the asset population and the >=1000 subset is not an ex-ante selection rule.', 'Event-time proxy and order execution remain unverified.'], 'factors':{}}
for name,parts in data.items():
    train,validation=assets(parts['TRAIN']),assets(parts['VALIDATION'])
    assert len(train)==len(parts['TRAIN']['assets']) and len(validation)==len(parts['VALIDATION']['assets'])
    output['factors'][name]={'all_available':describe(train,validation,0),'both_periods_at_least_1000_events':describe(train,validation,1000)}
Path(__file__).with_name('summary.json').write_text(json.dumps(output,indent=2)+'\n')
print(json.dumps(output['factors'],indent=2))
