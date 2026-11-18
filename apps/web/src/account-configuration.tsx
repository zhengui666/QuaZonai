import { accountAdapter, HYPERLIQUID_ACCOUNT_ADAPTER } from '../../../packages/contracts/src/account-adapters.ts';
import { useTranslation } from 'react-i18next';
import i18n from './i18n';
const t = (key: string, values?: Record<string, unknown>): string => i18n.t(key, { ns: 'settings', ...values });
import type {SavedAccountRunnerReport} from '../../../packages/contracts/src/nautilus.ts';
import RemoteAccountConfiguration from './remote-account-configuration';
import type {RemoteAccountBinding} from '../../../packages/contracts/src/remote-account.ts';
import AccountStrategy from './account-strategy';
import {useState} from 'react';
import {Button,Input,Checkbox,Select} from 'antd';
import {ResourceActivity,useResource} from './data-hooks';
import {useIsFetching} from '@tanstack/react-query';
import {useCommand} from './commands';
import {ErrorNotice,PendingCommand} from './ui';

export function accountConfigurationInput(text:string):Record<string,unknown>{
 const value:unknown=JSON.parse(text);
 if(!value||typeof value!=='object'||Array.isArray(value))throw new Error(t("Account configuration must be a JSON object"));
 return value as Record<string,unknown>;
}
const initialAccountDraft=JSON.stringify({mode:'paper',nativeStrategy:{instrument_id:'',strategy:{kind:'REGISTERED',name:''}}});
/** Field edits preserve unrelated owner values; identities and decimal values are never converted to numbers. */
export function accountConfigurationField(text:string,key:string,value:string):string{
 const config=accountConfigurationInput(text);
 const deployment={...((config.nativeStrategy??{}) as Record<string,unknown>)};
 const strategy={...((deployment.strategy??{}) as Record<string,unknown>)};
 if(['adapter','id','clientId','accountId','mode','network','startingBalance','paperFeeRate','accountAddress','accountSemantics','credentialEnv'].includes(key)){
  if(value==='')delete config[key];else config[key]=value;
 }else if(key==='instruments')config.instruments=value.split('\n').filter(line=>line!=='');
 else if(key==='instrument_id')deployment.instrument_id=value;
 else if(key==='name')strategy.name=value;
 else throw Error(t("Unknown account field"));
 if(key==='mode'&&value==='paper')delete config.credentialEnv;
 if(key==='name')deployment.strategy=strategy;
 if(key==='name'||key==='instrument_id')config.nativeStrategy=deployment;
 return JSON.stringify(config,null,2);
}
/** Keep textarea editing text verbatim. Only explicit save parses instrument lines. */
export function AccountInstrumentsInput({value,onChange,disabled}:{value:string;onChange:(value:string)=>void;disabled:boolean}){
 const { t } = useTranslation('settings');

 return <Input.TextArea aria-label={t("Subscribed instruments")} disabled={disabled} value={value} onChange={event=>onChange(event.target.value)}/>;
}
export function accountConfigurationForSave(text:string,instrumentsDraft?:string){
 return accountConfigurationInput(instrumentsDraft===undefined?text:accountConfigurationField(text,'instruments',instrumentsDraft));
}
export function AccountFields({text,onChange,disabled,instrumentsDraft,onInstrumentsChange,adapters=[HYPERLIQUID_ACCOUNT_ADAPTER]}:{adapters?:readonly string[];text:string;onChange:(text:string)=>void;disabled:boolean;instrumentsDraft?:string;onInstrumentsChange?:(value:string)=>void}){
 const { t } = useTranslation('settings');

 let config:Record<string,unknown>;try{config=accountConfigurationInput(text);}catch{return <p>{t("Advanced configuration must be a valid JSON object.")}</p>;}
 const deployment=config.nativeStrategy as {instrument_id?:string;strategy?:{kind?:string;name?:string}}|undefined;
 const edit=(key:string,value:string)=>onChange(accountConfigurationField(text,key,value));
 const field=(key:string,label:string)=><label style={{display:'block'}} key={key}>{label}<Input aria-label={label} disabled={disabled} value={String(config[key]??'')} onChange={event=>edit(key,event.target.value)}/></label>;
 const adapter=accountAdapter(config as {adapter?:string});
 return <fieldset disabled={disabled}><legend>{t("Account and native strategy")}</legend>
 <label>{t('Broker or exchange adapter')}<Select aria-label={t('Broker or exchange adapter')} disabled={disabled} value={adapter} onChange={value=>edit('adapter',value)} options={adapters.filter(id=>id===HYPERLIQUID_ACCOUNT_ADAPTER).map(id=>({value:id,label:'Hyperliquid'}))}/></label>
 {!adapters.includes(adapter)&&<p>{t('The selected runner does not provide this account adapter.')}</p>}
 {[['id',t("Local account name")],['clientId',t("Client ID")],['accountId',t("Native account ID")]].map(([key,label])=>field(key!,label!))}
 <label>{t("Account mode")}<Select aria-label={t("Account mode")} disabled={disabled} value={config.mode as string} onChange={value=>edit('mode',value)} options={[{value:'paper',label:t("Paper trading")},{value:'live',label:t("Live configuration (existing start restrictions still apply)")}]}/></label>
 <label style={{display:'block'}}>{t("Subscribed instruments (one native InstrumentId per line)")}<AccountInstrumentsInput disabled={disabled} value={instrumentsDraft??(Array.isArray(config.instruments)?config.instruments.join('\n'):'')} onChange={value=>onInstrumentsChange?.(value)}/></label>
 <label style={{display:'block'}}>{t("Strategy instrument (must be subscribed)")}<Input aria-label={t("Strategy instrument")} disabled={disabled} value={deployment?.instrument_id??''} onChange={event=>edit('instrument_id',event.target.value)}/></label>
 {adapter===HYPERLIQUID_ACCOUNT_ADAPTER&&<fieldset><legend>Hyperliquid</legend>
 <label>{t("Network")}<Select aria-label={t("Network")} disabled={disabled} allowClear value={config.network as string|undefined} onChange={value=>edit('network',value??'')} options={[{value:'mainnet',label:t("Mainnet market data")},{value:'testnet',label:t("Testnet market data")}]}/></label>
 {config.mode==='paper'?<>{field('startingBalance',t("Paper starting balance (exact USDC amount)"))}{field('paperFeeRate',t("Paper fee rate (decimal, not percentage)"))}</>:field('accountAddress',t("Public account address (not a private key)"))}
 {config.mode==='live'&&<>{field('credentialEnv',t("Existing signing credential reference (environment variable name, optional)"))}<p>{t("Enter only a QZ_HYPERLIQUID_…_PK variable name already configured in the runtime, never a private key. Leave blank to use the host credentials for this network. Saving neither reads credentials nor connects the account. A reference does not prove a distinct signer or authorization for the account. Rotating a value under the same name is not covered by account identity checks. Do not use the same signer concurrently across running instances.")}</p><label>{t("Account semantics")}<Select aria-label={t("Account semantics")} disabled={disabled} value={config.accountSemantics as string|undefined} allowClear onChange={value=>edit('accountSemantics',value??'')} placeholder={t("Legacy spot semantics (live-start protection remains)")} options={[{value:'NATIVE_MARGIN',label:t("Explicit Nautilus Margin account")}]}/></label><p>{t("This does not change the exchange account mode.")}</p></>}
 </fieldset>}
 {deployment?.strategy?.kind==='REGISTERED'&&<label style={{display:'block'}}>{t("Strategy name compiled into the runner")}<Input aria-label={t("Strategy name compiled into the runner")} disabled={disabled} value={deployment.strategy.name??''} onChange={event=>edit('name',event.target.value)}/></label>}

 </fieldset>;
}
const paperFields = [
 ['prob_fill_on_limit',"Limit fill probability (0–1)"],['prob_slippage',"Slippage probability (0–1)"],
 ['random_seed',"Random seed (exact integer)"],['base_latency_ns',"Base latency (ns)"],
 ['insert_latency_ns',"Order submission latency (ns)"],['update_latency_ns',"Order modification latency (ns)"],['cancel_latency_ns',"Order cancellation latency (ns)"],
] as const;
export function paperConfigurationField(text:string,key:string,value:string|boolean):string{
 const config=accountConfigurationInput(text);
 if(config.mode!=='paper')throw new Error(t("Simulation parameters apply only to paper accounts"));
 if(!paperFields.some(([field])=>field===key)&&key!=='queue_position')throw new Error(t("Unknown simulation parameter"));
 const prior=config.paperExecution;
 if(prior!==undefined&&(!prior||typeof prior!=='object'||Array.isArray(prior)))throw new Error(t("paperExecution must be an object"));
 const scenario={...(prior as Record<string,unknown>|undefined)};
 if(value==='')delete scenario[key];
 else if(key==='prob_fill_on_limit'||key==='prob_slippage'){
  const probability=Number(value);
  if(typeof value!=='string'||!value.trim()||!Number.isFinite(probability))throw new Error(t("Probability must be a valid number"));
  scenario[key]=probability;
 }else scenario[key]=value;
 return JSON.stringify({...config,paperExecution:scenario},null,2);
}
/** The native input owns intermediate decimal text until blur. Parsing on every
 * keystroke would turn "0." into "0" and corrupt the next keystroke. */
