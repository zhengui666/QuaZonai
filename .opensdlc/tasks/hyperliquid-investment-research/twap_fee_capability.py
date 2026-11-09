import json,pathlib,time,os,sys
import duckdb
root=pathlib.Path('/home/zzy/.local/share/quazonai-ts/data/hyperliquid-trades-20250728-20261013')
out=pathlib.Path('/home/zzy/projects/QuaZonai/.opensdlc/tasks/hyperliquid-investment-research')
inv=json.loads((out/'archive-inventory.json').read_text()); files=inv['files']
dates=sorted({x['path'].split('date=')[1].split('/')[0] for x in files}); sample={dates[0],dates[len(dates)//2],dates[-1]}
full='--full' in sys.argv
selected=files if full else [x for x in files if x['path'].split('date=')[1].split('/')[0] in sample]
manifest=json.loads((root/'market-data-processing-market-candidates.json').read_text()); excluded=next(s for s in manifest['profile']['streams'] if s['kind']=='trades')['not_in']['direction']
db=duckdb.connect();db.execute("SET memory_limit='512MB'");db.execute('SET threads=2')
assert db.execute('select count(*) filter(where x is not null),count(*) filter(where x!=0) from (values (NULL),(0),(7)) t(x)').fetchone()==(2,1)
result={'status':'RUNNING','pid':os.getpid(),'mode':'FULL' if full else 'SAMPLE','sampleDates':sorted(sample),'sourceRevision':inv['sourceRevision'],'filesTotal':len(selected),'filesCompleted':0,'selfCheck':'PASSED','groups':{},'files':[],'feeBpsDefinition':'quote token fee/(price*size)*10000; base token fee/size*10000; unmatched tokens excluded; builder_fee not added','limitations':['Historical field availability does not establish causal realtime publication.','No factor trials or execution-cost calibration.']}; start=time.monotonic()
path=out/('twap-fee-full.json' if full else 'twap-fee-sample.json')
def save():
 result['elapsedSeconds']=round(time.monotonic()-start,3);tmp=path.with_suffix('.tmp');tmp.write_text(json.dumps(result,indent=2)+'\n');tmp.replace(path)
save()
for item in selected:
 rows=db.execute('''WITH x AS (SELECT crossed,fee_token,fee,builder_fee,twap_id,
 crossed=true AND liquidation_method IS DISTINCT FROM 'backstop' AND (direction IS NULL OR NOT list_contains(?,direction)) AS candidate,
 CASE WHEN fee_token=quote_symbol AND price>0 AND size>0 THEN CAST(fee AS DOUBLE)/(CAST(price AS DOUBLE)*CAST(size AS DOUBLE))*10000 WHEN fee_token=base_symbol AND size>0 THEN CAST(fee AS DOUBLE)/CAST(size AS DOUBLE)*10000 END AS bps
 FROM read_parquet(?))
 SELECT candidate,crossed,fee_token,count(*),count(twap_id),count(*) FILTER(WHERE twap_id!=0),count(fee),count(*) FILTER(WHERE fee<0),count(*) FILTER(WHERE builder_fee!=0),count(bps),sum(bps),min(bps),max(bps) FROM x GROUP BY candidate,crossed,fee_token''',[excluded,str(root/item['path'])]).fetchall()
 stream=item['path'].split('/')[0];record={'file':item['path'],'rows':0,'twapNonnull':0,'twapNonzero':0}
 for r in rows:
  key=json.dumps([stream,*r[:3]],separators=(',',':'));g=result['groups'].setdefault(key,dict(stream=stream,candidate=r[0],crossed=r[1],feeToken=r[2],rows=0,twapNonnull=0,twapNonzero=0,feeNonnull=0,negativeFeeRows=0,builderFeeNonzero=0,bpsCount=0,bpsSum=0.,bpsMin=None,bpsMax=None))
  for name,value in zip(['rows','twapNonnull','twapNonzero','feeNonnull','negativeFeeRows','builderFeeNonzero','bpsCount','bpsSum'],r[3:11]):g[name]+=value or 0
  for name,value,fn in [('bpsMin',r[11],min),('bpsMax',r[12],max)]:
   if value is not None:g[name]=value if g[name] is None else fn(value,g[name])
  record['rows']+=r[3];record['twapNonnull']+=r[4];record['twapNonzero']+=r[5]
 result['files'].append(record);result['filesCompleted']+=1;save()
result['status']='COMPLETED';save()
print(json.dumps({k:result[k] for k in ['status','mode','sampleDates','filesCompleted','elapsedSeconds']}));print(json.dumps({'rows':sum(x['rows'] for x in result['files']),'twapNonnull':sum(x['twapNonnull'] for x in result['files']),'twapNonzero':sum(x['twapNonzero'] for x in result['files'])}))
