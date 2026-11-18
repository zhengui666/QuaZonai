import { useTranslation } from 'react-i18next';
import i18n from './i18n';
const t = (key: string, values?: Record<string, unknown>): string => i18n.t(key, { ns: 'settings', ...values });
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
  if (value.action === 'RECOVER' && value.status === 'COMPLETED' && (value.receipt?.result?.accountObservation as {supported?:boolean}|undefined)?.supported === false) return t("A reconnection receipt was received, but the bound runner lacks bounded account observation. An explicit upgrade is required; do not take over the old node in place. This is not proof of reconciled orders, trading readiness, or a started strategy. Keep the original records; no automatic rebinding occurs.");
  if (value.status === 'REJECTED' && value.error === 'account_bound_runner_summary_upgrade_required') return t("The bound runner lacks bounded account observation, so the strategy was not started. An explicit upgrade is required; no automatic rebinding or takeover of the old node occurs. Original orders, reservations, and UNKNOWN states remain.");
  if (value.action === 'RECOVER') return value.status === 'COMPLETED'
    ? t("A reconnection receipt was received. It does not prove reconciled orders, trading readiness, or a started strategy. No orders are automatically cancelled.")
    : value.status === 'REJECTED' ? t("Reconnection was rejected. Check the original receipt; no automatic retry with a different version occurs.")
    : t("The reconnection outcome is unknown. Query the original request only; do not reconnect automatically or mark unknown orders as settled.");
  if (value.receipt?.persisted === false) return value.receipt.result?.state === 'STOPPED'
    ? t("The native runtime confirmed a stop, but the remote receipt could not be persisted. The original command remains uncertain and will not be resent automatically.")
    : t("The remote receipt could not be persisted, and a stop is not confirmed. Check the current strategy status; no automatic resend occurs.");
  if (value.error === 'strategy_start_fence_changed') return t("The account strategy version changed, so this old start request was rejected. Check the latest status before explicitly starting a new request. No automatic version-switching retry occurs.");
  if (value.status === 'REJECTED' && value.error === 'hyperliquid_native_spot_free_required') return t("Live start is unavailable: official spot balance observation is not yet integrated with Account, Portfolio, and Risk consistency and freshness. No account connection or strategy start occurred. Official balance and risk consistency must be verified first; there is no automatic retry.");
  return value.status === 'COMPLETED' ? t("The original command received a receipt. This does not prove filled orders or withdrawn funds.")
    : value.status === 'REJECTED' ? t("The native runtime explicitly rejected this start; no strategy was started.")
    : t("The original command outcome is unknown. Only the same request is queried; no automatic resend or substitution with current status occurs.");
}
interface AccountObservationCapability {supported?:boolean|null;reason?:string|null;nativeSessionId?:string|null}
export function accountSummaryStartAllowed(value?:AccountObservationCapability|null,unavailable=false) {
 return !unavailable && value?.supported === true && !!value.nativeSessionId;
}
export function AccountObservationNotice({value,unavailable=false}:{value?:AccountObservationCapability|null;unavailable?:boolean}) {
 const { t } = useTranslation('settings');

 const unsupported=!unavailable&&value?.supported===false&&!!value.nativeSessionId;
 return <section aria-label={t("Bounded account observation")}><h4>{t("Bounded account observation")}</h4><p>{unsupported
  ?t("The original runner lacks bounded account observation; the strategy cannot start.")
  :accountSummaryStartAllowed(value,unavailable)?t("Latest report: bounded account observation is supported (not permission to trade).")
  :t("Bounded account observation support is unknown.")}</p>
 </section>;
}
interface LegacyObservation {nativeSessionId:string;observedAtNs:string;receivedAt:string;observation:{cash:unknown;equity:unknown}}
export function LegacyAccountObservation({path,capability,unavailable=false}:{path:string;capability?:AccountObservationCapability|null;unavailable?:boolean}) {
 const { t } = useTranslation('settings');

 const confirm=useConfirm(),inFlight=useRef(false);
 const [busy,setBusy]=useState(false),[result,setResult]=useState<LegacyObservation>(),[error,setError]=useState<unknown>();
 const session=!unavailable&&capability?.supported===false?capability.nativeSessionId:null;
 const read=()=>{if(!session||inFlight.current)return;inFlight.current=true;setBusy(true);setError(undefined);setResult(undefined);
   void api<LegacyObservation>(`${path}/legacy-snapshot`,{method:'POST',body:{mode:'LEGACY_FULL_SNAPSHOT',expectedNativeSessionId:session,acknowledgeFullHistoryCost:true}})
    .then(setResult).catch(setError).finally(()=>{inFlight.current=false;setBusy(false);});};
 return <section aria-label={t("One-time full observation from a legacy runner")}><h4>{t("One-time full observation from a legacy runner")}</h4>
 <p>{t("Reading all history may block the original node or exhaust memory.")}</p>
 <Button disabled={!session||busy} loading={busy} onClick={()=>confirm(t("Read the legacy full-history snapshot once?"),t("A large history may make the original runner slow, time out, or run out of memory, affecting the same node. Read only the connected original session; do not start a strategy. Failures are not retried automatically."),read)}>{t("Explicitly read legacy observation once")}</Button>
 {!!error&&<p role="alert">{t("Completion is unconfirmed. No automatic resend occurred, and old records remain. The original runner may still be processing the request; do not immediately repeat the read.")}</p>}
 {result&&<div><p>{t("Original session:")}{result.nativeSessionId}{t("; original observation time (ns):")}{result.observedAtNs}{t("; received at:")}{result.receivedAt}{t(". This is a historical reading, not proof of current connectivity or trading readiness")}{result.nativeSessionId!==session?t("; the current session has changed or is unavailable"):''}{t("Sentence end")}</p>
 <p>{t("Original cash:")}{typeof result.observation.cash==='string'?result.observation.cash:t("Unknown")}{t("; original equity:")}{typeof result.observation.equity==='string'?result.observation.equity:t("Unknown")}{t("Sentence end")}</p></div>}
 </section>;
}
interface LifecycleCapability {supported?:boolean|null;configuredKind?:string|null;scope?:string|null;strategyVerified?:boolean|null;nativeSessionId?:string|null}
export function StrategyLifecycleNotice({value,unavailable=false}:{value?:LifecycleCapability|null;unavailable?:boolean}){
 const { t } = useTranslation('settings');

 const observed=!unavailable&&value?.scope==='INSTALLED_DISPATCH_ONLY'&&value.strategyVerified===false&&!!value.nativeSessionId&&typeof value.supported==='boolean';
 return <section aria-label={t("Most recently read native strategy wiring report")}><h4>{t("Most recently read native strategy wiring report")}</h4><p>{!observed?t("Native strategy wiring status is unknown."):value.supported?t("Latest report: a strategy dispatch entry point is installed."):t("Latest report: no available strategy dispatch entry point was reported.")}</p>
 {observed&&<p>{t("Native report type:")}{value.configuredKind??t("Not provided")}{t("; original session:")}{value.nativeSessionId}{t("Sentence end")}</p>}
 </section>;
}
/** Edit only known deployment strings; opaque owner configuration is not interpreted. */
export function strategyConfigurationField(text:string,field:'instrument_id'|'name',value:string){
 const parsed:unknown=JSON.parse(text);
 if(!parsed||typeof parsed!=='object'||Array.isArray(parsed))throw Error(t("Original strategy configuration must be an object"));
 const config=parsed as Record<string,unknown>;
 if(field==='instrument_id')return JSON.stringify({...config,instrument_id:value},null,2);
 const strategy=config.strategy;
 if(!strategy||typeof strategy!=='object'||Array.isArray(strategy)||(strategy as Record<string,unknown>).kind!=='REGISTERED')throw Error(t("The original strategy is not REGISTERED; its type is not replaced"));
 return JSON.stringify({...config,strategy:{...strategy,name:value}},null,2);
}
export function StrategyConfigurationFields({text,disabled,onChange}:{text:string;disabled:boolean;onChange:(value:string)=>void}){
 const { t } = useTranslation('settings');

 let config:{instrument_id?:unknown;strategy?:{kind?:unknown;name?:unknown}};
 try{const value:unknown=JSON.parse(text);if(!value||typeof value!=='object'||Array.isArray(value))return <p>{t("Fix the advanced configuration JSON object.")}</p>;config=value;}catch{return <p>{t("Fix the advanced configuration JSON object.")}</p>;}
 return <fieldset disabled={disabled}><legend>{t("Native strategy configuration")}</legend>
 <label>{t("Strategy instrument (must retain the account-bound instrument)")}<Input aria-label={t("Edit strategy instrument")} disabled={disabled} value={typeof config.instrument_id==='string'?config.instrument_id:''} onChange={event=>onChange(strategyConfigurationField(text,'instrument_id',event.target.value))}/></label>
 {config.strategy?.kind==='REGISTERED'&&<label>{t("Original strategy name compiled into the runner")}<Input aria-label={t("Edit original strategy name")} disabled={disabled} value={typeof config.strategy.name==='string'?config.strategy.name:''} onChange={event=>onChange(strategyConfigurationField(text,'name',event.target.value))}/></label>}
 </fieldset>;
}
type StrategyEditIdentity={accountSourceId:string;strategyRevision:string};
export type StrategyEditDraft=StrategyEditIdentity&{text:string;accountKey:string};
export function strategyEditChanged(draft:StrategyEditDraft|undefined,current:StrategyEditIdentity|undefined,accountKey:string){
 return !!draft&&(!current||draft.accountKey!==accountKey||draft.accountSourceId!==current.accountSourceId||draft.strategyRevision!==current.strategyRevision);
}
export function strategyEditSubmission(draft:StrategyEditDraft|undefined,current:StrategyEditIdentity|undefined,accountKey:string,text:string){
 if(!current||strategyEditChanged(draft,current,accountKey))throw Error(t("The original strategy configuration version changed. Your draft is retained. Review it before explicitly discarding the draft and loading the latest configuration."));
 const identity=draft??current;
 return {configuration:JSON.parse(text),expectedAccountSourceId:identity.accountSourceId,expectedStrategyRevision:identity.strategyRevision};
}
export default function AccountStrategy({ value, configured=false, remoteUnsupported=[] }: { remoteUnsupported?:string[]; value: Pick<ObservationView,'id'|'mode'|'connectionName'|'connectionId'>;configured?:boolean }) {
 const { t } = useTranslation('settings');

  const path = configured?`/accounts/configurations/${encodeURIComponent(value.id)}/strategy`:`/observations/${encodeURIComponent(value.id)}/strategy`;
  const commandIdentity=configured?`configured:${value.id}`:value.id;
  const status = useResource<{ accountSourceId: string; strategyRevision: string; strategy: NativeAccountStrategyStatus }>(path, { poll: 3000 });
  const fence = useResource<{ accountSourceId: string; strategyRevision: string }>(`${path}/fence`, { poll: 3000 });
  const configuration = useResource<{ accountSourceId: string; strategyRevision: string; configuration: NativeAccountStrategyConfig; accountSemantics?: 'NATIVE_MARGIN' | 'LEGACY_SPOT'; connected: boolean | null; runnerValidated: boolean; lifecycleCapability?:LifecycleCapability|null; accountObservation?:AccountObservationCapability|null; pendingConfigurationRequests: string[] }>(`${path}/config`,{poll:3000});
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
  const liveSemanticsBlocked = value.mode === 'live' && configuration.data?.accountSemantics !== 'NATIVE_MARGIN';
  const unsettled = !!requestId && (!receipt.data || ['PENDING', 'UNCERTAIN'].includes(receipt.data.status));
  return <section className="result-section"><h3>{t("Nautilus native strategy")}</h3>
    <p>{t("Do not enter private keys or passwords.")}</p>
    <AccountObservationNotice value={configuration.data?.accountObservation} unavailable={!!configuration.error}/>
    {configured&&!remoteUnsupported.includes('legacySnapshot')&&<LegacyAccountObservation key={path} path={path} capability={configuration.data?.accountObservation} unavailable={!!configuration.error}/>}
    <StrategyLifecycleNotice value={configuration.data?.lifecycleCapability} unavailable={!!configuration.error}/>
    {configured&&!remoteUnsupported.includes('spotObservation')&&value.mode==='live'&&<SpotBalanceObservation key={path} path={`/accounts/configurations/${encodeURIComponent(value.id)}`} sourceId={configuration.data?.accountSourceId} nativeSessionId={configuration.data?.accountObservation?.nativeSessionId} connected={!configuration.error&&configuration.data?.connected===true&&configuration.data?.accountObservation?.supported===true}/>}
    <details><summary>{t("Configure native account strategy and parameters (no connection or trading)")}</summary>

      <ErrorNotice error={configuration.error ?? configurationError ?? saveConfiguration.error} retry={() => { void configuration.refetch(); }} />
      <StrategyConfigurationFields text={configurationText} onChange={editConfiguration} disabled={!configuration.data || configuration.data.connected !== false || !!saveConfiguration.pending}/>
      <details><summary>{t("Advanced: full native strategy configuration")}</summary><p>{t("Use the string format required by the original strategy for exact decimals.")}</p>
      <Input.TextArea aria-label={t("Native account strategy configuration JSON")} autoSize={{ minRows: 8, maxRows: 24 }} value={configurationText} disabled={!configuration.data || configuration.data.connected !== false || !!saveConfiguration.pending} onChange={event => editConfiguration(event.target.value)} /></details>
      {editChanged&&<p role="alert">{t("The account or strategy configuration version changed. Your draft is retained and cannot overwrite the original configuration using a new version number. Review it before explicitly discarding the draft and adopting the latest configuration.")}</p>}
      {edited&&<Button disabled={!!saveConfiguration.pending||saveConfiguration.busy} onClick={()=>confirm(t("Discard the unsaved draft?"),t("Discard local edits and show the most recently read original configuration. Do not save, connect, or start anything."),()=>{setEdited(undefined);setConfigurationError(undefined);void configuration.refetch();})}>{t("Discard draft and use latest configuration")}</Button>}
      <p>{t("Configuration can be saved only while disconnected.")}</p>
      <Button disabled={!configuration.data || configuration.data.connected !== false || !!saveConfiguration.pending || editChanged} loading={saveConfiguration.busy} onClick={() => {
        try { const payload = strategyEditSubmission(edited,configuration.data,commandIdentity,configurationText); setConfigurationError(undefined); void saveConfiguration.submit(payload); }
        catch (error) { setConfigurationError(error); }
      }}>{t("Save configuration only")}</Button>
      {!remoteUnsupported.includes('verifyConfiguration')&&(configuration.data?.pendingConfigurationRequests ?? []).map(requestId => <Button key={requestId} loading={verifyingConfiguration} disabled={verifyingConfiguration} onClick={() => confirm(t("Check whether the original configuration was persisted?"), t("Check only the files and synchronization result for original request {{v0}}. Do not connect Nautilus, start a strategy, or resolve any order UNKNOWN state.", { v0: requestId }), () => {
        setVerifyingConfiguration(true); void api<{status:string}>(`${path}/config/commands/${encodeURIComponent(requestId)}/verify`, {method:'POST',body:{}}).then(result=>{setConfigurationReceipt(result);void configuration.refetch();}).catch(setConfigurationError).finally(()=>setVerifyingConfiguration(false));
      })}>{t("Check original configuration save result:")}{requestId}</Button>)}
      <PendingCommand command={saveConfiguration} />
      {configurationReceipt && <p>{configurationReceipt.status === 'COMPLETED' ? t("Configuration saved; not connected or trading.") : t("Configuration persistence is uncertain. Keep the original request; trading start is blocked. Check the original configuration receipt rather than treating this as a successful save.")}</p>}
    </details>
    {liveSemanticsBlocked && <p role="status">{t("Live start is unavailable: spot balances and risk controls are not integrated yet.")}</p>}
    <ErrorNotice error={status.error} retry={() => { void status.refetch(); }} />
    {native && <><Facts items={[[t("Account mode"), value.mode === 'live' ? t("Live: may place real orders") : t("Paper trading")], [t("Strategy"), native.strategy_id ?? t("Not registered (reconnection only)")], [t("Status"), <Status value={native.state} />], [t("Instrument"), native.instrument_id], [t("Native version"), native.native_version], [t("Strategy control revision"), status.data!.strategyRevision]]} /><details><summary>{t("Full native account strategy configuration")}</summary><pre>{JSON.stringify(native.config, null, 2)}</pre></details></>}
    <ErrorNotice error={fence.error} retry={() => { void fence.refetch(); }} />
    <div className="form-actions"><Button loading={recover.busy} disabled={!fence.data || !!fence.error || !!recover.pending} onClick={() => confirm(t("Connect the account for recovery?"), t("Explicitly connect this account's Nautilus recovery node without loading a trading strategy or automatically placing, modifying, or cancelling orders. Original orders, reservations, and UNKNOWN states remain. A connection does not prove reconciliation or trading readiness."), () => { void recover.submit({ action: 'RECOVER', expectedAccountSourceId: fence.data!.accountSourceId, expectedStrategyRevision: fence.data!.strategyRevision }); })}>{t("Connect account for recovery")}</Button><Button type="primary" loading={start.busy} disabled={liveSemanticsBlocked || !accountSummaryStartAllowed(configuration.data?.accountObservation,!!configuration.error) || !supported || unsettled || !!status.error || native?.state === 'RUNNING' || !!start.pending} onClick={() => confirm(t("Start the native strategy?"), t("{{v0}} Account: {{v1}}; strategy: {{v2}}; instrument: {{v3}}. This starts more than observation.", { v0: value.mode === 'live' ? t("This is live trading. The configured strategy may send real orders.") : t("This is paper trading."), v1: value.connectionName ?? value.connectionId ?? value.id, v2: native?.strategy_id, v3: native?.instrument_id }), () => { void start.submit({ action: 'START', expectedAccountSourceId: status.data!.accountSourceId, expectedStrategyRevision: status.data!.strategyRevision }); })}>{value.mode === 'live' ? t("Start live strategy") : t("Start paper strategy")}</Button>
    <Button danger loading={stop.busy} disabled={!!stop.pending} onClick={() => confirm(t("Stop the native strategy?"), t("Send the official strategy STOP to this account. Depending on upstream configuration, this may cancel orders or close positions. Stopping local observation does not stop the strategy."), () => { void stop.submit({ action: 'STOP' }); })}>{t("Stop strategy")}</Button>
    <Button onClick={() => { void status.refetch(); void fence.refetch(); void configuration.refetch(); if (requestId) void receipt.refetch(); }}>{t("Check status and original receipt")}</Button></div>
    <PendingCommand command={recover} /><PendingCommand command={start} /><PendingCommand command={stop} /><ErrorNotice error={recover.error ?? start.error ?? stop.error ?? receipt.error} />
    {receipt.data && <div><h4>{t("Latest strategy command receipt")}</h4><Facts items={[[t("Request"), receipt.data.requestId], [t("Actions"), receipt.data.action], [t("Receipt"), <Status value={receipt.data.status} />]]} /><p>{strategyResultText(receipt.data)}</p>{receipt.data.error && <code>{receipt.data.error}</code>}</div>}
    {configured&&<AccountOrders key={value.id} accountId={value.id} readOnly={remoteUnsupported.includes('nativeCancellation')}/>}
  </section>;
}
