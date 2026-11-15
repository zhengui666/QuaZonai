import {spawn} from 'node:child_process';
import {connect,type Socket} from 'node:net';
import {lstat,readFile} from 'node:fs/promises';
import {dirname,isAbsolute,join,basename} from 'node:path';
import {randomUUID} from 'node:crypto';
import {remoteAccountFrame,remoteAccountReply,MAX_REMOTE_ACCOUNT_FRAME_BYTES,MAX_REMOTE_ACCOUNT_REPLY_BYTES,type RemoteAccountFrame,type RemoteAccountReply} from '../../../packages/contracts/src/remote-account.ts';
import {CliError} from './profile.ts';
import {remoteAbsolutePath,remoteSshArguments,remoteSshEnvironment} from './remote-ssh.ts';
import {inspectedRemoteOwner} from './remote-owner.ts';

const record=(value:unknown):Record<string,unknown>=>{if(!value||typeof value!=='object'||Array.isArray(value))throw new CliError('remote_account_input_invalid');return value as Record<string,unknown>;};
export function remoteAccountTransport(value:unknown):{kind:'UNIX';socketPath:string}|{kind:'SSH';args:string[]}{
 const input=record(value);
 if(input.kind==='UNIX'){
  if(Object.keys(input).some(k=>!['kind','socketPath'].includes(k))||typeof input.socketPath!=='string'||!isAbsolute(input.socketPath))throw new CliError('remote_account_socket_invalid');
  return {kind:'UNIX',socketPath:input.socketPath};
 }
 if(input.kind!=='SSH'||Object.keys(input).some(k=>!['kind','host','user','port','knownHostsFile','relayExecutable','socketPath'].includes(k))
  ||!remoteAbsolutePath(input.relayExecutable)||!remoteAbsolutePath(input.socketPath))throw new CliError('remote_account_transport_invalid');
 return {kind:'SSH',args:remoteSshArguments(input,`${input.relayExecutable} remote-account relay --socket-path ${input.socketPath}`)};
}
async function privateSocket(path:string):Promise<void>{
 if(process.platform!=='linux'||!process.getuid||!isAbsolute(path)||basename(path)!=='account.sock')throw new CliError('remote_account_linux_private_socket_required');
 const dir=await lstat(dirname(path)),socket=await lstat(path);
 if(!dir.isDirectory()||dir.isSymbolicLink()||dir.uid!==process.getuid()||(dir.mode&0o777)!==0o700
  ||!socket.isSocket()||socket.isSymbolicLink()||socket.uid!==process.getuid()||(socket.mode&0o777)!==0o600)throw new CliError('remote_account_socket_not_private');
}
/** Single attempt only. Loss of transport is not evidence that a business action stopped. */
export async function exchangeRemoteAccount(target:unknown,input:unknown,signal?:AbortSignal):Promise<RemoteAccountReply>{
 const plan=remoteAccountTransport(target),frame=remoteAccountFrame(input),encoded=JSON.stringify(frame)+'\n';
 if(Buffer.byteLength(encoded)>MAX_REMOTE_ACCOUNT_FRAME_BYTES)throw new CliError('remote_account_frame_too_large');
 if(signal?.aborted)throw new CliError('remote_account_cancelled_before_send');
 if(plan.kind==='UNIX')await privateSocket(plan.socketPath);
 if(signal?.aborted)throw new CliError('remote_account_cancelled_before_send');
 return new Promise((resolve,reject)=>{
  let output=Buffer.alloc(0),settled=false;
  const child=plan.kind==='SSH'?spawn('ssh',plan.args,{stdio:['pipe','pipe','ignore'],env:remoteSshEnvironment()}):null;
  const socket:Socket|null=plan.kind==='UNIX'?connect(plan.socketPath):null;
  const readable=child?.stdout??socket!,writable=child?.stdin??socket!;
  const cleanup=()=>{clearTimeout(timer);signal?.removeEventListener('abort',fail);};
  function fail(){if(settled)return;settled=true;cleanup();socket?.destroy();child?.kill('SIGTERM');reject(new CliError('remote_account_outcome_unknown'));}
  function finish(){
   if(settled)return;
   try{
    const first=output.indexOf(10);if(first<0||first!==output.length-1)throw Error('frame');
    const reply=remoteAccountReply(JSON.parse(output.subarray(0,first).toString('utf8')),frame.requestId);
    settled=true;cleanup();socket?.destroy();resolve(reply);
   }catch{fail();}
  }
  const timer=setTimeout(fail,35_000);
  signal?.addEventListener('abort',fail,{once:true});
  readable.on('data',(chunk:Buffer)=>{if(output.length+chunk.length>MAX_REMOTE_ACCOUNT_REPLY_BYTES){fail();return;}output=Buffer.concat([output,chunk]);});
  readable.on('error',fail);writable.on('error',fail);
  if(child){child.once('error',fail);child.once('close',code=>code===0?finish():fail());writable.end(encoded);}
  else{socket!.once('connect',()=>writable.end(encoded));socket!.once('end',finish);socket!.once('close',()=>{if(!settled)fail();});}
 });
}
export async function remoteAccountRelay(socketPath:string|undefined,signal?:AbortSignal):Promise<void>{
 if(!socketPath)throw new CliError('remote_account_socket_required');
 let input=Buffer.alloc(0);let frame:RemoteAccountFrame|undefined;
 const timer=setTimeout(()=>process.stdin.destroy(new Error('remote_account_frame_timeout')),10_000);
 try{
  for await(const chunk of process.stdin){
   const bytes=Buffer.isBuffer(chunk)?chunk:Buffer.from(chunk);
   if(input.length+bytes.length>MAX_REMOTE_ACCOUNT_FRAME_BYTES)throw new CliError('remote_account_frame_too_large');
   input=Buffer.concat([input,bytes]);
  }
  const end=input.indexOf(10);if(end<0||end!==input.length-1)throw new CliError('remote_account_single_frame_required');
  frame=remoteAccountFrame(JSON.parse(input.subarray(0,end).toString('utf8')));
 }finally{clearTimeout(timer);}
 const reply=await exchangeRemoteAccount({kind:'UNIX',socketPath},frame,signal);
 process.stdout.write(JSON.stringify(reply)+'\n');
}
/** Explicitly serve an already saved account. No new account, compilation, or automatic native START. */
export async function serveRemoteAccount(input:unknown,emit:(value:unknown)=>void,signal?:AbortSignal):Promise<void>{
 const config=record(input);
 if(Object.keys(config).some(k=>!['socketDirectory','accountDirectory','ownerExecutable','expectedIdentity'].includes(k))
  ||![config.socketDirectory,config.accountDirectory,config.ownerExecutable].every(v=>typeof v==='string'&&isAbsolute(v)))throw new CliError('remote_account_serve_config_invalid');
 const expected=record(config.expectedIdentity);
 const fields=['nautilusSource','nautilusVersion','nautilusRevision','precision','jobVersion'] as const;
 if(Object.keys(expected).some(k=>!fields.includes(k as typeof fields[number]))||fields.some(k=>typeof expected[k]!=='string'||!String(expected[k]).trim())||!['standard','high'].includes(String(expected.precision)))throw new CliError('remote_owner_identity_required');
 if(signal?.aborted)throw new CliError('remote_account_cancelled_before_open');
 const {execFile}=await import('node:child_process');const {promisify}=await import('node:util');
 let stdout:string;
 try{({stdout}=await promisify(execFile)(String(config.ownerExecutable),['capabilities'],{signal,timeout:20_000,maxBuffer:1_048_576,env:{PATH:process.env.PATH}}));}
 catch{throw new CliError('remote_owner_inspection_failed_outcome_unknown');}
 const declared=inspectedRemoteOwner(JSON.parse(stdout),expected as Record<typeof fields[number],string>);
 if(record(declared.account).compiled!==true)throw new CliError('remote_owner_account_not_compiled');
 const {accountCreation}=await import('../../nautilus/src/account-configurations.ts');
 const saved=accountCreation(JSON.parse(await readFile(join(String(config.accountDirectory),'configuration.json'),'utf8')));
 if(saved.configuration.nativeStrategy.strategy.kind==='REGISTERED'&&record(declared.account).registration!==true)throw new CliError('remote_owner_account_registration_missing');
 const {createCapitalAccount}=await import('../../nautilus/src/capital-account.ts');
 const {startRemoteAccountSupervisor}=await import('../../nautilus/src/remote-account-supervisor.ts');
 if(signal?.aborted)throw new CliError('remote_account_cancelled_before_open');
 const owner=await startRemoteAccountSupervisor({socketDirectory:String(config.socketDirectory),runnerIdentity:JSON.stringify(Object.fromEntries(fields.map(k=>[k,expected[k]]))),
  clientId:saved.configuration.clientId,accountId:saved.configuration.accountId,
  openAccount:()=>createCapitalAccount(saved.configuration,String(config.accountDirectory),String(config.ownerExecutable))});
 emit({socketPath:owner.socketPath,deploymentId:owner.deploymentId,automaticConnection:false,automaticStrategyStart:false,sourceAttested:false});
 // Transport interruption never invokes shutdownOwner. The native child retains its receipt lease.
 await new Promise<void>(resolve=>{if(signal?.aborted)resolve();else signal?.addEventListener('abort',()=>resolve(),{once:true});});
 // Do not strand a surviving owner by removing its only control socket.
 emit({accepting:true,ownerShutdownRequested:false,outcome:'SUPERVISOR_REMAINS_AVAILABLE_OWNER_SHUTDOWN_NOT_REQUESTED'});
 await new Promise<void>(()=>{});
}
export function remoteAccountHello():RemoteAccountFrame{return {version:1,requestId:randomUUID(),kind:'HELLO'};}
