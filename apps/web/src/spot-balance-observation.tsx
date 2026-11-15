import {useRef,useState} from 'react';
import {Button} from 'antd';
import {api} from './api';

type SpotObservation={accountSourceId?:string;nativeSessionId?:string;observation?:null|{request_started_ns?:unknown;received_ns?:unknown;balances?:unknown;error?:unknown}};
const text=(value:unknown)=>typeof value==='string'?value:'未知';
export function SpotBalanceFacts({value}:{value:SpotObservation}){
 const observed=value.observation;
 const rows=Array.isArray(observed?.balances)?observed.balances:null;
 return <div>
  {!observed?<p>余额未知：暂无官方观察。</p>:<>
   <p>原请求时间（纳秒）：{text(observed.request_started_ns)}；原接收时间（纳秒）：{text(observed.received_ns)}。</p>
   <p>历史观察（非当前可交易余额）</p>
   {observed.error!=null&&<p role="alert">官方余额读取失败，余额未知。</p>}
   {observed.error==null&&(rows===null?<p>余额未知。</p>:rows.length===0?<p>未返回币种，余额未知。</p>:<>
    <table><thead><tr><th>币种</th><th>原总额</th><th>原锁定额</th><th>原可用额（非交易许可）</th></tr></thead><tbody>{rows.slice(0,50).map((raw,index)=>{const row=raw&&typeof raw==='object'?raw as Record<string,unknown>:{};return <tr key={index}><td>{text(row.currency)}</td><td>{text(row.total)}</td><td>{text(row.locked)}</td><td>{text(row.free)}</td></tr>;})}</tbody></table>
    {rows.length>50&&<p>仅显示前 50 项。</p>}

   </>)}
  </>}
 </div>;
}
export function SpotBalanceObservation({path,sourceId,nativeSessionId,connected}:{path:string;sourceId?:string;nativeSessionId?:string|null;connected:boolean}){
 const inFlight=useRef(false);const [busy,setBusy]=useState(false),[result,setResult]=useState<SpotObservation>(),[error,setError]=useState(false);
 const read=()=>{if(!connected||!sourceId||!nativeSessionId||inFlight.current)return;inFlight.current=true;setBusy(true);setError(false);setResult(undefined);
  void api<SpotObservation>(`${path}/spot-balance-observation`).then(setResult).catch(()=>setError(true)).finally(()=>{inFlight.current=false;setBusy(false);});};
 return <section aria-label="官方现货余额只读观察"><h4>官方现货余额只读观察</h4>

  <Button disabled={!connected||!sourceId||!nativeSessionId||busy} loading={busy} onClick={read}>读取已保存的官方余额观察</Button>
  {!connected&&<p>原运行器未连接或不支持账户观察。</p>}
  {error&&<p role="alert">读取失败，余额未知。</p>}
  {result&&(result.accountSourceId!==sourceId||!nativeSessionId||result.nativeSessionId!==nativeSessionId||!connected?<p>账户或会话已变化，当前余额未知。</p>:<SpotBalanceFacts value={result}/>)}
 </section>;
}
