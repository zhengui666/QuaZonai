import duckdb, json, datetime
c=duckdb.connect()
items=[('LIQD','2025-11-01'),('BUDDY','2026-01-01')]
result=[]
for asset,day in items:
 path=f'/tmp/qz-spot-samples/{asset}.parquet'
 groups=c.execute('''SELECT received_time,event_time,any_value(symbol),count(*) FILTER (WHERE side='bid' AND try_cast(quantity as double)>0) bids,count(*) FILTER (WHERE side='ask' AND try_cast(quantity as double)>0) asks,max(try_cast(price as double)) FILTER (WHERE side='bid' AND try_cast(quantity as double)>0) bid,min(try_cast(price as double)) FILTER (WHERE side='ask' AND try_cast(quantity as double)>0) ask FROM read_parquet(?) GROUP BY 1,2 ORDER BY 1''',[path]).fetchall()
 target=int((datetime.datetime.fromisoformat(day).replace(tzinfo=datetime.timezone.utc)+datetime.timedelta(days=1)).timestamp()*1e9)-1
 def view(g):
  rec,event,sym,nb,na,bid,ask=g
  return {'received_time_ns':rec,'received_at_utc':datetime.datetime.fromtimestamp(rec/1e9,datetime.timezone.utc).isoformat(),'event_time_ms':event,'event_at_utc':datetime.datetime.fromtimestamp(event/1000,datetime.timezone.utc).isoformat(),'symbol':sym,'bid_levels':nb,'ask_levels':na,'best_bid':bid,'best_ask':ask,'valid_two_sided':bool(nb and na and bid and ask and 0<bid<=ask),'age_before_day_end_seconds':(target-rec)/1e9}
 eligible=[g for g in groups if g[0]<=target]
 entry={'asset':asset,'day':day,'groups':len(groups),'groups_valid':sum(view(g)['valid_two_sided'] for g in groups),'first':view(groups[0]),'last_before_day_end':view(eligible[-1]) if eligible else None}; result.append(entry)
print(json.dumps(result,indent=2)); open('/tmp/qz-spot-samples/report.json','w').write(json.dumps(result,indent=2)+'\n')
