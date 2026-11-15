import {test} from 'node:test';
import assert from 'node:assert/strict';
import {connect} from 'node:net';
import {mkdtemp,rm,lstat,readFile} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {startRemoteAccountSupervisor,type RemoteAccountOwner} from '../src/remote-account-supervisor.ts';
import {remoteAccountFrame,remoteAccountReply,type RemoteAccountBinding,type RemoteAccountReply} from '../../../packages/contracts/src/remote-account.ts';
import {createCapitalAccount} from '../src/capital-account.ts';
import type {AccountOptions,NativeAccountRuntime} from '../src/account-runtime.ts';
const hello={version:1,requestId:'hello',kind:'HELLO'};
function exchange(path:string,frame:unknown):Promise<RemoteAccountReply>{return new Promise((resolve,reject)=>{
 const socket=connect(path);let data='';const timer=setTimeout(()=>{socket.destroy();reject(Error('test_socket_timeout'));},3000);
 socket.on('error',reject);socket.on('connect',()=>socket.end(JSON.stringify(frame)+'\n'));
 socket.on('data',chunk=>{data+=chunk.toString();});socket.on('end',()=>{clearTimeout(timer);try{resolve(remoteAccountReply(JSON.parse(data),String((frame as {requestId:string}).requestId)));}catch(error){reject(error);}});socket.on('close',()=>clearTimeout(timer));
});}
const command=(binding:RemoteAccountBinding,action='STOP')=>({version:1,requestId:'transport-1',kind:'ACCOUNT_REQUEST',binding,method:'POST',path:'/strategy/commands',body:{requestId:'original-1',action,clientId:'client',accountId:'account'}});
function fake(){let closed=0,calls=0,session:string|null=null;
 const account:RemoteAccountOwner={async request(path,method){if(path==='/strategy/config')return {status:200,value:{clientId:'client',accountId:'account',accountSourceId:'source',accountObservation:{nativeSessionId:session}}};if(method==='POST')calls++;return {status:200,value:{status:'UNCERTAIN'}};},async close(){closed++;}};
 return {account,get closed(){return closed;},get calls(){return calls;},setSession(value:string|null){session=value;}};
}
test('remote frames expose only business routes and strict original bindings',()=>{
 assert.deepEqual(remoteAccountFrame(hello),hello);
 const binding={deploymentId:'d',sourceId:'s',clientId:'c',accountId:'a',runnerIdentity:'fixed',nativeSessionId:null};
 for(const path of ['/BOOT','/SHUTDOWN','/v1/execute','/strategy/legacy-snapshot','/targets'])assert.throws(()=>remoteAccountFrame({...command(binding),path}));
 assert.throws(()=>remoteAccountFrame({...command(binding),operation:'BOOT'}));
 assert.throws(()=>remoteAccountReply({version:1,requestId:'wrong',ok:false,error:'bad'},'expected'));
 assert.throws(()=>remoteAccountReply({version:1,requestId:'expected',ok:true},'expected'));
});
test('HELLO does not dispatch, sessions and all original bindings fail closed, EOF does not close owner',async()=>{
 const root=await mkdtemp(join(tmpdir(),'qz-remote-'));const owner=fake();
 const supervisor=await startRemoteAccountSupervisor({socketDirectory:join(root,'socket'),runnerIdentity:'fixed',clientId:'client',accountId:'account',openAccount:async()=>owner.account});
 try{
  const first=await exchange(supervisor.socketPath,hello);assert.equal(first.ok,true);assert.equal(owner.calls,0);assert.equal(owner.closed,0);
  assert.equal((await lstat(join(root,'socket'))).mode&0o777,0o700);assert.equal((await lstat(supervisor.socketPath)).mode&0o777,0o600);
  for(const key of ['deploymentId','sourceId','clientId','accountId','runnerIdentity','nativeSessionId']){
   const reply=await exchange(supervisor.socketPath,command({...first.binding!,[key]:'changed'}));assert.equal(reply.error,'remote_account_binding_changed');
  }
  assert.equal(owner.calls,0);owner.setSession('new-session');assert.equal((await exchange(supervisor.socketPath,command(first.binding!))).error,'remote_account_binding_changed');
  const current=await exchange(supervisor.socketPath,hello);assert.equal((await exchange(supervisor.socketPath,command(current.binding!))).ok,true);assert.equal(owner.calls,1);assert.equal(owner.closed,0);
  await supervisor.stopAccepting();assert.equal(owner.closed,0);
 }finally{await supervisor.shutdownOwner();assert.equal(owner.closed,1);await rm(root,{recursive:true,force:true});}
});
test('disconnection after admission does not replay or shut down the owner',async()=>{
 const root=await mkdtemp(join(tmpdir(),'qz-disconnect-'));const owner=fake();let entered!:()=>void,release!:()=>void;
 const admitted=new Promise<void>(resolve=>{entered=resolve;}),blocked=new Promise<void>(resolve=>{release=resolve;});const request=owner.account.request.bind(owner.account);
 owner.account.request=async(path,method,client,body)=>{if(method==='POST'){entered();await blocked;}return request(path,method,client,body);};
 const supervisor=await startRemoteAccountSupervisor({socketDirectory:join(root,'socket'),runnerIdentity:'fixed',clientId:'client',accountId:'account',openAccount:async()=>owner.account});
 try{
  const binding=(await exchange(supervisor.socketPath,hello)).binding!;const socket=connect(supervisor.socketPath);socket.on('error',()=>{});socket.on('connect',()=>socket.write(JSON.stringify(command(binding))+'\n'));await admitted;socket.destroy();release();
  await exchange(supervisor.socketPath,hello);assert.equal(owner.calls,1);assert.equal(owner.closed,0);
 }finally{release();await supervisor.shutdownOwner();await rm(root,{recursive:true,force:true});}
});
test('real account receipts retain original CONFIGURE across reconnect with no native start',async()=>{
 const root=await mkdtemp(join(tmpdir(),'qz-original-'));let starts=0;
 const options:AccountOptions={id:'paper',clientId:'client',accountId:'account',mode:'paper',instruments:['TEST.SIM'],startingBalance:'100',paperFeeRate:'0.001',nativeStrategy:{instrument_id:'TEST.SIM',strategy:{kind:'REGISTERED',name:'owner',config:null}}};
 const runtime:NativeAccountRuntime={isConnected:()=>false,async start(){starts++;throw Error('must_not_start');},async request(){throw Error('native_not_started');},async close(){}};
 const directory=join(root,'account');
 const supervisor=await startRemoteAccountSupervisor({socketDirectory:join(root,'socket'),runnerIdentity:'fixed',clientId:'client',accountId:'account',openAccount:()=>createCapitalAccount(options,directory,'/never-execute',runtime)});
 try{
  const binding=(await exchange(supervisor.socketPath,hello)).binding!;
  const frame={version:1,kind:'ACCOUNT_REQUEST',requestId:'save',binding,method:'POST',path:'/strategy/config',body:{requestId:'save-original',clientId:'client',accountId:'account',expectedAccountSourceId:binding.sourceId,expectedStrategyRevision:'1',configuration:options.nativeStrategy}};
  const first=await exchange(supervisor.socketPath,frame);assert.equal(first.ok,true);assert.equal((first.value as {status:string}).status,'COMPLETED');
  const repeated=await exchange(supervisor.socketPath,{...frame,requestId:'new-transport'});assert.deepEqual(repeated.value,first.value);
  const receipt=await exchange(supervisor.socketPath,{version:1,kind:'ACCOUNT_REQUEST',requestId:'read',binding,method:'GET',path:'/strategy/commands/save-original'});assert.deepEqual(receipt.value,first.value);
  const state=JSON.parse(await readFile(join(directory,'capital-receipts.json'),'utf8'));assert.equal(Object.keys(state.strategyCommands).length,1);assert.equal(starts,0);
 }finally{await supervisor.shutdownOwner();await rm(root,{recursive:true,force:true});}
});

