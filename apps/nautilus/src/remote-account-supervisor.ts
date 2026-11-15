import {randomUUID} from 'node:crypto';
import {createServer,type Socket} from 'node:net';
import {mkdir,lstat,chmod} from 'node:fs/promises';
import {join,isAbsolute} from 'node:path';
import {remoteAccountFrame,sameRemoteAccountBinding,MAX_REMOTE_ACCOUNT_FRAME_BYTES,MAX_REMOTE_ACCOUNT_REPLY_BYTES,type RemoteAccountBinding,type RemoteAccountReply} from '../../../packages/contracts/src/remote-account.ts';

export interface RemoteAccountOwner {
 request(path:string,method:'GET'|'POST',clientId:unknown,body?:unknown):Promise<{status:number;value:unknown}>;
 close():Promise<void>;
}
export interface RemoteAccountSupervisorOptions {
 socketDirectory:string; runnerIdentity:string; clientId:string; accountId:string;
 /** Must open the existing business account; it alone acquires the receipt lease. Never BOOT here. */
 openAccount():Promise<RemoteAccountOwner>;
 timeoutMs?:number;
}
const record=(v:unknown):Record<string,unknown>=>v&&typeof v==='object'&&!Array.isArray(v)?v as Record<string,unknown>:{};
/** Explicit owner lifecycle. SSH/relay disconnects never close or restart this account.
 * Access uses a private same-UID directory/socket, not an unimplemented SO_PEERCRED claim.
 * No TCP, arbitrary operation, binary path, credential, or environment is accepted on the wire.
 */
