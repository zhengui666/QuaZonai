import {SpotBalanceObservation} from './spot-balance-observation';
import AccountOrders from './account-orders';
import { useRef, useState } from 'react';
import { Button, Input } from 'antd';
import type { ObservationView } from '../../../packages/contracts/src/index.ts';
import type { NativeAccountStrategyStatus, NativeAccountStrategyConfig } from '../../../packages/contracts/src/account-strategy.ts';
import { api } from './api';
import { useResource } from './data-hooks';
import { readDraft, saveDraft, useCommand } from './commands';
import { ErrorNotice, Facts, PendingCommand, Status, useConfirm } from './ui';

export interface StrategyReceipt {
  requestId: string; action: 'START' | 'STOP' | 'RECOVER'; status: 'PENDING' | 'UNCERTAIN' | 'COMPLETED' | 'REJECTED'; error: string | null;
  receipt: { persisted?: boolean; result?: Record<string, unknown> | null; persistenceError?: string } | null;
}
export function strategyResultText(value: StrategyReceipt) {
  if (value.action === 'RECOVER' && value.status === 'COMPLETED' && (value.receipt?.result?.accountObservation as {supported?:boolean}|undefined)?.supported === false) return '恢复连接已收到回执，但原绑定运行器不支持有界账户观察，需要明确安排升级，不能热接管旧节点。不代表订单已对账、账户可交易或策略已启动；保留原记录，不自动重绑。';
  if (value.status === 'REJECTED' && value.error === 'account_bound_runner_summary_upgrade_required') return '原绑定运行器不支持有界账户观察，本次未启动策略。需要明确安排升级，不能自动换绑或热接管旧节点；原订单、预留和 UNKNOWN 保留。';
  if (value.action === 'RECOVER') return value.status === 'COMPLETED'
    ? '已收到恢复连接回执；不代表订单已对账、账户可交易或策略已启动。不会自动撤单。'
    : value.status === 'REJECTED' ? '恢复连接请求被拒绝；请核对原回执，不自动更换版本重试。'
    : '恢复连接结果尚未确定；只查询原请求，不自动重连，也不把未知订单标为已结算。';
  if (value.receipt?.persisted === false) return value.receipt.result?.state === 'STOPPED'
    ? '原生已确认停止，但远端回执未能持久化；原命令仍标为不确定，不会自动重发。'
    : '远端回执未能持久化，且未确认停止；请核对当前策略状态，不会自动重发。';
  if (value.error === 'strategy_start_fence_changed') return '账户策略版本已变化，本次旧启动请求被拒绝。请先核对最新状态，再明确发起新的启动；不会自动换版本重试。';
  if (value.status === 'REJECTED' && value.error === 'hyperliquid_native_spot_free_required') return '实盘启动暂不可用：官方现货余额观察尚未与 Account、Portfolio 和 Risk 的一致性及新鲜度接通。本次未连接或启动策略；后续需完成官方余额与风控一致性验收，不会自动重试。';
  return value.status === 'COMPLETED' ? '原命令已收到回执；不代表订单已成交或资金已退出。'
    : value.status === 'REJECTED' ? '原生明确拒绝本次启动，未启动策略。'
    : '原命令尚未确定；这里只查询同一请求，不自动重发或用当前状态代替回执。';
}
interface AccountObservationCapability {supported?:boolean|null;reason?:string|null;nativeSessionId?:string|null}
export function accountSummaryStartAllowed(value?:AccountObservationCapability|null,unavailable=false) {
 return !unavailable && value?.supported === true && !!value.nativeSessionId;
}
export function AccountObservationNotice({value,unavailable=false}:{value?:AccountObservationCapability|null;unavailable?:boolean}) {
 const unsupported=!unavailable&&value?.supported===false&&!!value.nativeSessionId;
 return <section aria-label="有界账户观察能力"><h4>有界账户观察能力</h4><p>{unsupported
  ?'原运行器不支持有界账户观察，不能启动策略。'
  :accountSummaryStartAllowed(value,unavailable)?'最近报告：支持有界账户观察（非交易许可）。'
  :'有界账户观察能力未知。'}</p>
 </section>;
}
interface LegacyObservation {nativeSessionId:string;observedAtNs:string;receivedAt:string;observation:{cash:unknown;equity:unknown}}
export function LegacyAccountObservation({path,capability,unavailable=false}:{path:string;capability?:AccountObservationCapability|null;unavailable?:boolean}) {
 const confirm=useConfirm(),inFlight=useRef(false);
 const [busy,setBusy]=useState(false),[result,setResult]=useState<LegacyObservation>(),[error,setError]=useState<unknown>();
 const session=!unavailable&&capability?.supported===false?capability.nativeSessionId:null;
 const read=()=>{if(!session||inFlight.current)return;inFlight.current=true;setBusy(true);setError(undefined);setResult(undefined);
   void api<LegacyObservation>(`${path}/legacy-snapshot`,{method:'POST',body:{mode:'LEGACY_FULL_SNAPSHOT',expectedNativeSessionId:session,acknowledgeFullHistoryCost:true}})
    .then(setResult).catch(setError).finally(()=>{inFlight.current=false;setBusy(false);});};
 return <section aria-label="旧运行器单次全量观察"><h4>旧运行器单次全量观察</h4>
 <p>全历史读取可能阻塞原节点或耗尽内存。</p>
 <Button disabled={!session||busy} loading={busy} onClick={()=>confirm('读取一次旧版全历史快照？','原运行器可能因全部历史量过大而缓慢、超时或内存不足；可能影响同一节点响应。仅读取已连接的原会话，不启动策略；失败不自动重试。',read)}>明确读取一次旧版观察</Button>
 {!!error&&<p role="alert">本次观察未确认完成；没有自动重发，旧记录未清除。原运行器可能仍在处理请求，不应立即重复读取。</p>}
 {result&&<div><p>原会话：{result.nativeSessionId}；原观测纳秒：{result.observedAtNs}；接收时间：{result.receivedAt}。这是一次历史读数，不代表当前在线或可交易{result.nativeSessionId!==session?'，当前会话已变化或不可用':''}。</p>
 <p>原现金：{typeof result.observation.cash==='string'?result.observation.cash:'未知'}；原权益：{typeof result.observation.equity==='string'?result.observation.equity:'未知'}。</p></div>}
 </section>;
}
interface LifecycleCapability {supported?:boolean|null;configuredKind?:string|null;scope?:string|null;strategyVerified?:boolean|null;nativeSessionId?:string|null}
export function StrategyLifecycleNotice({value,unavailable=false}:{value?:LifecycleCapability|null;unavailable?:boolean}){
 const observed=!unavailable&&value?.scope==='INSTALLED_DISPATCH_ONLY'&&value.strategyVerified===false&&!!value.nativeSessionId&&typeof value.supported==='boolean';
 return <section aria-label="最近读取的原生策略接线报告"><h4>最近读取的原生策略接线报告</h4><p>{!observed?'原生策略接线状态未知。':value.supported?'最近报告：已安装策略派发入口。':'最近报告：未报告可用的策略派发入口。'}</p>
 {observed&&<p>原生报告类型：{value.configuredKind??'未提供'}；原会话：{value.nativeSessionId}。</p>}
 </section>;
}
/** Edit only known deployment strings; opaque owner configuration is not interpreted. */
export function strategyConfigurationField(text:string,field:'instrument_id'|'name',value:string){
 const parsed:unknown=JSON.parse(text);
 if(!parsed||typeof parsed!=='object'||Array.isArray(parsed))throw Error('原策略配置必须为对象');
 const config=parsed as Record<string,unknown>;
 if(field==='instrument_id')return JSON.stringify({...config,instrument_id:value},null,2);
 const strategy=config.strategy;
 if(!strategy||typeof strategy!=='object'||Array.isArray(strategy)||(strategy as Record<string,unknown>).kind!=='REGISTERED')throw Error('原策略不是 REGISTERED，不替换策略类型');
 return JSON.stringify({...config,strategy:{...strategy,name:value}},null,2);
}
export function StrategyConfigurationFields({text,disabled,onChange}:{text:string;disabled:boolean;onChange:(value:string)=>void}){
 let config:{instrument_id?:unknown;strategy?:{kind?:unknown;name?:unknown}};
 try{const value:unknown=JSON.parse(text);if(!value||typeof value!=='object'||Array.isArray(value))return <p>请修正高级配置 JSON 对象。</p>;config=value;}catch{return <p>请修正高级配置 JSON 对象。</p>;}
 return <fieldset disabled={disabled}><legend>原生策略配置</legend>
 <label>策略标的（必须沿用此账户已绑定标的）<Input aria-label="编辑策略标的" disabled={disabled} value={typeof config.instrument_id==='string'?config.instrument_id:''} onChange={event=>onChange(strategyConfigurationField(text,'instrument_id',event.target.value))}/></label>
 {config.strategy?.kind==='REGISTERED'&&<label>已编入运行器的原策略名称<Input aria-label="编辑原策略名称" disabled={disabled} value={typeof config.strategy.name==='string'?config.strategy.name:''} onChange={event=>onChange(strategyConfigurationField(text,'name',event.target.value))}/></label>}
 </fieldset>;
}
type StrategyEditIdentity={accountSourceId:string;strategyRevision:string};
export type StrategyEditDraft=StrategyEditIdentity&{text:string;accountKey:string};
export function strategyEditChanged(draft:StrategyEditDraft|undefined,current:StrategyEditIdentity|undefined,accountKey:string){
 return !!draft&&(!current||draft.accountKey!==accountKey||draft.accountSourceId!==current.accountSourceId||draft.strategyRevision!==current.strategyRevision);
}
export function strategyEditSubmission(draft:StrategyEditDraft|undefined,current:StrategyEditIdentity|undefined,accountKey:string,text:string){
 if(!current||strategyEditChanged(draft,current,accountKey))throw Error('原策略配置版本已变化；草稿保留，请核对后明确放弃草稿并读取最新配置。');
 const identity=draft??current;
 return {configuration:JSON.parse(text),expectedAccountSourceId:identity.accountSourceId,expectedStrategyRevision:identity.strategyRevision};
}
export default function AccountStrategy({ value, configured=false }: { value: Pick<ObservationView,'id'|'mode'|'connectionName'|'connectionId'>;configured?:boolean }) {
  const path = configured?`/accounts/configurations/${encodeURIComponent(value.id)}/strategy`:`/observations/${encodeURIComponent(value.id)}/strategy`;
  const commandIdentity=configured?`configured:${value.id}`:value.id;
  const status = useResource<{ accountSourceId: string; strategyRevision: string; strategy: NativeAccountStrategyStatus }>(path, { poll: 3000 });
  const fence = useResource<{ accountSourceId: string; strategyRevision: string }>(`${path}/fence`, { poll: 3000 });
  const configuration = useResource<{ accountSourceId: string; strategyRevision: string; configuration: NativeAccountStrategyConfig; connected: boolean | null; runnerValidated: boolean; lifecycleCapability?:LifecycleCapability|null; accountObservation?:AccountObservationCapability|null; pendingConfigurationRequests: string[] }>(`${path}/config`,{poll:3000});
  const [edited, setEdited] = useState<StrategyEditDraft>();
  const editConfiguration=(text:string)=>{if(configuration.data)setEdited(prior=>prior?{...prior,text}:{text,accountKey:commandIdentity,accountSourceId:configuration.data!.accountSourceId,strategyRevision:configuration.data!.strategyRevision});};
  const editChanged=strategyEditChanged(edited,configuration.data,commandIdentity);
  const [configurationError, setConfigurationError] = useState<unknown>();
  const [verifyingConfiguration, setVerifyingConfiguration] = useState(false);
  const [configurationReceipt, setConfigurationReceipt] = useState<{ status: string }>();
  const saveConfiguration = useCommand<{ status: string }>(`strategy:${commandIdentity}:CONFIGURE`, result => { setConfigurationReceipt(result); setEdited(undefined); void configuration.refetch(); void fence.refetch(); }, `${path}/config`);
  const configurationText = edited?.text ?? (configuration.data ? JSON.stringify(configuration.data.configuration, null, 2) : '');
  const key = `account-strategy:${commandIdentity}`;
  const [last, setLast] = useState(() => readDraft<string>(key));
  const confirm = useConfirm();
  const accepted = (result: StrategyReceipt) => { saveDraft(key, result.requestId); setLast(result.requestId); void status.refetch(); void fence.refetch(); };
  const start = useCommand<StrategyReceipt>(`strategy:${commandIdentity}:START`, accepted, `${path}/commands`);
  const stop = useCommand<StrategyReceipt>(`strategy:${commandIdentity}:STOP`, accepted, `${path}/commands`);
  const recover = useCommand<StrategyReceipt>(`strategy:${commandIdentity}:RECOVER`, accepted, `${path}/commands`);
  const requestId = (stop.pending ? stop.values?.requestId as string | undefined : start.pending ? start.values?.requestId as string | undefined : recover.pending ? recover.values?.requestId as string | undefined : undefined) ?? last;
  const receipt = useResource<StrategyReceipt>(`${path}/commands/${encodeURIComponent(requestId ?? '')}`, { enabled: !!requestId, poll: 2000 });
  const native = status.data?.strategy;
  const supported = !!native && typeof status.data?.accountSourceId === 'string' && /^\d+$/.test(status.data.strategyRevision);
  const unsettled = !!requestId && (!receipt.data || ['PENDING', 'UNCERTAIN'].includes(receipt.data.status));
  return <section className="result-section"><h3>Nautilus 原生策略</h3>
    <p>勿填写私钥或密码。</p>
    <AccountObservationNotice value={configuration.data?.accountObservation} unavailable={!!configuration.error}/>
    {configured&&<LegacyAccountObservation key={path} path={path} capability={configuration.data?.accountObservation} unavailable={!!configuration.error}/>}
    <StrategyLifecycleNotice value={configuration.data?.lifecycleCapability} unavailable={!!configuration.error}/>
    {configured&&value.mode==='live'&&<SpotBalanceObservation key={path} path={`/accounts/configurations/${encodeURIComponent(value.id)}`} sourceId={configuration.data?.accountSourceId} nativeSessionId={configuration.data?.accountObservation?.nativeSessionId} connected={!configuration.error&&configuration.data?.connected===true&&configuration.data?.accountObservation?.supported===true}/>}
    <details><summary>配置账户原生策略与参数（不连接、不交易）</summary>

      <ErrorNotice error={configuration.error ?? configurationError ?? saveConfiguration.error} retry={() => { void configuration.refetch(); }} />
      <StrategyConfigurationFields text={configurationText} onChange={editConfiguration} disabled={!configuration.data || configuration.data.connected !== false || !!saveConfiguration.pending}/>
      <details><summary>高级：完整原生策略配置</summary><p>精确小数请使用原策略要求的字符串格式。</p>
      <Input.TextArea aria-label="账户原生策略配置 JSON" autoSize={{ minRows: 8, maxRows: 24 }} value={configurationText} disabled={!configuration.data || configuration.data.connected !== false || !!saveConfiguration.pending} onChange={event => editConfiguration(event.target.value)} /></details>
      {editChanged&&<p role="alert">原账户或策略配置版本已变化。草稿保留，不能用新版本号覆盖原配置。请先核对，再明确放弃草稿采用最新配置。</p>}
      {edited&&<Button disabled={!!saveConfiguration.pending||saveConfiguration.busy} onClick={()=>confirm('放弃当前未保存草稿？','只放弃本地编辑并显示最近读取的原配置，不保存、不连接、不启动。',()=>{setEdited(undefined);setConfigurationError(undefined);void configuration.refetch();})}>放弃草稿，采用最新配置</Button>}
      <p>仅断开时可保存。</p>
      <Button disabled={!configuration.data || configuration.data.connected !== false || !!saveConfiguration.pending || editChanged} loading={saveConfiguration.busy} onClick={() => {
        try { const payload = strategyEditSubmission(edited,configuration.data,commandIdentity,configurationText); setConfigurationError(undefined); void saveConfiguration.submit(payload); }
        catch (error) { setConfigurationError(error); }
      }}>仅保存配置</Button>
      {(configuration.data?.pendingConfigurationRequests ?? []).map(requestId => <Button key={requestId} loading={verifyingConfiguration} disabled={verifyingConfiguration} onClick={() => confirm('核实原配置是否已持久保存？', `只核对原请求 ${requestId} 的文件与同步结果，不连接 Nautilus、不启动策略，也不解除任何订单 UNKNOWN。`, () => {
        setVerifyingConfiguration(true); void api<{status:string}>(`${path}/config/commands/${encodeURIComponent(requestId)}/verify`, {method:'POST',body:{}}).then(result=>{setConfigurationReceipt(result);void configuration.refetch();}).catch(setConfigurationError).finally(()=>setVerifyingConfiguration(false));
      })}>核实原配置保存结果：{requestId}</Button>)}
      <PendingCommand command={saveConfiguration} />
      {configurationReceipt && <p>{configurationReceipt.status === 'COMPLETED' ? '配置已保存，尚未连接或启动交易。' : '配置持久结果不确定；保留原请求，交易启动被阻止。请核对原配置回执，不要当作保存成功。'}</p>}
    </details>

    {value.mode === 'live' && <p role="status">实盘启动暂不可用：现货余额与风控尚未接通。</p>}
    <ErrorNotice error={status.error} retry={() => { void status.refetch(); }} />
    {native && <><Facts items={[["账户模式", value.mode === 'live' ? '实盘：可能产生真实订单' : '模拟盘'], ["策略", native.strategy_id ?? '未注册（仅恢复连接）'], ["状态", <Status value={native.state} />], ["标的", native.instrument_id], ["原生版本", native.native_version], ["策略控制修订", status.data!.strategyRevision]]} /><details><summary>账户原生策略完整配置</summary><pre>{JSON.stringify(native.config, null, 2)}</pre></details></>}
    <ErrorNotice error={fence.error} retry={() => { void fence.refetch(); }} />
    <div className="form-actions"><Button loading={recover.busy} disabled={!fence.data || !!fence.error || !!recover.pending} onClick={() => confirm('连接账户进行恢复？', '明确连接此账户的 Nautilus 恢复节点，不加载交易策略，不自动下单、改单或撤单。原订单、预留和 UNKNOWN 保留；连接成功不代表对账完成或可交易。', () => { void recover.submit({ action: 'RECOVER', expectedAccountSourceId: fence.data!.accountSourceId, expectedStrategyRevision: fence.data!.strategyRevision }); })}>连接账户进行恢复</Button><Button type="primary" loading={start.busy} disabled={value.mode === 'live' || !accountSummaryStartAllowed(configuration.data?.accountObservation,!!configuration.error) || !supported || unsettled || !!status.error || native?.state === 'RUNNING' || !!start.pending} onClick={() => confirm('启动原生策略？', `${value.mode === 'live' ? '这是实盘，会按已配置策略发送真实订单。' : '这是模拟盘。'}账户：${value.connectionName ?? value.connectionId ?? value.id}；策略：${native?.strategy_id}；标的：${native?.instrument_id}。不是仅启动观测。`, () => { void start.submit({ action: 'START', expectedAccountSourceId: status.data!.accountSourceId, expectedStrategyRevision: status.data!.strategyRevision }); })}>启动{value.mode === 'live' ? '实盘' : '模拟'}策略</Button>
    <Button danger loading={stop.busy} disabled={!!stop.pending} onClick={() => confirm('停止原生策略？', '向此账户发送官方策略 STOP；依上游配置可能撤单或平仓。停止本地观测不等于停止策略。', () => { void stop.submit({ action: 'STOP' }); })}>停止策略</Button>
    <Button onClick={() => { void status.refetch(); void fence.refetch(); void configuration.refetch(); if (requestId) void receipt.refetch(); }}>核对状态与原回执</Button></div>
    <PendingCommand command={recover} /><PendingCommand command={start} /><PendingCommand command={stop} /><ErrorNotice error={recover.error ?? start.error ?? stop.error ?? receipt.error} />
    {receipt.data && <div><h4>最近策略命令回执</h4><Facts items={[["请求", receipt.data.requestId], ["操作", receipt.data.action], ["回执", <Status value={receipt.data.status} />]]} /><p>{strategyResultText(receipt.data)}</p>{receipt.data.error && <code>{receipt.data.error}</code>}</div>}
    {configured&&<AccountOrders key={value.id} accountId={value.id}/>}
  </section>;
}
