import assert from 'node:assert/strict';
import {test} from 'node:test';
import {mkdtemp,writeFile,readFile,rm} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {spawn} from 'node:child_process';
import {DatabaseSync} from 'node:sqlite';
import {once} from 'node:events';
import {setTimeout as delay} from 'node:timers/promises';
import {BACKGROUND_CONTEXT as context} from '@earendil-works/chord/context';
import {createModels,fauxProvider} from '@earendil-works/pi-ai';
import {openResearch} from '../src/host.ts';
import {executeNativeValue,NativeRemoteUncertainty,remoteNativeExecutionStatus} from '../src/native-execution.ts';
import type {ProcessNativeTarget} from '../../../packages/contracts/src/nautilus.ts';

const options=(directory:string)=>{const models=createModels();models.setProvider(fauxProvider().provider);return {database:join(directory,'research.sqlite'),models};};
const target=(directory:string):ProcessNativeTarget=>({kind:'RUST_PROCESS',id:'host-crash-original',binary:join(directory,'runner.mjs'),capabilities:{schemaVersion:1,protocol:'quazonai-native-process-v1',identity:{nautilusVersion:'protocol-test-only',precision:'high'},research:{singleStrategyRegistration:false,configuredRunFactory:true},account:{compiled:false,registration:false,command:'account-runtime',requiresExplicitStart:true},running:false}});

for (const mode of ['idle','stream','final-result']) test(`service owner SIGKILL ${mode}: original Pi identity remains UNKNOWN without respawn`,{timeout:30000,skip:!!process.env.QZ_PROCESS_CRASH_MODE&&process.env.QZ_PROCESS_CRASH_MODE!==mode},async()=>{
 const childDirectory=process.env.QZ_PROCESS_HOST_CRASH_CHILD;
 if(childDirectory){
  const host=await openResearch(options(childDirectory));
  await writeFile(join(childDirectory,'host-pid'),String(process.pid));
  await host.withNativeHost(()=>executeNativeValue(target(childDirectory),'native-configured-backtest',{},[],childDirectory,context));
  throw new Error('The parent must interrupt the host while the native process is still alive.');
 }
 const directory=await mkdtemp(join(tmpdir(),'qz-native-host-crash-'));
 const marker=join(directory,'launches');let runnerPid:number|undefined;let hostPid:number|undefined;let lock:DatabaseSync|undefined;
 await writeFile(join(directory,'runner.mjs'),`#!${process.execPath}\nimport{appendFileSync,writeFileSync,existsSync}from'node:fs';import{once}from'node:events';import{setTimeout as delay}from'node:timers/promises';appendFileSync(${JSON.stringify(marker)},process.pid+'\\n');process.stdin.resume();
 if(${JSON.stringify(mode)}==='stream'){process.stdout.write('{"canonical_result":{"orders":[');for(let i=0;i<8000;i++)if(!process.stdout.write((i?',':'')+JSON.stringify({quantity:'0.12345678901234567890',detail:'x'.repeat(128)})))await once(process.stdout,'drain');writeFileSync(${JSON.stringify(join(directory,'streaming'))},'bytes-written');}
 if(${JSON.stringify(mode)}==='final-result'){writeFileSync(${JSON.stringify(join(directory,'ready'))},'ready');while(!existsSync(${JSON.stringify(join(directory,'finish'))}))await delay(10);process.stdout.end('{"native_result":"original-completed-value"}');await once(process.stdout,'finish');process.exit(0);}
 setInterval(()=>{},1000);`,{mode:0o755});
 const env:NodeJS.ProcessEnv={...process.env,QZ_PROCESS_HOST_CRASH_CHILD:directory,QZ_PROCESS_CRASH_MODE:mode};delete env.NODE_TEST_CONTEXT;
 const child=spawn(process.execPath,['--experimental-strip-types','--test',import.meta.filename],{env,stdio:'ignore'});
 const exited=once(child,'exit');let host:Awaited<ReturnType<typeof openResearch>>|undefined;
 try{
  for(let i=0;i<200;i++){try{runnerPid=Number((await readFile(marker,'utf8')).trim());break;}catch{await delay(25);}}
  assert.ok(runnerPid,'native child really spawned before killing the owning host');
  hostPid=Number(await readFile(join(directory,'host-pid'),'utf8'));
  if(mode!=='idle'){
   const ready=join(directory,mode==='stream'?'streaming':'ready');let seen=false;for(let i=0;i<200;i++){try{await readFile(ready);seen=true;break;}catch{await delay(10);}}assert.ok(seen);
   if(mode==='stream')await delay(100);
   else {
    // Hold the real SQLite write lock before allowing successful native exit.
    // No fabricated checkpoint: final evidence/outcome cannot commit through this lock.
    lock=new DatabaseSync(join(directory,'research.sqlite'));lock.exec('BEGIN IMMEDIATE');await writeFile(join(directory,'finish'),'exit');
    let nativeExited=false;for(let i=0;i<100;i++){try{process.kill(runnerPid!,0);}catch{nativeExited=true;break;}await delay(10);}assert.ok(nativeExited,'native exit 0 occurred before owner interruption');
   }
  }
  assert.ok(Number.isSafeInteger(hostPid)&&hostPid>0);
  process.kill(hostPid!,'SIGKILL');await exited;
  if(lock){lock.exec('ROLLBACK');lock.close();lock=undefined;}
  assert.throws(()=>process.kill(hostPid!,0),{code:'ESRCH'},'The actual SQLite/Harness owner exited, not merely its test coordinator.');
  if(mode!=='final-result')process.kill(runnerPid!,0); // Host death is not business cancellation.
  host=await openResearch(options(directory));
  const tasks=(await host.harness.commit(tx=>tx.scanTasks({kind:'qz.native.execution'},10),context)).items;
  assert.equal(tasks.length,1);const original=tasks[0]!;const requestId=(original.input as any).requestId;
  await host.harness.waitForTask(original.id,context);
  const facts=await host.harness.snapshot(NativeRemoteUncertainty,original.conversationId,String(original.id),context);
  assert.equal(facts!.requests[0]!.requestId,requestId);assert.equal(facts!.requests[0]!.status,'UNKNOWN');
  assert.equal((await remoteNativeExecutionStatus(target(directory),requestId)).status,'UNKNOWN');
  await host.close();host=await openResearch(options(directory));
  await host.harness.waitForTask(original.id,context);
  assert.equal((await readFile(marker,'utf8')).trim().split('\n').length,1);
  const again=(await host.harness.commit(tx=>tx.scanTasks({kind:'qz.native.execution'},10),context)).items;
  assert.equal(again.length,1);assert.equal(again[0]!.id,original.id);assert.equal((again[0]!.input as any).requestId,requestId);
 }finally{
  if(lock){lock.exec('ROLLBACK');lock.close();}
  if(hostPid)try{process.kill(hostPid,'SIGKILL');}catch{}
  if(child.exitCode===null&&child.signalCode===null)child.kill('SIGKILL');
  if(runnerPid){
   try{process.kill(runnerPid,'SIGKILL');}catch{}
   let stopped=false;for(let i=0;i<100;i++){try{const stat=await readFile(`/proc/${runnerPid}/stat`,'utf8');if(/\) Z /.test(stat)){stopped=true;break;}}catch{stopped=true;break;}await delay(10);}
   assert.ok(stopped,'The isolated native test process must be exited (or reparented zombie), not running.');
  }
  await host?.close();await rm(directory,{recursive:true,force:true});
 }
});
