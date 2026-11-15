import {useRef,useState} from 'react';
import {Button,Input,Select} from 'antd';
import type {RemoteAccountBinding} from '../../../packages/contracts/src/remote-account.ts';
import {api} from './api';
import {useResource} from './data-hooks';
import {useCommand} from './commands';
import {ErrorNotice,PendingCommand} from './ui';
export interface RemoteAccountProbe {probeId:string;targetId:string;binding:RemoteAccountBinding;configuration:unknown}
export function remoteAccountBindInput(id:string,targetId:string,probe:RemoteAccountProbe|undefined,supersedes?:string){
 if(supersedes===id)throw Error('新会话须使用新的绑定名称，原绑定保留');
 if(!/^remote-[A-Za-z0-9_-]{1,64}$/.test(id)||!probe||probe.targetId!==targetId||!probe.probeId)throw Error('请先探测所选远端并填写唯一绑定名称');
 return {id,targetId,probeId:probe.probeId,binding:structuredClone(probe.binding),...(supersedes?{supersedes}:{})};
}
export default function RemoteAccountConfiguration({onSaved,previous=[]}:{onSaved:()=>void;previous?:Array<{id:string}>}){
 const targets=useResource<Array<{id:string;name:string}>>('/accounts/remote/targets');
 const [targetId,setTargetId]=useState(''),[id,setId]=useState(''),[probe,setProbe]=useState<RemoteAccountProbe>(),[error,setError]=useState<unknown>(),[notice,setNotice]=useState(''),[probing,setProbing]=useState(false);
 const [supersedes,setSupersedes]=useState<string>();
 const inFlight=useRef(false);
 const command=useCommand<{id:string}>('remote-account-binding',result=>{setNotice(`已保存远端绑定 ${result.id}；未请求启动。`);setProbe(undefined);onSaved();},'/accounts/remote/bind');
 const disabled=probing||command.busy||!!command.pending;
 const inspect=async()=>{if(inFlight.current||disabled||!targetId)return;inFlight.current=true;setProbing(true);setError(undefined);setProbe(undefined);try{setProbe(await api<RemoteAccountProbe>('/accounts/remote/probe',{method:'POST',body:{targetId}}));}catch(error){setError(error);}finally{inFlight.current=false;setProbing(false);}};
 return <section aria-label="远端原生账户"><h3>远端账户</h3>
 <Select aria-label="已授权远端" placeholder="选择已配置远端" disabled={disabled} value={targetId||undefined} onChange={value=>{setTargetId(value);setProbe(undefined);setNotice('');}} options={targets.data?.map(item=>({value:item.id,label:item.name}))}/>
 <Select aria-label="接受新会话的原绑定" placeholder="可选：原绑定的新会话" allowClear disabled={disabled} value={supersedes} onChange={setSupersedes} options={previous.map(item=>({value:item.id,label:item.id}))}/>
 {supersedes&&<p>保留原绑定与 UNKNOWN，不重发原请求。</p>}
 <Input aria-label="远端绑定名称" placeholder="remote- 开头的唯一名称" value={id} disabled={disabled} onChange={event=>setId(event.target.value)}/>
 <Button disabled={disabled||!targetId} loading={probing} onClick={()=>void inspect()}>探测原会话</Button>
 {probe&&<dl><dt>原账户</dt><dd>{probe.binding.accountId}</dd><dt>原来源</dt><dd>{probe.binding.sourceId}</dd><dt>原会话</dt><dd>{probe.binding.nativeSessionId??'未报告'}</dd><dt>部署</dt><dd>{probe.binding.deploymentId}</dd></dl>}
 <Button loading={command.busy} disabled={probing||command.busy||(!command.pending&&(!probe||!id))} onClick={()=>{if(command.pending){void command.submit();return;}try{setError(undefined);void command.submit(remoteAccountBindInput(id,targetId,probe,supersedes));}catch(error){setError(error);}}}>{command.pending?'核实原绑定请求':supersedes?'接受新会话（不启动）':'保存此绑定（不启动）'}</Button>
 <PendingCommand command={command}/><ErrorNotice error={error??command.error??targets.error}/>{notice&&<p>{notice}</p>}
 </section>;
}