export function PaperProbabilityInput({label,value,disabled,onCommit}:{label:string;value:unknown;disabled:boolean;onCommit:(text:string)=>boolean|void}){
 useTranslation('settings');

 return <Input key={String(value??'')} aria-label={label} defaultValue={String(value??'')} disabled={disabled}
  onBlur={event=>{if(onCommit(event.target.value)===false)event.target.value=String(value??'');}} onPressEnter={event=>event.currentTarget.blur()}/>;
}
function PaperParameters({text,onChange,disabled,onError}:{text:string;onChange:(value:string)=>void;disabled:boolean;onError:(error:unknown)=>void}){
 const { t } = useTranslation('settings');

 let config:Record<string,unknown>;try{config=accountConfigurationInput(text);}catch{return null;}
 if(config.mode!=='paper')return null;
 const value=config.paperExecution;
 if(value!==undefined&&(!value||typeof value!=='object'||Array.isArray(value)))return <p>{t("Fix the paperExecution object in the JSON first.")}</p>;
 const scenario=(value??{}) as Record<string,unknown>;
 const edit=(key:string,value:string|boolean)=>{try{onChange(paperConfigurationField(text,key,value));onError(undefined);return true;}catch(error){onError(new Error(t("{{v0}}. Configuration was not changed; the input was restored to its original JSON value.", { v0: error instanceof Error?error.message:t("Invalid simulation parameters") })));return false;}};
 const adapter=accountAdapter(config as {adapter?:string});
 return <fieldset disabled={disabled}><legend>{t("Official Sandbox simulation parameters (optional)")}</legend>

 {paperFields.map(([key,label])=><label key={key} style={{display:'block'}}>{t(label)}{key==='prob_fill_on_limit'||key==='prob_slippage'?<PaperProbabilityInput label={t(label)} value={scenario[key]} disabled={disabled} onCommit={value=>edit(key,value)}/>:<Input aria-label={t(label)} value={String(scenario[key]??'')} disabled={disabled} onChange={event=>edit(key,event.target.value)}/>}</label>)}
 <Checkbox checked={scenario.queue_position===true} disabled={disabled} onChange={event=>edit('queue_position',event.target.checked)}>{t("Official queue-position simulation")}</Checkbox>
 {value!==undefined&&<Button disabled={disabled} onClick={()=>{const next={...config};delete next.paperExecution;onChange(JSON.stringify(next,null,2));}}>{t("Remove all optional simulation parameters")}</Button>}
 </fieldset>;
}
export function SavedRunnerNotice({value}:{value?:SavedAccountRunnerReport}){
 const { t } = useTranslation('settings');

 return <section aria-label={t("Original account runner")}><h4>{t("Runner report at account creation")}</h4>

 <dl><dt>{t("Nautilus version")}</dt><dd>{value?.nautilusVersion??t("Not recorded")}</dd><dt>{t("Upstream revision")}</dt><dd>{value?.nautilusRevision??t("Not recorded")}</dd><dt>{t("Precision")}</dt><dd>{value?.precision??t("Not recorded")}</dd><dt>{t("Trusted-code compatibility declaration")}</dt><dd>{value?.compatibilityDeclaration==='TRUSTED_STRATEGY_ONLY_SINGLE_NODE_V1'?t("Declared single-node strategy registration (not certification)"):t("No declaration recorded")}</dd></dl>
 </section>;
}
export default function AccountConfiguration({enabled,adapters=[HYPERLIQUID_ACCOUNT_ADAPTER]}:{enabled:boolean;adapters?:readonly string[]}){
 const { t } = useTranslation('settings');

 const records=useResource<Array<{id:string;unavailable?:string;remoteBinding?:RemoteAccountBinding;supersededBy?:string;remoteUnsupported?:string[];savedRunner?:SavedAccountRunnerReport;accountConfiguration:{mode:'paper'|'live';paperExecution?:unknown}}>>('/accounts/configurations');
 const [instrumentsDraft,setInstrumentsDraft]=useState<string>();
 const [text,setText]=useState(initialAccountDraft),[error,setError]=useState<unknown>(),[notice,setNotice]=useState('');
 const command=useCommand<{id:string}>('account-configuration',result=>{setNotice(t("Configuration for account {{v0}} was saved. No connection or strategy start occurred.", { v0: result.id }));void records.refetch();},'/accounts/configurations');
 return <section><h3>{t("Save account configuration without connecting or trading")}</h3>

 <AccountFields adapters={adapters} text={text} onChange={setText} instrumentsDraft={instrumentsDraft} onInstrumentsChange={setInstrumentsDraft} disabled={!!command.pending||command.busy}/>

 <details><summary>{t("Advanced: native strategy parameters and full configuration")}</summary>
 <p>{t("Do not enter private keys, passwords, or tokens.")}</p>
 <Input.TextArea aria-label={t("New account configuration JSON")} autoSize={{minRows:8,maxRows:24}} value={text} disabled={!!command.pending} onChange={event=>{setText(event.target.value);setInstrumentsDraft(undefined);}}/></details>
 <PaperParameters text={text} onChange={setText} disabled={!!command.pending} onError={setError}/>
 {!enabled&&<p>{t("Select a runner with account support.")}</p>}
 <Button disabled={!enabled||command.busy} loading={command.busy} onClick={()=>{if(command.pending){void command.submit();return;}try{setError(undefined);void command.submit({configuration:accountConfigurationForSave(text,instrumentsDraft)});}catch(failure){setError(failure);}}}>{command.pending?t("Check the original account save request"):t("Save new account configuration only")}</Button>
 <PendingCommand command={command}/><ErrorNotice error={error??command.error??records.error}/>{notice&&<p>{notice}</p>}

 <RemoteAccountConfiguration onSaved={()=>void records.refetch()} previous={records.data?.filter(item=>item.remoteBinding&&!item.supersededBy).map(item=>({id:item.id}))}/>
 <Button onClick={()=>void records.refetch()}>{t("Read saved accounts only")}</Button>
 {records.data?.map(item=><SavedAccountPanel key={item.id} item={item}/>)}
 </section>;
}