test('timed out admission cannot reach the owner',async()=>{
 const root=await mkdtemp(join(tmpdir(),'qz-admission-'));const owner=fake();let block=false,release!:()=>void;const wait=new Promise<void>(resolve=>{release=resolve;});
 const original=owner.account.request.bind(owner.account);owner.account.request=async(path,method,client,body)=>{if(block&&path==='/strategy/config')await wait;return original(path,method,client,body);};
 const supervisor=await startRemoteAccountSupervisor({socketDirectory:join(root,'socket'),runnerIdentity:'fixed',clientId:'client',accountId:'account',timeoutMs:100,openAccount:async()=>owner.account});
 try{
  const binding=(await exchange(supervisor.socketPath,hello)).binding!;block=true;
  const reply=await exchange(supervisor.socketPath,command(binding));assert.equal(reply.error,'remote_account_frame_timeout');release();block=false;
  await exchange(supervisor.socketPath,hello);assert.equal(owner.calls,0);
 }finally{release();await supervisor.shutdownOwner();await rm(root,{recursive:true,force:true});}
});
test('stale or reusable socket directories are never silently taken over',async()=>{
 const root=await mkdtemp(join(tmpdir(),'qz-stale-'));const owner=fake();let opens=0;
 try{await assert.rejects(startRemoteAccountSupervisor({socketDirectory:root,runnerIdentity:'fixed',clientId:'client',accountId:'account',openAccount:async()=>{opens++;return owner.account;}}),{code:'EEXIST'});assert.equal(opens,0);}
 finally{await rm(root,{recursive:true,force:true});}
});

