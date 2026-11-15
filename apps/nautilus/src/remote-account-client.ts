import {spawn} from 'node:child_process';
import {connect,type Socket} from 'node:net';
import {lstat} from 'node:fs/promises';
import {dirname,isAbsolute,basename} from 'node:path';
import {randomUUID} from 'node:crypto';
import {remoteAccountFrame,remoteAccountReply,MAX_REMOTE_ACCOUNT_FRAME_BYTES,MAX_REMOTE_ACCOUNT_REPLY_BYTES,type RemoteAccountFrame,type RemoteAccountReply} from '../../../packages/contracts/src/remote-account.ts';
import {remoteAbsolutePath,remoteSshArguments,remoteSshEnvironment} from './remote-account-ssh.ts';
const record=(value:unknown):Record<string,unknown>=>{if(!value||typeof value!=='object'||Array.isArray(value))throw new Error('remote_account_input_invalid');return value as Record<string,unknown>;};
export function remoteAccountTransport(value:unknown):{kind:'UNIX';socketPath:string}|{kind:'SSH';args:string[]}{
 const input=record(value);
 if(input.kind==='UNIX'){
  if(Object.keys(input).some(k=>!['kind','socketPath'].includes(k))||typeof input.socketPath!=='string'||!isAbsolute(input.socketPath))throw new Error('remote_account_socket_invalid');
  return {kind:'UNIX',socketPath:input.socketPath};
 }
 if(input.kind!=='SSH'||Object.keys(input).some(k=>!['kind','host','user','port','knownHostsFile','relayExecutable','socketPath'].includes(k))
  ||!remoteAbsolutePath(input.relayExecutable)||!remoteAbsolutePath(input.socketPath))throw new Error('remote_account_transport_invalid');
 return {kind:'SSH',args:remoteSshArguments(input,`${input.relayExecutable} remote-account relay --socket-path ${input.socketPath}`)};
}
async function privateSocket(path:string):Promise<void>{
 if(process.platform!=='linux'||!process.getuid||!isAbsolute(path)||basename(path)!=='account.sock')throw new Error('remote_account_linux_private_socket_required');
 const dir=await lstat(dirname(path)),socket=await lstat(path);
 if(!dir.isDirectory()||dir.isSymbolicLink()||dir.uid!==process.getuid()||(dir.mode&0o777)!==0o700
  ||!socket.isSocket()||socket.isSymbolicLink()||socket.uid!==process.getuid()||(socket.mode&0o777)!==0o600)throw new Error('remote_account_socket_not_private');
}
/** Single attempt only. Loss of transport is not evidence that a business action stopped. */
export async function exchangeRemoteAccount(target:unknown,input:unknown,signal?:AbortSignal):Promise<RemoteAccountReply>{
 const plan=remoteAccountTransport(target),frame=remoteAccountFrame(input),encoded=JSON.stringify(frame)+'\n';
 if(Buffer.byteLength(encoded)>MAX_REMOTE_ACCOUNT_FRAME_BYTES)throw new Error('remote_account_frame_too_large');
 if(signal?.aborted)throw new Error('remote_account_cancelled_before_send');
 if(plan.kind==='UNIX')await privateSocket(plan.socketPath);
 if(signal?.aborted)throw new Error('remote_account_cancelled_before_send');
 return new Promise((resolve,reject)=>{
  let output=Buffer.alloc(0),settled=false;
  const child=plan.kind==='SSH'?spawn('ssh',plan.args,{stdio:['pipe','pipe','ignore'],env:remoteSshEnvironment()}):null;
  const socket:Socket|null=plan.kind==='UNIX'?connect(plan.socketPath):null;
  const readable=child?.stdout??socket!,writable=child?.stdin??socket!;
  const cleanup=()=>{clearTimeout(timer);signal?.removeEventListener('abort',fail);};
  function fail(){if(settled)return;settled=true;cleanup();socket?.destroy();child?.kill('SIGTERM');reject(new Error('remote_account_outcome_unknown'));}
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
export function remoteAccountHello():RemoteAccountFrame{return {version:1,requestId:randomUUID(),kind:'HELLO'};}
