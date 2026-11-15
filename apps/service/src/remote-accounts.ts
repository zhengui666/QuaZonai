import {acquireAccountReceiptLock} from '../../nautilus/src/account-receipt-lock.ts';
import {randomUUID} from 'node:crypto';
import {mkdir,open,readdir,readFile,rm} from 'node:fs/promises';
import {join} from 'node:path';
import {exchangeRemoteAccount,remoteAccountHello,remoteAccountTransport} from '../../nautilus/src/remote-account-client.ts';
import {remoteAccountFrame,sameRemoteAccountBinding,type RemoteAccountBinding} from '../../../packages/contracts/src/remote-account.ts';
import {canonical} from '../../research/src/documents.ts';
import {registerProcessAccount} from './downstream.ts';
import type {ServiceConfig,TrustedRemoteAccountTarget,ConnectionConfig} from './config.ts';
import {Problem,object,text} from './facts.ts';
const key=(v:unknown)=>canonical(v as never);
const loopback=(v:unknown)=>v==='127.0.0.1'||v==='::1'||v==='::ffff:127.0.0.1';
/** In addition to existing session authentication, never infer trust from clientId. */
export function assertRemoteAccountAccess(config:ServiceConfig,request:{headers:Record<string,unknown>;raw:{socket:{remoteAddress?:string}};method:string}){
 const origins=config.remoteAccounts?.origins??[];
 let origin=request.headers.origin;const host=request.headers.host;
 if(origin===undefined&&['GET','HEAD'].includes(request.method)&&request.headers['sec-fetch-site']==='same-origin'&&typeof request.headers.referer==='string'){
  try{origin=new URL(request.headers.referer).origin;}catch{}
 }
 if(!loopback(config.host)||!loopback(request.raw.socket.remoteAddress)||typeof origin!=='string'||typeof host!=='string')throw new Problem('remote_account_local_origin_required',403);
 const permitted=origins.some(value=>{try{const url=new URL(value);return ['http:','https:'].includes(url.protocol)&&['127.0.0.1','[::1]'].includes(url.hostname)&&url.origin===value&&value===origin&&url.host===host;}catch{return false;}});
 if(!permitted)throw new Problem('remote_account_local_origin_required',403);
 if(request.method!=='GET'&&request.method!=='HEAD'&&request.method!=='OPTIONS'&&!/^application\/json(?:\s*;|$)/i.test(String(request.headers['content-type']??'')))throw new Problem('remote_account_json_required',415);
}
interface Saved {schemaVersion:1;id:string;requestId:string;target:TrustedRemoteAccountTarget;binding:RemoteAccountBinding;mode:'paper'|'live';currency:string;supersedes?:string}
async function syncBindings(directory:string){const dir=await open(directory,'r');try{await dir.sync();}finally{await dir.close();}}
export async function remoteAccounts(config:ServiceConfig,exchange=exchangeRemoteAccount,syncPublication=syncBindings){
 const root=join(config.dataDir,'remote-account-bindings');await mkdir(root,{recursive:true,mode:0o700});
 const storeLease=config.remoteAccounts||(await readdir(root)).some(name=>name.endsWith('.json'))?await acquireAccountReceiptLock(root):{close:async()=>{}};
 const publishing=new Map<string,Saved>();
 const saved=new Map<string,Saved>(),detach=new Map<string,()=>void>();
 const probes=new Map<string,{expires:number;target:TrustedRemoteAccountTarget;binding:RemoteAccountBinding;configuration:Record<string,unknown>}>();let lane=Promise.resolve();let closed=false;
 function target(id:string){const matches=(config.remoteAccounts?.targets??[]).filter(t=>t.id===id);if(matches.length!==1)throw new Problem('remote_account_target_not_allowed',409);const selected=matches[0]!;if(selected.transport.kind!=='SSH')throw new Problem('remote_account_ssh_target_required',422);remoteAccountTransport(selected.transport);return structuredClone(selected);}
 function current(record:Saved){if(closed)throw new Problem('remote_account_service_closed',503);if(key(target(record.target.id))!==key(record.target))throw new Problem('remote_account_target_changed',409);}
 async function request(record:Saved,path:string,method:'GET'|'POST',clientId:unknown,input?:unknown){
  current(record);if(method==='POST'&&[...saved.values()].some(next=>next.supersedes===record.id))throw new Problem('remote_account_binding_superseded',409);if(clientId!==record.binding.clientId)throw new Problem('remote_account_client_binding_mismatch',409);
  const frame=remoteAccountFrame({version:1,requestId:randomUUID(),kind:'ACCOUNT_REQUEST',binding:record.binding,method,path,...(input===undefined?{}:{body:input})});
  const reply=await exchange(record.target.transport,frame);
  if(!reply.ok)throw new Problem(reply.error??'remote_account_outcome_unknown',reply.error==='remote_account_binding_changed'?409:503);
  if(method==='GET'&&(!reply.binding||!sameRemoteAccountBinding(record.binding,reply.binding)))throw new Problem('remote_account_binding_changed',409);
  // A command may create a new native session. Its response remains a result of
  // the original request; it never mutates this persisted connection binding.
  return {status:reply.status??200,value:reply.value};
 }
 function connection(record:Saved):ConnectionConfig{return {id:record.id,name:record.id,mode:record.mode,clientId:record.binding.clientId,accountId:record.binding.accountId,url:`rust-process://remote/${encodeURIComponent(record.id)}`,kind:'RUST_PROCESS',remoteBinding:{target:record.target,binding:record.binding}};}
 function attach(record:Saved){const conn=connection(record);if(config.connections.some(c=>c.id===record.id))throw new Problem('remote_account_id_conflict',409);detach.set(record.id,registerProcessAccount(conn,(path,method,client,input)=>request(record,path,method,client,input)));config.connections.push(conn);}
 function boundedConfiguration(value:unknown){if(Buffer.byteLength(JSON.stringify(value))>65536)throw new Problem('remote_account_configuration_too_large',422);return object(value);}
 function validate(value:unknown):Saved{const r=value as Saved;if(r?.schemaVersion!==1||!r.id||!/^remote-[A-Za-z0-9_-]{1,64}$/.test(r.id)||r.supersedes!==undefined&&typeof r.supersedes!=='string'||typeof r.requestId!=='string'||!r.requestId||!['paper','live'].includes(r.mode)||r.currency!=='USDC'||!r.target?.id)throw new Problem('remote_account_saved_binding_invalid',409);remoteAccountFrame({version:1,requestId:'validate',kind:'ACCOUNT_REQUEST',binding:r.binding,method:'GET',path:'/strategy/config'});return r;}
 try{for(const name of await readdir(root)){if(!name.endsWith('.json'))continue;const record=validate(JSON.parse(await readFile(join(root,name),'utf8')));if(name!==record.id+'.json'||saved.has(record.id))throw new Problem('remote_account_saved_binding_invalid',409);if(saved.size>=64)throw new Problem('remote_account_binding_limit',409);saved.set(record.id,record);attach(record);}}catch(error){for(const f of detach.values())f();await storeLease.close();throw error;}
 const targets=()=> (config.remoteAccounts?.targets??[]).map(t=>({id:t.id,name:t.name}));
 async function probe(value:unknown){const input=object(value);if(Object.keys(input).some(k=>k!=='targetId'))throw new Problem('remote_account_probe_invalid',422);const t=target(text(input.targetId,'targetId'));const hello=await exchange(t.transport,remoteAccountHello());if(!hello.ok||!hello.binding)throw new Problem(hello.error??'remote_account_hello_failed',409);
  const b=hello.binding;const reply=await exchange(t.transport,{version:1,requestId:randomUUID(),kind:'ACCOUNT_REQUEST',binding:b,method:'GET',path:'/strategy/config'});
  if(!reply.ok||!reply.binding||!sameRemoteAccountBinding(b,reply.binding))throw new Problem('remote_account_binding_changed',409);
  const configuration=boundedConfiguration(reply.value);if(configuration.accountSourceId!==b.sourceId||configuration.accountId!==b.accountId||configuration.clientId!==b.clientId||!['paper','live'].includes(String(configuration.mode))||configuration.currency!=='USDC')throw new Problem('remote_account_configuration_mismatch',409);
  for(const [id,p]of probes)if(p.expires<Date.now())probes.delete(id);if(probes.size>=64)throw new Problem('remote_account_probe_limit',429);
  const probeId=randomUUID();probes.set(probeId,{expires:Date.now()+600000,target:t,binding:b,configuration});return {probeId,targetId:t.id,binding:b,configuration};
 }
 async function row(record:Saved):Promise<Record<string,unknown>&{id:string}>{const base={id:record.id,transport:'SSH',accountConfiguration:{mode:record.mode},remoteBinding:record.binding,supersedes:record.supersedes??null,supersededBy:[...saved.values()].find(next=>next.supersedes===record.id)?.id??null,configurationOnly:true,automaticConnection:false,automaticStrategyStart:false,remoteUnsupported:['legacySnapshot','spotObservation','verifyConfiguration','nativeCancellation']};try{return {...boundedConfiguration((await request(record,'/strategy/config','GET',record.binding.clientId)).value),...base};}catch{return {...base,unavailable:'remote_account_original_binding_unavailable'};}}
 function bind(value:unknown){const input=object(value);const work=lane.then(async()=>{
  if(closed)throw new Problem('remote_account_service_closed',503);
  if(Object.keys(input).some(k=>!['id','requestId','probeId','targetId','binding','supersedes'].includes(k)))throw new Problem('remote_account_bind_invalid',422);
  const id=text(input.id,'id'),requestId=text(input.requestId,'requestId');if(!/^remote-[A-Za-z0-9_-]{1,64}$/.test(id)||requestId.length>512)throw new Problem('remote_account_bind_invalid',422);
  let prior=saved.get(id);
  if(!prior){try{prior=validate(JSON.parse(await readFile(join(root,id+'.json'),'utf8')));if(prior.id!==id||publishing.has(id)&&key(publishing.get(id))!==key(prior))throw new Problem('remote_account_saved_binding_invalid',409);}catch(error){if((error as NodeJS.ErrnoException).code!=='ENOENT')throw error;}}
  if(prior){if(prior.requestId!==requestId||prior.target.id!==input.targetId||prior.supersedes!==input.supersedes||key(prior.binding)!==key(input.binding))throw new Problem('remote_account_binding_immutable',409);if(!saved.has(id)){current(prior);await syncPublication(root);attach(prior);saved.set(id,prior);}return row(prior);}
  if(saved.size>=64)throw new Problem('remote_account_binding_limit',409);
  const p=probes.get(String(input.probeId));if(!p||p.expires<Date.now()||p.target.id!==input.targetId||key(p.binding)!==key(input.binding)||key(target(p.target.id))!==key(p.target))throw new Problem('remote_account_probe_stale',409);
  const previous=input.supersedes===undefined?undefined:saved.get(String(input.supersedes));
  if(input.supersedes!==undefined&&(!previous||key(previous.target)!==key(p.target)||previous.mode!==p.configuration.mode||previous.currency!==p.configuration.currency||previous.binding.nativeSessionId===p.binding.nativeSessionId||!sameRemoteAccountBinding({...previous.binding,nativeSessionId:p.binding.nativeSessionId},p.binding)||[...saved.values()].some(next=>next.supersedes===previous.id)))throw new Problem('remote_account_successor_invalid',409);
  const chain=new Set<string>();let ancestor=previous;while(ancestor){if(chain.has(ancestor.id))throw new Problem('remote_account_successor_invalid',409);chain.add(ancestor.id);ancestor=ancestor.supersedes?saved.get(ancestor.supersedes):undefined;}
  for(const old of saved.values())if(!chain.has(old.id)&&(old.binding.sourceId===p.binding.sourceId||old.binding.accountId===p.binding.accountId))throw new Problem('remote_account_history_requires_reconciliation',409);
  // No remote physical-account proof exists here. Do not assume a different
  // transport/account label proves it is unrelated to retained local live state.
  if(config.connections.some(c=>!chain.has(c.id)&&(c.id===id||c.accountId===p.binding.accountId||p.configuration.mode==='live'&&c.mode==='live')))throw new Problem('remote_account_history_requires_reconciliation',409);
  if(p.configuration.mode==='live'){const entries=await readdir(join(config.dataDir,'accounts')).catch((e:NodeJS.ErrnoException)=>{if(e.code==='ENOENT')return [];throw e;});if(entries.length)throw new Problem('remote_account_history_requires_reconciliation',409);}
  const record=validate({schemaVersion:1,id,requestId,target:p.target,binding:p.binding,mode:p.configuration.mode,currency:p.configuration.currency,...(previous?{supersedes:previous.id}:{})});
  const check=await request(record,'/strategy/config','GET',record.binding.clientId);const checked=object(check.value);if(check.status!==200||checked.accountSourceId!==record.binding.sourceId||checked.accountId!==record.binding.accountId||checked.clientId!==record.binding.clientId||checked.mode!==record.mode||checked.currency!==record.currency)throw new Problem('remote_account_binding_changed',409);
  const tmp=join(root,'.binding-'+randomUUID());const file=await open(tmp,'wx',0o600);
  try{await file.writeFile(JSON.stringify(record));await file.sync();}finally{await file.close();}
  publishing.set(id,record);
  // Exclusive final publication preserves another process's original binding.
  try{const {link}=await import('node:fs/promises');await link(tmp,join(root,id+'.json'));await syncPublication(root);}finally{await rm(tmp,{force:true});}
  saved.set(id,record);attach(record);return row(record);
 });lane=work.then(()=>undefined,()=>undefined);return work;}
 return {active:()=>(config.remoteAccounts?.targets.length??0)>0||saved.size>0,targets,probe,bind,has:(id:string)=>saved.has(id),async list(){const records=[...saved.values()],result:Array<Record<string,unknown>&{id:string}>=[];for(let i=0;i<records.length;i+=4)result.push(...await Promise.all(records.slice(i,i+4).map(row)));return result;},async binding(id:string){const record=saved.get(id);if(!record)throw new Problem('remote_account_not_found',404);current(record);return {connection:structuredClone(connection(record)),view:{currency:record.currency}};},async close(){if(closed)return;closed=true;await lane;for(const f of detach.values())f();config.connections=config.connections.filter(c=>!c.remoteBinding);await storeLease.close();}};
}