export async function startRemoteAccountSupervisor(options:RemoteAccountSupervisorOptions,secureSocket:typeof chmod=chmod){
 if(process.platform!=='linux'||!process.getuid||!isAbsolute(options.socketDirectory))throw Error('remote_account_linux_private_socket_required');
 if(!options.clientId||!options.accountId||!options.runnerIdentity||options.runnerIdentity.length>8192)throw Error('remote_account_owner_identity_invalid');
 const timeoutMs=options.timeoutMs??30_000;
 if(!Number.isSafeInteger(timeoutMs)||timeoutMs<10||timeoutMs>120_000)throw Error('remote_account_timeout_invalid');
 await mkdir(options.socketDirectory,{mode:0o700}); // Never reuse/delete a stale deployment directory.
 const directory=await lstat(options.socketDirectory);
 if(!directory.isDirectory()||directory.isSymbolicLink()||directory.uid!==process.getuid()||(directory.mode&0o777)!==0o700)throw Error('remote_account_socket_directory_not_private');
 const socketPath=join(options.socketDirectory,'account.sock');
 if(Buffer.byteLength(socketPath)>100)throw Error('remote_account_socket_path_too_long');
 const deploymentId=randomUUID();
 const account=await options.openAccount();
 let accepting=true,active=0,lane:Promise<unknown>=Promise.resolve(),closed:Promise<void>|undefined,stopping:Promise<void>|undefined;
 const clients=new Set<Socket>();
 async function binding():Promise<RemoteAccountBinding>{
  const response=await account.request('/strategy/config','GET',options.clientId);
  const value=record(response.value),observation=record(value.accountObservation);
  if(response.status!==200||typeof value.accountSourceId!=='string'||!value.accountSourceId||value.clientId!==options.clientId||value.accountId!==options.accountId)throw Error('remote_account_original_identity_unavailable');
  const session=observation.nativeSessionId;
  if(session!==undefined&&session!==null&&typeof session!=='string')throw Error('remote_account_session_invalid');
  return {deploymentId,sourceId:value.accountSourceId,clientId:options.clientId,accountId:options.accountId,runnerIdentity:options.runnerIdentity,nativeSessionId:typeof session==='string'?session:null};
 }
 const server=createServer({allowHalfOpen:true},socket=>{
  if(!accepting||clients.size>=16){socket.destroy();return;}
  clients.add(socket);socket.on('error',()=>{});socket.once('close',()=>clients.delete(socket));
  let buffer=Buffer.alloc(0),received=false,requestId='',dispatched=false,replied=false;
  const timer=setTimeout(()=>reply({version:1,requestId,ok:false,error:dispatched?'remote_account_outcome_unknown':'remote_account_frame_timeout'}),timeoutMs);timer.unref();
  function reply(value:RemoteAccountReply){
   clearTimeout(timer);if(socket.destroyed||socket.writableEnded||replied)return;replied=true;
   let encoded:string;try{encoded=JSON.stringify(value)+'\n';}catch{encoded=JSON.stringify({version:1,requestId,ok:false,error:'remote_account_reply_unavailable'})+'\n';}
   if(Buffer.byteLength(encoded)>MAX_REMOTE_ACCOUNT_REPLY_BYTES)encoded=JSON.stringify({version:1,requestId,ok:false,error:'remote_account_reply_too_large'})+'\n';
   socket.end(encoded);
  }
  socket.once('close',()=>clearTimeout(timer));
  socket.on('data',chunk=>{
   if(received){socket.destroy();return;}
   if(buffer.length+chunk.length>MAX_REMOTE_ACCOUNT_FRAME_BYTES){received=true;reply({version:1,requestId,ok:false,error:'remote_account_frame_too_large'});return;}
   buffer=Buffer.concat([buffer,chunk]);const end=buffer.indexOf(10);if(end<0)return;
   received=true;
   if(end!==buffer.length-1){reply({version:1,requestId,ok:false,error:'remote_account_single_frame_required'});return;}
   let frame:ReturnType<typeof remoteAccountFrame>;
   try{frame=remoteAccountFrame(JSON.parse(buffer.subarray(0,end).toString('utf8')));requestId=frame.requestId;}catch{reply({version:1,requestId,ok:false,error:'remote_account_frame_invalid'});return;}
   if(active>=16){reply({version:1,requestId,ok:false,error:'remote_account_capacity_exceeded'});return;}
   active++;
   const run=lane.then(async()=>{
    // A frame disconnected before admission has no mutation authority. No retry.
    if(!accepting||socket.destroyed||socket.writableEnded)return;
    const current=await binding();
    if(frame.kind==='HELLO'){reply({version:1,requestId,ok:true,binding:current});return;}
    if(!sameRemoteAccountBinding(frame.binding,current)){reply({version:1,requestId,ok:false,error:'remote_account_binding_changed'});return;}
    // No dispatch after a timeout while reading the binding.
    if(socket.destroyed||socket.writableEnded)return;
    dispatched=true;
    const result=await account.request(frame.path,frame.method,options.clientId,frame.body);
    reply({version:1,requestId,ok:result.status<400,status:result.status,value:result.value,binding:await binding()});
   }).catch(()=>reply({version:1,requestId,ok:false,error:dispatched?'remote_account_outcome_unknown':'remote_account_original_identity_unavailable'})).finally(()=>{active--;});
   lane=run;
  });
  socket.on('end',()=>{if(!received)socket.destroy();});
 });
 try{await new Promise<void>((resolve,reject)=>{server.once('error',reject);server.listen(socketPath,()=>{server.off('error',reject);resolve();});});await secureSocket(socketPath,0o600);}
 catch(error){await stopAccepting();await account.close();throw error;}
 function stopAccepting():Promise<void>{return stopping??=(async()=>{
  accepting=false;
  const done=new Promise<void>(resolve=>{if(server.listening)server.close(()=>resolve());else resolve();});
  for(const client of clients)client.destroy();
  await lane;await done;
  // Node owns removal of its Unix socket. Never unlink/reuse a stale deployment directory.
 })();}
 return {socketPath,deploymentId,stopAccepting,shutdownOwner:()=>closed??=(async()=>{await stopAccepting();await account.close();})()};
}
