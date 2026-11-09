import json,collections,statistics
from decimal import Decimal as D
from pathlib import Path
import duckdb
p=Path('/home/zzy/projects/QuaZonai/.opensdlc/tasks/hyperliquid-investment-research/spot-valuation-batch-20261030')
qs=[json.loads(x) for x in (p/'spot-valuation.jsonl').read_text().splitlines()]; ps=json.loads((p/'provenance.json').read_text()); assert len(qs)==len(ps)==188

def buy(levels,cash):
 q=D(0)
 for price,size in levels:
  take=min(cash,price*size);q+=take/price;cash-=take
  if cash<=0:return q
 return None

def sell(levels,q):
 cash=D(0)
 for price,size in levels:
  take=min(q,size);cash+=take*price;q-=take
  if q<=0:return cash
 return None
assert buy([(D(2),D(3)),(D(4),D(1))],D(10))==D(4)
assert sell([(D(2),D(3)),(D(1),D(1))],D(4))==D(7)
c=duckdb.connect(); groups=collections.defaultdict(list)
for q,s in zip(qs,ps,strict=True):
 rows=c.execute('select side, price, quantity from read_parquet(?) where received_time = ?',[s['file'],int(q['received_ns'])]).fetchall(); sides={'ask':[],'bid':[]}
 for side,price,size in rows:sides[side].append((D(str(price)),D(str(size))))
 sides['ask'].sort();sides['bid'].sort(reverse=True)
 assert str(sides['ask'][0][0])==q['ask'] and str(sides['bid'][0][0])==q['bid']
 for cash in (10,30):
  quantity=buy(sides['ask'],D(cash)); proceeds=sell(sides['bid'],quantity) if quantity is not None else None
  if proceeds is not None:groups[(s['currency'],cash)].append(float((D(cash)-proceeds)/D(cash)*10000))
  else:groups[(s['currency'],cash)].append(None)
for cash in (10,30):
 allvals=[v for (a,n),vs in groups.items() if n==cash for v in vs if v is not None]
 print('cash',cash,'n',len(allvals),'median_bps',statistics.median(allvals),'loss_positive',sum(v>0 for v in allvals),'p25',sorted(allvals)[len(allvals)//4],'p75',sorted(allvals)[3*len(allvals)//4])
 for a in ('IZEC','HFUN','HPENGU','LIQD'):
  vs=[v for v in groups[(a,cash)] if v is not None]; print(a,'n',len(vs),'median_bps',statistics.median(vs))
