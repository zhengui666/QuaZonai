import {lazy,Suspense,useState} from 'react';
import {Alert,Button,Select,Table} from 'antd';
import {useTranslation} from 'react-i18next';
import {useResource} from './data-hooks';
import {ErrorNotice,Loading} from './ui';
import {nativeChartData} from './native-chart-data';
import type {ResearchNativeBacktests} from '../../../packages/contracts/src/index';
import type {NativeResultTablePage,NativeResultTableReference} from '../../../packages/contracts/src/native-result-tables';
const ResearchChart=lazy(()=>import('./research-chart'));
export function NativeEvidenceCell({value,present=true}:{value:unknown;present?:boolean}){const {t}=useTranslation('charts');return <code style={{whiteSpace:'pre-wrap',overflowWrap:'anywhere'}}>{!present?t('Field absent'):value===null?'null':typeof value==='object'?JSON.stringify(value):String(value)}</code>;}
function EvidencePage({researchId,taskId,path}:{researchId:number;taskId:number;path:string}){
 const {t,i18n}=useTranslation('charts');const [offset,setOffset]=useState(0);const [previous,setPrevious]=useState<number[]>([]);
 const query=useResource<NativeResultTablePage>(`/research/${researchId}/native-tables?taskId=${taskId}&path=${encodeURIComponent(path)}&offset=${offset}&pageSize=200`);
 const page=!query.error&&query.data?.taskId===taskId&&query.data.path===path&&query.data.offset===offset?query.data:undefined;const rows=page?.rows??[];const chart=nativeChartData(path,rows,offset);
 const number=(v:number)=>new Intl.NumberFormat(i18n.language).format(v);
 // Columns are the original page's keys, not a guessed financial schema.
 const keys=[...new Set(rows.flatMap(row=>row&&typeof row==='object'&&!Array.isArray(row)?Object.keys(row):['value']))];
 return <section><ErrorNotice error={query.error} retry={()=>void query.refetch()}/>{query.isPending?<Loading/>:page&&<>
  <p><code>{path}</code></p><p role="status">{t('Rows {{start}}–{{end}} of {{total}}. This is the loaded page, not the full history.',{start:number(page.totalRows===0?0:page.offset+1),end:number(page.endExclusive),total:number(page.totalRows)})}</p>
  {page.totalRows===0?<Alert type="info" title={t('The original table is empty. No trading or risk conclusion follows from an empty table.')}/>:<>
   {chart&&chart.series.some(s=>s.points.some(p=>p.y!==null))?<Suspense fallback={<Loading/>}><ResearchChart data={chart}/></Suspense>:<p>{t('No supported numeric series on this page. Original evidence remains in the table.')}</p>}
   <details open={!chart}><summary>{t('Original page values')}</summary><Table rowKey={(_,i)=>String(page.offset+(i??0))} size="small" pagination={false} scroll={{x:'max-content'}} dataSource={rows.map(row=>row&&typeof row==='object'&&!Array.isArray(row)?row as Record<string,unknown>:{value:row})} columns={keys.map(key=>({key,title:key,width:200,render:(_,row)=><NativeEvidenceCell value={row[key]} present={Object.hasOwn(row,key)}/>}))}/></details>
  </>}
  <div style={{display:'flex',gap:8,flexWrap:'wrap'}}><Button disabled={!previous.length} onClick={()=>{setOffset(previous.at(-1)!);setPrevious(previous.slice(0,-1));}}>{t('Previous page')}</Button><Button disabled={page.nextOffset===null} onClick={()=>{setPrevious([...previous,offset]);setOffset(page.nextOffset!);}}>{t('Next page')}</Button></div>
 </>}</section>;
}
function NativeTaskCharts({researchId,taskId}:{researchId:number;taskId:number}){
 const {t,i18n}=useTranslation('charts');const [offset,setOffset]=useState(0);const [previous,setPrevious]=useState<number[]>([]);const [selected,setSelected]=useState('');
 const query=useResource<NativeResultTablePage>(`/research/${researchId}/native-tables?taskId=${taskId}&offset=${offset}&pageSize=200`);
 const tables=(query.data?.rows??[]) as unknown as NativeResultTableReference[];
 const path=tables.some(row=>row.path===selected)?selected:tables.find(row=>row.name==='portfolio_snapshots')?.path??tables.find(row=>row.name==='equity_returns')?.path??tables[0]?.path;
 return <section className="result-card"><h4>{t('Charts and original evidence')}</h4><p>{t('Task {{taskId}}. Charts use original native values; they do not create new performance metrics.',{taskId})}</p><ErrorNotice error={query.error} retry={()=>void query.refetch()}/>{query.isPending?<Loading/>:query.data&&!query.error&&<>
  {!tables.length?<p>{t('No retained tables are available for this result.')}</p>:<><Select aria-label={t('Original result table')} style={{width:'100%',maxWidth:700}} value={path} options={tables.map(row=>({value:row.path,label:`${row.name} · ${new Intl.NumberFormat(i18n.language).format(row.rowCount)} · ${row.resultPath}`}))} onChange={setSelected}/>{path&&<EvidencePage key={`${taskId}:${path}`} researchId={researchId} taskId={taskId} path={path}/>}</>}
  {(previous.length>0||query.data.nextOffset!==null)&&<div style={{display:'flex',gap:8,flexWrap:'wrap'}}><Button disabled={!previous.length} onClick={()=>{setOffset(previous.at(-1)!);setPrevious(previous.slice(0,-1));}}>{t('Previous tables')}</Button><Button disabled={query.data.nextOffset===null} onClick={()=>{setPrevious([...previous,offset]);setOffset(query.data!.nextOffset!);}}>{t('More tables')}</Button></div>}
 </>}</section>;
}

