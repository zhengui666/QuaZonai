import AccountOrders from './account-orders';
import { useState } from 'react';
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
  if (value.action === 'RECOVER') return value.status === 'COMPLETED'
    ? '已收到恢复连接回执；不代表订单已对账、账户可交易或策略已启动。不会自动撤单。'
    : value.status === 'REJECTED' ? '恢复连接请求被拒绝；请核对原回执，不自动更换版本重试。'
    : '恢复连接结果尚未确定；只查询原请求，不自动重连，也不把未知订单标为已结算。';
  if (value.receipt?.persisted === false) return value.receipt.result?.state === 'STOPPED'
    ? '原生已确认停止，但远端回执未能持久化；原命令仍标为不确定，不会自动重发。'
    : '远端回执未能持久化，且未确认停止；请核对当前策略状态，不会自动重发。';
  if (value.error === 'strategy_start_fence_changed') return '账户策略版本已变化，本次旧启动请求被拒绝。请先核对最新状态，再明确发起新的启动；不会自动换版本重试。';
  if (value.status === 'REJECTED' && value.error === 'hyperliquid_native_spot_free_required') return '实盘启动暂不可用：尚未接通可验证的 Nautilus 官方现货可用余额事实。本次未连接或启动策略；后续需完成官方余额与风控一致性验收，不会自动重试。';
  return value.status === 'COMPLETED' ? '原命令已收到回执；不代表订单已成交或资金已退出。'
    : value.status === 'REJECTED' ? '原生明确拒绝本次启动，未启动策略。'
    : '原命令尚未确定；这里只查询同一请求，不自动重发或用当前状态代替回执。';
}
interface LifecycleCapability {supported?:boolean|null;configuredKind?:string|null;scope?:string|null;strategyVerified?:boolean|null;nativeSessionId?:string|null}
export function StrategyLifecycleNotice({value,unavailable=false}:{value?:LifecycleCapability|null;unavailable?:boolean}){
 const observed=!unavailable&&value?.scope==='INSTALLED_DISPATCH_ONLY'&&value.strategyVerified===false&&!!value.nativeSessionId&&typeof value.supported==='boolean';
 return <section aria-label="最近读取的原生策略接线报告"><h4>最近读取的原生策略接线报告</h4><p>{!observed?'未知：尚未取得当前会话的 BOOT 能力事实，或本次读取不可用。保存的配置和声明不能代替实际观测。':value.supported?'最近一次读取到的原生会话报告已安装此策略的派发入口。':'最近一次读取到的原生会话未报告此策略的派发入口可用。'}</p>
 {observed&&<p>原生报告类型：{value.configuredKind??'未提供'}；原会话：{value.nativeSessionId}。</p>}
 <p>前台约每 3 秒读取一次，后台暂停轮询；缓存报告可能过时，不证明会话持续在线。这里只显示接线事实，不验证用户工厂、策略兼容性或交易结果。可信代码声明不是认证，也不是交易许可；查看不连接、不启动策略，不改变原命令准入。</p></section>;
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
 try{const value:unknown=JSON.parse(text);if(!value||typeof value!=='object'||Array.isArray(value))return <p>请先在高级配置中修正原对象，不生成替代配置。</p>;config=value;}catch{return <p>请先在高级配置中修正原对象，不生成替代配置。</p>;}
 return <fieldset disabled={disabled}><legend>原生策略配置</legend>
 <label>策略标的（必须沿用此账户已绑定标的）<Input aria-label="编辑策略标的" disabled={disabled} value={typeof config.instrument_id==='string'?config.instrument_id:''} onChange={event=>onChange(strategyConfigurationField(text,'instrument_id',event.target.value))}/></label>
 {config.strategy?.kind==='REGISTERED'&&<label>已编入运行器的原策略名称<Input aria-label="编辑原策略名称" disabled={disabled} value={typeof config.strategy.name==='string'?config.strategy.name:''} onChange={event=>onChange(strategyConfigurationField(text,'name',event.target.value))}/></label>}
 <p>只修改原合同中的文本字段。自定义 config 没有可用 schema，保留高级入口且不猜测参数；未编辑字段保留，十进制字符串不转换为数字。保存不连接或启动策略。</p></fieldset>;
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
  const configuration = useResource<{ accountSourceId: string; strategyRevision: string; configuration: NativeAccountStrategyConfig; connected: boolean | null; runnerValidated: boolean; lifecycleCapability?:LifecycleCapability|null; pendingConfigurationRequests: string[] }>(`${path}/config`,{poll:3000});
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
    <p>保存配置、连接恢复、启动交易是三个独立操作。REGISTERED 指向已编入账户运行器的原生 Rust 策略；名称与 config 原样提交，不表示运行器已支持。未知名称不能回退到示例策略。不要在参数中填写私钥或密码。</p>
    <StrategyLifecycleNotice value={configuration.data?.lifecycleCapability} unavailable={!!configuration.error}/>
    <details><summary>配置账户原生策略与参数（不连接、不交易）</summary>
      <p>保留此账户已绑定标的。自定义策略格式为 instrument_id 与 strategy: {`{kind: "REGISTERED", name: "已注册名称", config: 任意JSON}`}。新增 Rust 策略类型须先构建部署账户运行器；仅改 config 参数不需要重新编译。</p>
      <ErrorNotice error={configuration.error ?? configurationError ?? saveConfiguration.error} retry={() => { void configuration.refetch(); }} />
      <StrategyConfigurationFields text={configurationText} onChange={editConfiguration} disabled={!configuration.data || configuration.data.connected !== false || !!saveConfiguration.pending}/>
      <details><summary>高级：完整原生策略配置</summary><p>未知字段与自定义参数按原合同编辑；裸 JSON 数字不保证超出 JavaScript 精度范围的原始词法，精确小数请沿原策略要求使用字符串。</p>
      <Input.TextArea aria-label="账户原生策略配置 JSON" autoSize={{ minRows: 8, maxRows: 24 }} value={configurationText} disabled={!configuration.data || configuration.data.connected !== false || !!saveConfiguration.pending} onChange={event => editConfiguration(event.target.value)} /></details>
      {editChanged&&<p role="alert">原账户或策略配置版本已变化。草稿保留，不能用新版本号覆盖原配置。请先核对，再明确放弃草稿采用最新配置。</p>}
      {edited&&<Button disabled={!!saveConfiguration.pending||saveConfiguration.busy} onClick={()=>confirm('放弃当前未保存草稿？','只放弃本地编辑并显示最近读取的原配置，不保存、不连接、不启动。',()=>{setEdited(undefined);setConfigurationError(undefined);void configuration.refetch();})}>放弃草稿，采用最新配置</Button>}
      <p>仅断开状态允许保存；保存成功不代表参数已被原生运行器验证，更不会连接账户或发送订单。</p>
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
    <p>查看账户和重启服务不会自动连接。连接恢复不加载交易策略，不自动下单、改单或撤单；再次启动策略是独立操作。停止遵从上游配置，不保证撤销全部订单或清空持仓。资金退出请使用独立资金操作。</p>
    {value.mode === 'live' && <p role="status">实盘启动暂不可用：缺少已验证的官方现货余额事实接线。保存配置、明确恢复连接和停止仍是独立操作；不得用合并账户余额代替现货余额。</p>}
    <ErrorNotice error={status.error} retry={() => { void status.refetch(); }} />
    {native && <><Facts items={[["账户模式", value.mode === 'live' ? '实盘：可能产生真实订单' : '模拟盘'], ["策略", native.strategy_id ?? '未注册（仅恢复连接）'], ["状态", <Status value={native.state} />], ["标的", native.instrument_id], ["原生版本", native.native_version], ["策略控制修订", status.data!.strategyRevision]]} /><details><summary>账户原生策略完整配置</summary><pre>{JSON.stringify(native.config, null, 2)}</pre></details></>}
    <ErrorNotice error={fence.error} retry={() => { void fence.refetch(); }} />
    <div className="form-actions"><Button loading={recover.busy} disabled={!fence.data || !!fence.error || !!recover.pending} onClick={() => confirm('连接账户进行恢复？', '明确连接此账户的 Nautilus 恢复节点，不加载交易策略，不自动下单、改单或撤单。原订单、预留和 UNKNOWN 保留；连接成功不代表对账完成或可交易。', () => { void recover.submit({ action: 'RECOVER', expectedAccountSourceId: fence.data!.accountSourceId, expectedStrategyRevision: fence.data!.strategyRevision }); })}>连接账户进行恢复</Button><Button type="primary" loading={start.busy} disabled={value.mode === 'live' || !supported || unsettled || !!status.error || native?.state === 'RUNNING' || !!start.pending} onClick={() => confirm('启动原生策略？', `${value.mode === 'live' ? '这是实盘，会按已配置策略发送真实订单。' : '这是模拟盘。'}账户：${value.connectionName ?? value.connectionId ?? value.id}；策略：${native?.strategy_id}；标的：${native?.instrument_id}。不是仅启动观测。`, () => { void start.submit({ action: 'START', expectedAccountSourceId: status.data!.accountSourceId, expectedStrategyRevision: status.data!.strategyRevision }); })}>启动{value.mode === 'live' ? '实盘' : '模拟'}策略</Button>
    <Button danger loading={stop.busy} disabled={!!stop.pending} onClick={() => confirm('停止原生策略？', '向此账户发送官方策略 STOP；依上游配置可能撤单或平仓。停止本地观测不等于停止策略。', () => { void stop.submit({ action: 'STOP' }); })}>停止策略</Button>
    <Button onClick={() => { void status.refetch(); void fence.refetch(); if (requestId) void receipt.refetch(); }}>核对状态与原回执</Button></div>
    <PendingCommand command={recover} /><PendingCommand command={start} /><PendingCommand command={stop} /><ErrorNotice error={recover.error ?? start.error ?? stop.error ?? receipt.error} />
    {receipt.data && <div><h4>最近策略命令回执</h4><Facts items={[["请求", receipt.data.requestId], ["操作", receipt.data.action], ["回执", <Status value={receipt.data.status} />]]} /><p>{strategyResultText(receipt.data)}</p>{receipt.data.error && <code>{receipt.data.error}</code>}</div>}
    {configured&&<AccountOrders key={value.id} accountId={value.id}/>}
  </section>;
}