test('concurrent stopAccepting and shutdownOwner wait for the same admitted request drain',async()=>{
 const root=await mkdtemp(join(tmpdir(),'qz-drain-'));const owner=fake();let entered!:()=>void,release!:()=>void;
 const admitted=new Promise<void>(resolve=>{entered=resolve;}),wait=new Promise<void>(resolve=>{release=resolve;});const original=owner.account.request.bind(owner.account);
 owner.account.request=async(path,method,client,body)=>{if(method==='POST'){entered();await wait;}return original(path,method,client,body);};
 const supervisor=await startRemoteAccountSupervisor({socketDirectory:join(root,'socket'),runnerIdentity:'fixed',clientId:'client',accountId:'account',openAccount:async()=>owner.account});
 try{
  const binding=(await exchange(supervisor.socketPath,hello)).binding!;const socket=connect(supervisor.socketPath);socket.on('error',()=>{});socket.on('connect',()=>socket.write(JSON.stringify(command(binding))+'\n'));await admitted;
  const stopping=supervisor.stopAccepting(),shutdown=supervisor.shutdownOwner();await new Promise(resolve=>setImmediate(resolve));assert.equal(owner.closed,0);
  release();await Promise.all([stopping,shutdown]);assert.equal(owner.calls,1);assert.equal(owner.closed,1);
 }finally{release();await supervisor.shutdownOwner();await rm(root,{recursive:true,force:true});}
});

test('socket permission failure drains listener before closing the owner',async()=>{
 const root=await mkdtemp(join(tmpdir(),'qz-permission-'));const owner=fake();const socketDirectory=join(root,'socket');
 try{
  await assert.rejects(startRemoteAccountSupervisor({socketDirectory,runnerIdentity:'fixed',clientId:'client',accountId:'account',openAccount:async()=>owner.account},async()=>{throw Error('controlled_permission_failure');}),/controlled_permission_failure/);
  assert.equal(owner.closed,1);
  await assert.rejects(exchange(join(socketDirectory,'account.sock'),hello));
 }finally{await rm(root,{recursive:true,force:true});}
});