/** The final result ID is the Pi tool, not the native child. Keep both identities. */
export default function NativeResultCharts({researchId,taskId}:{researchId:number;taskId:number}){
 const {t}=useTranslation('charts');const [cursor,setCursor]=useState<string>();const [previous,setPrevious]=useState<(string|undefined)[]>([]);const [selected,setSelected]=useState<number>();
 const query=useResource<ResearchNativeBacktests>(`/research/${researchId}/native-backtests${cursor?`?cursor=${encodeURIComponent(cursor)}`:''}`);
 const rows=query.data?.items??[];const matching=rows.filter(row=>row.ownerTaskId===taskId);
 const active=rows.find(row=>row.taskId===selected)??(matching.length===1&&matching[0]?.status==='completed'&&!cursor&&query.data?.nextCursor===null?matching[0]:undefined);
 return <section><h3>{t('Original native runs')}</h3><p>{t('Choose an original run. Failed and unfinished runs remain visible; a parameter case is not a selected winner.')}</p><ErrorNotice error={query.error} retry={()=>void query.refetch()}/>{query.isPending?<Loading/>:query.data&&!query.error&&<>
  <Select aria-label={t('Original native task')} style={{width:'100%',maxWidth:700}} placeholder={t('Choose an original run')} value={active?.taskId} options={rows.map(row=>({value:row.taskId,label:`#${row.taskId} · ${row.status} · ${row.ownerTaskId===taskId?t('This result request'):t('Historical request')}`}))} onChange={setSelected}/>
  {active?.status==='completed'?<NativeTaskCharts key={`${researchId}:${active.taskId}`} researchId={researchId} taskId={active.taskId}/>:active?<p>{t('This original task did not complete successfully. No result chart is inferred.')}</p>:null}
  <div style={{display:'flex',gap:8,flexWrap:'wrap'}}><Button disabled={!previous.length} onClick={()=>{setCursor(previous.at(-1));setPrevious(previous.slice(0,-1));setSelected(undefined);}}>{t('Previous runs')}</Button><Button disabled={query.data.nextCursor===null} onClick={()=>{setPrevious([...previous,cursor]);setCursor(query.data!.nextCursor!);setSelected(undefined);}}>{t('More runs')}</Button></div>
 </>}</section>;
}
