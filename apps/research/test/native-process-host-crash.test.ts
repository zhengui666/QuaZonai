import assert from 'node:assert/strict';
import {test} from 'node:test';
import {mkdtemp,writeFile,readFile,rm} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {spawn} from 'node:child_process';
import {once} from 'node:events';
import {setTimeout as delay} from 'node:timers/promises';
import {BACKGROUND_CONTEXT as context} from '@earendil-works/chord/context';
import {createModels,fauxProvider} from '@earendil-works/pi-ai';
import {openResearch} from '../src/host.ts';
import {executeNativeValue,NativeRemoteUncertainty,remoteNativeExecutionStatus} from '../src/native-execution.ts';
import type {ProcessNativeTarget} from '../../../packages/contracts/src/nautilus.ts';

const options=(directory:string)=>{const models=createModels();models.setProvider(fauxProvider().provider);return {database:join(directory,'research.sqlite'),models};};
const target=(directory:string):ProcessNativeTarget=>({kind:'RUST_PROCESS',id:'host-crash-original',binary:join(directory,'runner.mjs'),capabilities:{schemaVersion:1,protocol:'quazonai-native-process-v1',identity:{nautilusVersion:'protocol-test-only',precision:'high'},research:{singleStrategyRegistration:false,configuredRunFactory:true},account:{compiled:false,registration:false,command:'account-runtime',requiresExplicitStart:true},running:false}});

test('service process SIGKILL preserves original running Pi identity as UNKNOWN, never automatically respawns',{timeout:30000},async()=>{
 const childDirectory=process.env.QZ_PROCESS_HOST_CRASH_CHILD;
 if(childDirectory){
  const host=await openResearch(options(childDirectory));
  await writeFile(join(childDirectory,'host-pid'),String(process.pid));
  await host.withNativeHost(()=>executeNativeValue(target(childDirectory),'native-configured-backtest',{},[],childDirectory,context));
  throw new Error('The parent must interrupt the host while the native process is still alive.');
 }
 const directory=await mkdtemp(join(tmpdir(),'qz-native-host-crash-'));
 const marker=join(directory,'launches');let runnerPid:number|undefined;
 await writeFile(join(directory,'runner.mjs'),`#!${process.execPath}\nimport{appendFileSync}from'node:fs';appendFileSync(${JSON.stringify(marker)},process.pid+'\\n');process.stdin.resume();setInterval(()=>{},1000);`,{mode:0o755});
 const env:NodeJS.ProcessEnv={...process.env,QZ_PROCESS_HOST_CRASH_CHILD:directory};delete env.NODE_TEST_CONTEXT;
 const child=spawn(process.execPath,['--experimental-strip-types','--test',import.meta.filename],{env,stdio:'ignore'});
 const exited=once(child,'exit');let host:Awaited<ReturnType<typeof openResearch>>|undefined;
 try{
  for(let i=0;i<200;i++){try{runnerPid=Number((await readFile(marker,'utf8')).trim());break;}catch{await delay(25);}}
  assert.ok(runnerPid,'native child really spawned before killing the owning host');
  const hostPid=Number(await readFile(join(directory,'host-pid'),'utf8'));
  assert.ok(Number.isSafeInteger(hostPid)&&hostPid>0);
  process.kill(hostPid,'SIGKILL');await exited;
  assert.throws(()=>process.kill(hostPid,0),{code:'ESRCH'},'The actual SQLite/Harness owner exited, not merely its test coordinator.');
  process.kill(runnerPid!,0); // Host death is not evidence of business cancellation.
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
  if(child.exitCode===null&&child.signalCode===null)child.kill('SIGKILL');
  if(runnerPid){
   try{process.kill(runnerPid,'SIGKILL');}catch{}
   let stopped=false;for(let i=0;i<100;i++){try{const stat=await readFile(`/proc/${runnerPid}/stat`,'utf8');if(/\) Z /.test(stat)){stopped=true;break;}}catch{stopped=true;break;}await delay(10);}
   assert.ok(stopped,'The isolated native test process must be exited (or reparented zombie), not running.');
  }
  await host?.close();await rm(directory,{recursive:true,force:true});
 }
});
