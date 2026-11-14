import {useRef,useState} from 'react';
import {Button} from 'antd';
import {api} from './api';

type SpotObservation={accountSourceId?:string;nativeSessionId?:string;observation?:null|{request_started_ns?:unknown;received_ns?:unknown;balances?:unknown;error?:unknown}};
const text=(value:unknown)=>typeof value==='string'?value:'未知';
export function SpotBalanceFacts({value}:{value:SpotObservation}){
 const observed=value.observation;
 const rows=Array.isArray(observed?.balances)?observed.balances:null;
 return <div>
  {!observed?<p>暂无官方现货余额观察；不等于余额为零。</p>:<>
   <p>原请求时间（纳秒）：{text(observed.request_started_ns)}；原接收时间（纳秒）：{text(observed.received_ns)}。</p>
   <p>这是异步历史观察，未核实当前新鲜度，没有交易所原子时点；不能作为当前可交易余额。</p>
   {observed.error!=null&&<p role="alert">官方余额读取失败，本次余额未知。请核对原账户诊断；不会补零或自动重试。</p>}
   {observed.error==null&&(rows===null?<p>余额未知。</p>:rows.length===0?<p>官方本次未返回币种；空结果或缺少币种不能视为零余额，也不会清除历史余额。</p>:<>
    <table><thead><tr><th>币种</th><th>原总额</th><th>原锁定额</th><th>原可用额（非交易许可）</th></tr></thead><tbody>{rows.slice(0,50).map((raw,index)=>{const row=raw&&typeof raw==='object'?raw as Record<string,unknown>:{};return <tr key={index}><td>{text(row.currency)}</td><td>{text(row.total)}</td><td>{text(row.locked)}</td><td>{text(row.free)}</td></tr>;})}</tbody></table>
    {rows.length>50&&<p>仅展示前50项原始余额，不代表完整币种清单。</p>}
    <p>未返回的币种不表示零余额；金额按官方原字符串展示，不换算或汇总。</p>
   </>)}
  </>}
 </div>;
}
export function SpotBalanceObservation({path,sourceId,nativeSessionId,connected}:{path:string;sourceId?:string;nativeSessionId?:string|null;connected:boolean}){
 const inFlight=useRef(false);const [busy,setBusy]=useState(false),[result,setResult]=useState<SpotObservation>(),[error,setError]=useState(false);
 const read=()=>{if(!connected||!sourceId||!nativeSessionId||inFlight.current)return;inFlight.current=true;setBusy(true);setError(false);setResult(undefined);
  void api<SpotObservation>(`${path}/spot-balance-observation`).then(setResult).catch(()=>setError(true)).finally(()=>{inFlight.current=false;setBusy(false);});};
 return <section aria-label="官方现货余额只读观察"><h4>官方现货余额只读观察</h4>
  <p>仅明确点击后读取已连接原运行器保存的观察，不发起新的交易所余额查询，不连接账户或启动策略。不会更新 Portfolio、资金预留或交易准入。</p>
  <Button disabled={!connected||!sourceId||!nativeSessionId||busy} loading={busy} onClick={read}>读取已保存的官方余额观察</Button>
  {!connected&&<p>原运行器尚未确认连接或不支持账户观察，不能读取；这里不会自动恢复连接。</p>}
  {error&&<p role="alert">本次读取失败，余额未知；没有自动重发，不沿用上次结果作为当前余额。</p>}
  {result&&(result.accountSourceId!==sourceId||!nativeSessionId||result.nativeSessionId!==nativeSessionId||!connected?<p>原账户来源或运行会话已变化，或连接状态未知；不展示旧观察为当前余额。</p>:<SpotBalanceFacts value={result}/>)}
 </section>;
}
