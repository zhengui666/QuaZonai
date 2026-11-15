import {test} from 'node:test';
import assert from 'node:assert/strict';
import {mkdtemp,rm,readFile} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {configDefaults} from '../src/config.ts';
import {assertRemoteAccountAccess,remoteAccounts} from '../src/remote-accounts.ts';
import type {RemoteAccountReply,RemoteAccountFrame} from '../../../packages/contracts/src/remote-account.ts';
const target={id:'trusted',name:'Fixture',transport:{kind:'SSH',host:'fixture.invalid',user:'fixture',port:22,knownHostsFile:'/fixture/known_hosts',relayExecutable:'/fixture/relay',socketPath:'/fixture/account.sock'}};
const binding={deploymentId:'deployment',sourceId:'source',clientId:'client',accountId:'account',runnerIdentity:'fixed',nativeSessionId:null};
const configValue={...binding,accountSourceId:binding.sourceId,mode:'paper',currency:'USDC',configuration:{instrument_id:'TEST.SIM'}};
test('remote management adds strict local origin guards to existing auth',()=>{
 const config=configDefaults({remoteAccounts:{targets:[target],origins:['http://127.0.0.1:8080']}});
 const request={headers:{origin:'http://127.0.0.1:8080',host:'127.0.0.1:8080','content-type':'application/json'},raw:{socket:{remoteAddress:'127.0.0.1'}},method:'POST'};
 assert.doesNotThrow(()=>assertRemoteAccountAccess(config,request));
 assert.doesNotThrow(()=>assertRemoteAccountAccess(config,{...request,method:'GET',headers:{host:'127.0.0.1:8080','sec-fetch-site':'same-origin',referer:'http://127.0.0.1:8080/accounts'}}));
 assert.throws(()=>assertRemoteAccountAccess(config,{...request,method:'GET',headers:{host:'127.0.0.1:8080','sec-fetch-site':'cross-site',referer:'http://127.0.0.1:8080/accounts'}}));
 for(const origin of [undefined,'null','http://evil.invalid','http://127.0.0.1:8080/','http://127.0.0.1:8080.evil.invalid'])assert.throws(()=>assertRemoteAccountAccess(config,{...request,headers:{...request.headers,origin}}));
 assert.throws(()=>assertRemoteAccountAccess(config,{...request,raw:{socket:{remoteAddress:'192.0.2.1'}}}));
 assert.throws(()=>assertRemoteAccountAccess({...config,host:'0.0.0.0'},request));
 assert.throws(()=>assertRemoteAccountAccess(config,{...request,headers:{...request.headers,host:'evil.invalid'}}));
 assert.throws(()=>assertRemoteAccountAccess(config,{...request,headers:{...request.headers,'content-type':'text/plain'}}));
});
test('explicit probe and immutable binding save never sends mutations or overwrites old binding',async()=>{
 const root=await mkdtemp(join(tmpdir(),'qz-remote-service-'));
 const config=configDefaults({dataDir:root,remoteAccounts:{targets:[target],origins:['http://127.0.0.1:8080']}});let count=0;
 const exchange=async(_target:unknown,input:unknown):Promise<RemoteAccountReply>=>{const f=input as RemoteAccountFrame;count++;assert.ok(f.kind==='HELLO'||f.method==='GET');return {version:1,requestId:f.requestId,ok:true,binding,status:200,value:{...configValue,id:'untrusted-id',remoteBinding:{evil:true},automaticStrategyStart:true}};};
 const accounts=await remoteAccounts(config,exchange);
 try{
  assert.equal(count,0);await assert.rejects(accounts.probe({targetId:'other'}),/target_not_allowed/);assert.equal(count,0);
  const p=await accounts.probe({targetId:'trusted'});const input={id:'remote-fixture',requestId:'save-original',probeId:p.probeId,targetId:p.targetId,binding:p.binding};
  const row=await accounts.bind(input);assert.equal(row.id,'remote-fixture');assert.equal(row.automaticStrategyStart,false);assert.deepEqual(row.remoteBinding,binding);
  const path=join(root,'remote-account-bindings','remote-fixture.json'),bytes=await readFile(path,'utf8');
  await accounts.bind({...input,probeId:'expired-or-lost'});assert.equal(await readFile(path,'utf8'),bytes);
  await assert.rejects(accounts.bind({...input,requestId:'different'}),/binding_immutable/);
  await assert.rejects(accounts.bind({...input,id:'remote-bypass'}),/history_requires_reconciliation/);
  const fixed=await accounts.binding('remote-fixture');assert.deepEqual(fixed.connection.remoteBinding?.binding,binding);
  config.remoteAccounts!.targets[0]={...target,name:'changed'};const views=await accounts.list();assert.equal(views[0]!.unavailable,'remote_account_original_binding_unavailable');assert.equal(await readFile(path,'utf8'),bytes);
 }finally{await accounts.close();await rm(root,{recursive:true,force:true});}
});
test('new session requires an explicit successor; original binding remains immutable',async()=>{
 const root=await mkdtemp(join(tmpdir(),'qz-remote-successor-'));const config=configDefaults({dataDir:root,remoteAccounts:{targets:[target],origins:['http://127.0.0.1:8080']}});
 let current={...binding,nativeSessionId:null as string|null};let writes=0;
 const exchange=async(_target:unknown,input:unknown):Promise<RemoteAccountReply>=>{const f=input as RemoteAccountFrame;if(f.kind==='ACCOUNT_REQUEST'&&f.method==='POST')writes++;return {version:1,requestId:f.requestId,ok:true,binding:current,status:200,value:{...configValue,accountObservation:{nativeSessionId:current.nativeSessionId}}};};
 const accounts=await remoteAccounts(config,exchange);
 try{
  await assert.rejects(remoteAccounts(config,exchange),/owner_already_running/);
  const p=await accounts.probe({targetId:'trusted'});await accounts.bind({id:'remote-old',requestId:'save-old',probeId:p.probeId,targetId:p.targetId,binding:p.binding});
  const path=join(root,'remote-account-bindings','remote-old.json'),bytes=await readFile(path,'utf8');current={...current,nativeSessionId:'explicit-new-session'};
  const next=await accounts.probe({targetId:'trusted'});const input={id:'remote-new',requestId:'save-new',probeId:next.probeId,targetId:next.targetId,binding:next.binding};
  await assert.rejects(accounts.bind(input),/history_requires_reconciliation/);
  await accounts.bind({...input,supersedes:'remote-old'});assert.equal(await readFile(path,'utf8'),bytes);assert.equal(writes,0);
  const {downstreamRequest}=await import('../src/downstream.ts');const old=await accounts.binding('remote-old');
  await assert.rejects(downstreamRequest(old.connection,'strategy/commands','POST',{requestId:'never-send',action:'START'}),/binding_superseded/);assert.equal(writes,0);
  assert.equal((await accounts.list()).find(r=>r.id==='remote-old')!.supersededBy,'remote-new');
 }finally{await accounts.close();await rm(root,{recursive:true,force:true});}
});
test('publication fsync failure can verify the original saved request without a fresh probe',async()=>{
 const root=await mkdtemp(join(tmpdir(),'qz-remote-publication-'));const config=configDefaults({dataDir:root,remoteAccounts:{targets:[target],origins:['http://127.0.0.1:8080']}});let syncs=0;
 const exchange=async(_target:unknown,input:unknown):Promise<RemoteAccountReply>=>{const f=input as RemoteAccountFrame;assert.ok(f.kind==='HELLO'||f.method==='GET');return {version:1,requestId:f.requestId,ok:true,binding,status:200,value:configValue};};
 const accounts=await remoteAccounts(config,exchange,async()=>{if(++syncs===1)throw Error('injected_publication_sync_failed');});
 try{
  const p=await accounts.probe({targetId:'trusted'});const input={id:'remote-original',requestId:'save-original',probeId:p.probeId,targetId:p.targetId,binding:p.binding};
  await assert.rejects(accounts.bind(input),/injected_publication_sync_failed/);const path=join(root,'remote-account-bindings','remote-original.json'),bytes=await readFile(path,'utf8');
  await assert.rejects(accounts.bind({...input,requestId:'conflicting'}),/binding_immutable/);assert.equal(await readFile(path,'utf8'),bytes);
  const row=await accounts.bind({...input,probeId:'expired'});assert.equal(row.id,input.id);assert.equal(await readFile(path,'utf8'),bytes);assert.equal(syncs,2);
 }finally{await accounts.close();await rm(root,{recursive:true,force:true});}
});
test('unknown remote mutation reply remains 503 and keeps the original request available for read',async()=>{
 const root=await mkdtemp(join(tmpdir(),'qz-remote-unknown-'));const config=configDefaults({dataDir:root,remoteAccounts:{targets:[target],origins:['http://127.0.0.1:8080']}});const posts:unknown[]=[];
 const exchange=async(_target:unknown,input:unknown):Promise<RemoteAccountReply>=>{const f=input as RemoteAccountFrame;if(f.kind==='ACCOUNT_REQUEST'&&f.method==='POST'){posts.push(f.body);return {version:1,requestId:f.requestId,ok:false,error:'remote_account_outcome_unknown'};}return {version:1,requestId:f.requestId,ok:true,binding,status:200,value:configValue};};
 const accounts=await remoteAccounts(config,exchange);
 try{const p=await accounts.probe({targetId:'trusted'});await accounts.bind({id:'remote-unknown',requestId:'save',probeId:p.probeId,targetId:p.targetId,binding:p.binding});
  const {downstreamRequest}=await import('../src/downstream.ts');const original=await accounts.binding('remote-unknown');const body={requestId:'original-action',action:'START'};
  await assert.rejects(downstreamRequest(original.connection,'strategy/commands','POST',body),(error:unknown)=>(error as {statusCode:number;message:string}).statusCode===503&&(error as Error).message==='remote_account_outcome_unknown');
  await downstreamRequest(original.connection,'strategy/commands/original-action','GET');assert.deepEqual(posts,[body]);
 }finally{await accounts.close();await rm(root,{recursive:true,force:true});}
});
