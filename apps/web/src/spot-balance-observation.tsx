import { useTranslation } from 'react-i18next';
import i18n from './i18n';
const t = (key: string, values?: Record<string, unknown>): string => i18n.t(key, { ns: 'settings', ...values });
import {useRef,useState} from 'react';
import {Button} from 'antd';
import {api} from './api';

type SpotObservation={accountSourceId?:string;nativeSessionId?:string;observation?:null|{request_started_ns?:unknown;received_ns?:unknown;balances?:unknown;error?:unknown}};
const text=(value:unknown)=>typeof value==='string'?value:t("Unknown");
export function SpotBalanceFacts({value}:{value:SpotObservation}){
 const { t } = useTranslation('settings');

 const observed=value.observation;
 const rows=Array.isArray(observed?.balances)?observed.balances:null;
 return <div>
  {!observed?<p>{t("Balance unknown: no official observation available.")}</p>:<>
   <p>{t("Original request time (ns):")}{text(observed.request_started_ns)}{t("; original receive time (ns):")}{text(observed.received_ns)}{t("Sentence end")}</p>
   <p>{t("Historical observation (not a current tradable balance)")}</p>
   {observed.error!=null&&<p role="alert">{t("Official balance read failed; balance is unknown.")}</p>}
   {observed.error==null&&(rows===null?<p>{t("Balance unknown.")}</p>:rows.length===0?<p>{t("No currencies returned; balance is unknown.")}</p>:<>
    <table><thead><tr><th>{t("Currency")}</th><th>{t("Original total")}</th><th>{t("Original locked amount")}</th><th>{t("Original available amount (not permission to trade)")}</th></tr></thead><tbody>{rows.slice(0,50).map((raw,index)=>{const row=raw&&typeof raw==='object'?raw as Record<string,unknown>:{};return <tr key={index}><td>{text(row.currency)}</td><td>{text(row.total)}</td><td>{text(row.locked)}</td><td>{text(row.free)}</td></tr>;})}</tbody></table>
    {rows.length>50&&<p>{t("Showing only the first 50 items.")}</p>}

   </>)}
  </>}
 </div>;
}
export function SpotBalanceObservation({path,sourceId,nativeSessionId,connected}:{path:string;sourceId?:string;nativeSessionId?:string|null;connected:boolean}){
 const { t } = useTranslation('settings');

 const inFlight=useRef(false);const [busy,setBusy]=useState(false),[result,setResult]=useState<SpotObservation>(),[error,setError]=useState(false);
 const read=()=>{if(!connected||!sourceId||!nativeSessionId||inFlight.current)return;inFlight.current=true;setBusy(true);setError(false);setResult(undefined);
  void api<SpotObservation>(`${path}/spot-balance-observation`).then(setResult).catch(()=>setError(true)).finally(()=>{inFlight.current=false;setBusy(false);});};
 return <section aria-label={t("Official spot balance observation (read-only)")}><h4>{t("Official spot balance observation (read-only)")}</h4>

  <Button disabled={!connected||!sourceId||!nativeSessionId||busy} loading={busy} onClick={read}>{t("Read saved official balance observation")}</Button>
  {!connected&&<p>{t("The original runner is disconnected or lacks account observation support.")}</p>}
  {error&&<p role="alert">{t("Read failed; balance is unknown.")}</p>}
  {result&&(result.accountSourceId!==sourceId||!nativeSessionId||result.nativeSessionId!==nativeSessionId||!connected?<p>{t("The account or session changed; current balance is unknown.")}</p>:<SpotBalanceFacts value={result}/>)}
 </section>;
}
