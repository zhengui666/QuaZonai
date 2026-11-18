import { useTranslation } from 'react-i18next';
import i18n from './i18n';
const t = (key: string, values?: Record<string, unknown>): string => i18n.t(key, { ns: 'settings', ...values });
import {useRef,useState} from 'react';
import {Button,Input,Select} from 'antd';
import type {RemoteAccountBinding} from '../../../packages/contracts/src/remote-account.ts';
import {api} from './api';
import {useResource} from './data-hooks';
import {useCommand} from './commands';
import {ErrorNotice,PendingCommand} from './ui';
export interface RemoteAccountProbe {probeId:string;targetId:string;binding:RemoteAccountBinding;configuration:unknown}
export function remoteAccountBindInput(id:string,targetId:string,probe:RemoteAccountProbe|undefined,supersedes?:string){
 if(supersedes===id)throw Error(t("Use a new binding name for a new session. The original binding is retained"));
 if(!/^remote-[A-Za-z0-9_-]{1,64}$/.test(id)||!probe||probe.targetId!==targetId||!probe.probeId)throw Error(t("Probe the selected remote and enter a unique binding name first"));
 return {id,targetId,probeId:probe.probeId,binding:structuredClone(probe.binding),...(supersedes?{supersedes}:{})};
}
export default function RemoteAccountConfiguration({onSaved,previous=[]}:{onSaved:()=>void;previous?:Array<{id:string}>}){
 const { t } = useTranslation('settings');

 const targets=useResource<Array<{id:string;name:string}>>('/accounts/remote/targets');
 const [targetId,setTargetId]=useState(''),[id,setId]=useState(''),[probe,setProbe]=useState<RemoteAccountProbe>(),[error,setError]=useState<unknown>(),[notice,setNotice]=useState(''),[probing,setProbing]=useState(false);
 const [supersedes,setSupersedes]=useState<string>();
 const inFlight=useRef(false);
 const command=useCommand<{id:string}>('remote-account-binding',result=>{setNotice(t("Remote binding {{v0}} saved. No start was requested.", { v0: result.id }));setProbe(undefined);onSaved();},'/accounts/remote/bind');
 const disabled=probing||command.busy||!!command.pending;
 const inspect=async()=>{if(inFlight.current||disabled||!targetId)return;inFlight.current=true;setProbing(true);setError(undefined);setProbe(undefined);try{setProbe(await api<RemoteAccountProbe>('/accounts/remote/probe',{method:'POST',body:{targetId}}));}catch(error){setError(error);}finally{inFlight.current=false;setProbing(false);}};
 return <section aria-label={t("Remote native account")}><h3>{t("Remote account")}</h3>
 <Select aria-label={t("Authorized remote")} placeholder={t("Choose a configured remote")} disabled={disabled} value={targetId||undefined} onChange={value=>{setTargetId(value);setProbe(undefined);setNotice('');}} options={targets.data?.map(item=>({value:item.id,label:item.name}))}/>
 <Select aria-label={t("Original binding whose new session to accept")} placeholder={t("Optional: new session for an original binding")} allowClear disabled={disabled} value={supersedes} onChange={setSupersedes} options={previous.map(item=>({value:item.id,label:item.id}))}/>
 {supersedes&&<p>{t("Keep the original binding and UNKNOWN state; do not resend the original request.")}</p>}
 <Input aria-label={t("Remote binding name")} placeholder={t("Unique name starting with remote-")} value={id} disabled={disabled} onChange={event=>setId(event.target.value)}/>
 <Button disabled={disabled||!targetId} loading={probing} onClick={()=>void inspect()}>{t("Probe original session")}</Button>
 {probe&&<dl><dt>{t("Original account")}</dt><dd>{probe.binding.accountId}</dd><dt>{t("Original source")}</dt><dd>{probe.binding.sourceId}</dd><dt>{t("Original session")}</dt><dd>{probe.binding.nativeSessionId??t("Not reported")}</dd><dt>{t("Deployment")}</dt><dd>{probe.binding.deploymentId}</dd></dl>}
 <Button loading={command.busy} disabled={probing||command.busy||(!command.pending&&(!probe||!id))} onClick={()=>{if(command.pending){void command.submit();return;}try{setError(undefined);void command.submit(remoteAccountBindInput(id,targetId,probe,supersedes));}catch(error){setError(error);}}}>{command.pending?t("Check original binding request"):supersedes?t("Accept new session (do not start)"):t("Save binding (do not start)")}</Button>
 <PendingCommand command={command}/><ErrorNotice error={error??command.error??targets.error}/>{notice&&<p>{notice}</p>}
 </section>;
}