export type SavedAccountItem={id:string;unavailable?:string;remoteBinding?:RemoteAccountBinding;supersededBy?:string;remoteUnsupported?:string[];savedRunner?:SavedAccountRunnerReport;accountConfiguration:{mode:'paper'|'live';paperExecution?:unknown}};
export function SavedAccountPanel({item}:{item:SavedAccountItem}){
 const { t } = useTranslation('settings');

 const [open,setOpen]=useState(false),[visited,setVisited]=useState(false);
 const prefix=`/accounts/configurations/${encodeURIComponent(item.id)}/strategy`;
 const fetching=useIsFetching({predicate:query=>typeof query.queryKey[0]==='string'&&(query.queryKey[0]===prefix||query.queryKey[0].startsWith(`${prefix}/`))});
 return <details onToggle={event=>{const next=event.currentTarget.open;setOpen(next);if(next)setVisited(true);}}>
 <summary>{item.id} · {item.accountConfiguration.mode}</summary>
 {visited&&<ResourceActivity.Provider value={open}>
 {item.remoteBinding?<p>{t("Remote ·")}{item.remoteBinding.accountId} · {item.remoteBinding.nativeSessionId??t("Not reported by the original session")}{item.supersededBy?t(" · Retained as history; new binding {{v0}}", { v0: item.supersededBy }):''}</p>:<SavedRunnerNotice value={item.savedRunner}/>}
 {item.accountConfiguration.paperExecution!==undefined&&<pre aria-label={t("Saved official simulation parameters, unchanged")}>{JSON.stringify(item.accountConfiguration.paperExecution,null,2)}</pre>}
 {open&&fetching>0&&<p role="status">{t("Refreshing. The values below may be from the previous read.")}</p>}
 {item.unavailable?<p>{t("Original account not ready:")}{item.unavailable}{t("Sentence end")}</p>:<AccountStrategy configured remoteUnsupported={item.remoteUnsupported} value={{id:item.id,mode:item.accountConfiguration.mode}}/>}
 </ResourceActivity.Provider>}
 </details>;
}
