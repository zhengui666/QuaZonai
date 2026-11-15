import {readFile} from 'node:fs/promises';
import {isAbsolute,join} from 'node:path';
import {remoteAccountFrame,MAX_REMOTE_ACCOUNT_FRAME_BYTES,type RemoteAccountFrame,type RemoteAccountReply} from '../../../packages/contracts/src/remote-account.ts';
import {exchangeRemoteAccount as exchange,remoteAccountHello,remoteAccountTransport} from '../../nautilus/src/remote-account-client.ts';
import {CliError} from './profile.ts';
import {inspectedRemoteOwner} from './remote-owner.ts';
export {remoteAccountHello,remoteAccountTransport};
const record=(value:unknown):Record<string,unknown>=>{if(!value||typeof value!=='object'||Array.isArray(value))throw new CliError('remote_account_input_invalid');return value as Record<string,unknown>;};
export async function exchangeRemoteAccount(target:unknown,input:unknown,signal?:AbortSignal):Promise<RemoteAccountReply>{
 try{return await exchange(target,input,signal);}catch(error){throw new CliError(error instanceof Error&&/^remote_account_[a-z_]+$/.test(error.message)?error.message:'remote_account_outcome_unknown');}
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
 // Keep this listener, but terminal process-group signals may also reach the
 // owner, and repeated signals may terminate this CLI. Neither proves a trade outcome.
 emit({accepting:true,ownerShutdownRequested:false,nativeOutcome:'UNKNOWN',signalIsolationVerified:false});
 await new Promise<void>(()=>{});
}
