import assert from 'node:assert/strict';
import test from 'node:test';
import {remoteAccountTransport,exchangeRemoteAccount,remoteAccountHello} from '../src/remote-account.ts';

test('local and SSH use one account frame and no raw engine operation',()=>{
 const hello=remoteAccountHello();assert.equal(hello.kind,'HELLO');assert.equal(hello.version,1);
 const ssh=remoteAccountTransport({kind:'SSH',host:'example.invalid',user:'owner',knownHostsFile:'/pinned/hosts',relayExecutable:'/opt/qz/quazonai',socketPath:'/private/deploy/account.sock'});
 assert.equal(ssh.kind,'SSH');if(ssh.kind!=='SSH')throw Error();
 assert.equal(ssh.args.at(-1),'/opt/qz/quazonai remote-account relay --socket-path /private/deploy/account.sock');
 assert.ok(ssh.args.includes('StrictHostKeyChecking=yes'));
 assert.deepEqual(remoteAccountTransport({kind:'UNIX',socketPath:'/private/deploy/account.sock'}),{kind:'UNIX',socketPath:'/private/deploy/account.sock'});
});
test('transport rejects arbitrary command, environment, forwarded credentials and relative socket',()=>{
 const valid={kind:'SSH',host:'example.invalid',user:'owner',knownHostsFile:'/pinned/hosts',relayExecutable:'/opt/qz/quazonai',socketPath:'/private/deploy/account.sock'};
 for(const bad of [{...valid,command:'START'},{...valid,env:{HYPERLIQUID_PK:'x'}},{...valid,relayExecutable:'/bin/x;id'},{...valid,socketPath:'/x/../secret'},{kind:'UNIX',socketPath:'relative'}])assert.throws(()=>remoteAccountTransport(bad));
});
test('pre-aborted exchange never invokes SSH or a local socket',async()=>{
 await assert.rejects(exchangeRemoteAccount({kind:'UNIX',socketPath:'/no/account.sock'},remoteAccountHello(),AbortSignal.abort()),/cancelled_before_send/);
});

test('actual serve CLI reports unknown on first signal; repeated signal can terminate it without proving owner state',async()=>{
 const {mkdtemp,writeFile,mkdir,rm,readFile}=await import('node:fs/promises');
 const {tmpdir}=await import('node:os');const {join,resolve}=await import('node:path');const {spawn}=await import('node:child_process');
 const root=await mkdtemp(join(tmpdir(),'qz-cli-sig-'));const accountDirectory=join(root,'account');await mkdir(accountDirectory);
 const identity={nautilusSource:'fixture',nautilusVersion:'0.69.0',nautilusRevision:'fixed',precision:'high',jobVersion:'test'};
 const script=join(root,'owner.mjs'),executable=join(root,'owner');
 const capability={schemaVersion:1,protocol:'quazonai-native-process-v1',running:false,identity,account:{compiled:true,registration:true,command:'account-runtime',requiresExplicitStart:true},research:{singleStrategyRegistration:true,configuredRunFactory:false}};
 await writeFile(script,`if(process.argv[2]!=='capabilities')process.exit(91);console.log(${JSON.stringify(JSON.stringify(capability))});`);
 const quote=(s:string)=>"'"+s.replaceAll("'","'\\''")+"'";
 await writeFile(executable,`#!/bin/sh\nexec ${quote(process.execPath)} ${quote(script)} "$@"\n`,{mode:0o700});
 await writeFile(join(accountDirectory,'configuration.json'),JSON.stringify({requestId:'saved-creation',configuration:{id:'paper',clientId:'client',accountId:'account',mode:'paper',instruments:['TEST.SIM'],startingBalance:'100',paperFeeRate:'0.001',nativeStrategy:{instrument_id:'TEST.SIM',strategy:{kind:'REGISTERED',name:'owner',config:null}}}}));
 const file=join(root,'serve.json');await writeFile(file,JSON.stringify({socketDirectory:join(root,'s'),accountDirectory,ownerExecutable:executable,expectedIdentity:identity}));
 const child=spawn(process.execPath,['--experimental-strip-types',resolve(import.meta.dirname,'../src/main.ts'),'remote-account','serve','--file',file],{stdio:['ignore','pipe','pipe']});
 const lines:Array<Record<string,unknown>>=[];let buffer='',diagnostic='';
 child.stdout.on('data',chunk=>{buffer+=chunk.toString();let index:number;while((index=buffer.indexOf('\n'))>=0){lines.push(JSON.parse(buffer.slice(0,index)));buffer=buffer.slice(index+1);}});
 child.stderr.on('data',chunk=>{diagnostic=(diagnostic+chunk).slice(-4096);});
 const exit=new Promise<{code:number|null;signal:NodeJS.Signals|null}>(resolve=>child.once('exit',(code,signal)=>resolve({code,signal})));
 const until=async(predicate:()=>boolean)=>{const end=Date.now()+15000;while(!predicate()){if(child.exitCode!==null||child.signalCode!==null)throw Error(`serve exited: ${diagnostic}`);if(Date.now()>end)throw Error('serve_wait_timeout');await new Promise(resolve=>setTimeout(resolve,20));}};
 try{
  await until(()=>lines.some(x=>typeof x.socketPath==='string'));
  const before=await readFile(join(accountDirectory,'capital-receipts.json'),'utf8');
  child.kill('SIGINT');await until(()=>lines.some(x=>x.nativeOutcome==='UNKNOWN'));
  const report=lines.find(x=>x.nativeOutcome==='UNKNOWN')!;assert.equal(report.ownerShutdownRequested,false);assert.equal(report.signalIsolationVerified,false);
  child.kill('SIGINT');const stopped=await Promise.race([exit,new Promise<never>((_,reject)=>setTimeout(()=>reject(Error('serve_exit_timeout')),5000).unref())]);
  assert.equal(stopped.signal,'SIGINT');assert.equal(await readFile(join(accountDirectory,'capital-receipts.json'),'utf8'),before);
 }finally{child.kill('SIGKILL');await exit;await rm(root,{recursive:true,force:true});}
});
