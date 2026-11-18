/** Display adapters for the pinned Nautilus canonical JSON, not new statistics.
 * Decimal strings and nanosecond clocks remain attached to every plotted point.
 * The caller supplies one explicitly labelled page, never a full-history claim. */
export type NativeChartPoint = { x: number; y: number | null; time: string; exact: string; row: number; note?: string };
export type NativeChartSeries = { name: string; unit: string; points: NativeChartPoint[] };
export type NativeChartData = { kind: 'line' | 'scatter' | 'bar'; title: string; series: NativeChartSeries[]; omitted: number; xKind: 'time' | 'row' };
const object = (v: unknown): Record<string, unknown> => v && typeof v === 'object' && !Array.isArray(v) ? v as Record<string, unknown> : {};
function decimal(v: unknown): { exact: string; value: number } | null {
 if (typeof v !== 'string' && typeof v !== 'number') return null;
 const exact=String(v);
 if (!/^-?\d+(?:\.\d+)?(?:[eE][+-]?\d+)?$/.test(exact)) return null;
 const value=Number(exact);return Number.isFinite(value)?{exact,value}:null;
}
function money(v: unknown): { exact: string; value: number; currency: string } | null {
 if(typeof v!=='string')return null;
 const match=/^(-?\d+(?:\.\d+)?) ([^\s]+)$/.exec(v);if(!match)return null;
 const amount=decimal(match[1]);return amount?{...amount,currency:match[2]!}:null;
}
export function nativeNanosecondTime(v: unknown): { coordinate:number; exact:string } | null {
 if(typeof v!=='string'||!/^\d+$/.test(v))return null;
 const ms=BigInt(v)/1_000_000n;
 if(ms>8_640_000_000_000_000n)return null;
 return {coordinate:Number(ms),exact:v};
}
export function nativeChartData(path:string, rows:readonly unknown[], offset=0):NativeChartData|null {
 const name=path.split('.').at(-1);const series=new Map<string,NativeChartSeries>();let omitted=0;
 const push=(group:string,unit:string,point:NativeChartPoint)=>{const key=JSON.stringify([group,unit]);let s=series.get(key);if(!s){s={name:group,unit,points:[]};series.set(key,s);}s.points.push(point);};
 if(name==='returns'&&path.endsWith('.equity_performance.returns')){
  rows.forEach((v,i)=>{const r=object(v),time=nativeNanosecondTime(r.ts_event_ns),value=decimal(r.value);if(!time){omitted++;return;}if(!value)omitted++;push('Official daily return','ratio',{x:time.coordinate,y:value?.value??null,time:time.exact,exact:value?.exact??'',row:offset+i});});
  return {kind:'line',title:'Official daily returns',xKind:'time',series:[...series.values()],omitted};
 }
 if(name==='portfolio_snapshots'){
  rows.forEach((v,i)=>{const r=object(v),time=nativeNanosecondTime(r.ts_event);if(!time||typeof r.account_id!=='string'){omitted++;return;}
   // Keep native per-currency equity separate. Never sum currencies or substitute balances.
   if(!Array.isArray(r.total_equity)||!r.total_equity.length){omitted++;return;}
   const unpriced=Array.isArray(r.unpriced_instruments)&&r.unpriced_instruments.length>0;
   r.total_equity.forEach(v=>{const m=money(v);if(!m){omitted++;return;}if(unpriced)omitted++;push(r.account_id as string,m.currency,{x:time.coordinate,y:unpriced?null:m.value,time:time.exact,exact:m.exact,row:offset+i,...(unpriced?{note:'Unpriced instruments; not plotted'}:r.is_stale===true?{note:'Carried or stale valuation'}:{})});});
  });
  return {kind:'line',title:'Native account equity',xKind:'time',series:[...series.values()],omitted};
 }
 if(name==='positions'||name==='position_snapshots'){
  rows.forEach((v,i)=>{const r=object(v),m=money(r.realized_pnl);if(!m){omitted++;return;}push(typeof r.instrument_id==='string'?r.instrument_id:'Unknown instrument',m.currency,{x:offset+i,y:m.value,time:typeof r.ts_closed==='string'?r.ts_closed:'',exact:m.exact,row:offset+i,note:typeof r.position_id==='string'?r.position_id:undefined});});
  return {kind:'bar',title:'Native realized PnL by position record',xKind:'row',series:[...series.values()],omitted};
 }
 if(name==='fills'){
  rows.forEach((v,i)=>{const r=object(v),event=object(object(r.event).Filled),time=nativeNanosecondTime(event.ts_event),price=decimal(event.last_px);
   if(!time||!price||typeof event.instrument_id!=='string'||typeof event.currency!=='string'){omitted++;return;}
   push(event.instrument_id,event.currency,{x:time.coordinate,y:price.value,time:time.exact,exact:price.exact,row:offset+i,note:typeof r.client_order_id==='string'?r.client_order_id:undefined});
  });
  return {kind:'scatter',title:'Native fill prices',xKind:'time',series:[...series.values()],omitted};
 }
 // Orders and diagnostics remain original tables until their exact
 // nested schema has an explicit adapter; guessing fields would mislabel evidence.
 return null;
}
