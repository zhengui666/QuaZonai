import assert from 'node:assert/strict';
import {test} from 'node:test';
import {mkdtemp,writeFile,readFile,rm} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {BACKGROUND_CONTEXT as context} from '@earendil-works/chord/context';
import {createModels,fauxProvider} from '@earendil-works/pi-ai';
import {openResearch} from '../src/host.ts';
import {executeNativeValue,NativeRemoteUncertainty,remoteNativeExecutionStatus} from '../src/native-execution.ts';
import {NativeEvidencePart} from '../plugins/native-evidence.ts';
import {isNativeTable,readNativeTable} from '../src/native-table.ts';
import type {ProcessNativeTarget} from '../../../packages/contracts/src/nautilus.ts';

test('original Pi process tasks preserve paged output and crash UNKNOWN across SQLite reopen without re-execution',{timeout:1220000},async()=>{
 if(!process.env.QZ_PROCESS_DURABLE_CHILD){
  const {execFile}=await import('node:child_process');const {promisify}=await import('node:util');
  const env:NodeJS.ProcessEnv={...process.env,QZ_PROCESS_DURABLE_CHILD:'1'};delete env.NODE_TEST_CONTEXT;
  const child=await promisify(execFile)(process.execPath,['--max-old-space-size=160','--experimental-strip-types','--test',import.meta.filename],{env,timeout:115000,maxBuffer:1024*1024});
  assert.match(child.stdout,/pass 1|# pass 1/);return;
 }
 const directory=await mkdtemp(join(tmpdir(),'qz-durable-process-'));const binary=join(directory,'runner.mjs'),marker=join(directory,'launches');
 await writeFile(binary,`#!${process.execPath}\nimport{appendFileSync}from'node:fs';import{once}from'node:events';appendFileSync(${JSON.stringify(marker)},'start\\n');let text='';for await(const c of process.stdin)text+=c;const input=JSON.parse(text);if(input.crash)process.kill(process.pid,'SIGKILL');process.stdout.write('{"canonical_result":{"orders":[');const row=JSON.stringify({quantity:'0.12345678901234567890',detail:'original'.repeat(128)});for(let i=0;i<220000;i++)if(!process.stdout.write((i?',':'')+row))await once(process.stdout,'drain');process.stdout.write(']}}');`,{mode:0o755});
 const target:ProcessNativeTarget={kind:'RUST_PROCESS',id:'original-environment',binary,capabilities:{schemaVersion:1,protocol:'quazonai-native-process-v1',identity:{nautilusVersion:'protocol-only',precision:'high'},research:{singleStrategyRegistration:false,configuredRunFactory:true},account:{compiled:false,registration:false,command:'account-runtime',requiresExplicitStart:true},running:false}};
 const models=createModels();models.setProvider(fauxProvider().provider);const options={database:join(directory,'research.sqlite'),models};
 let host=await openResearch(options);
 try{
  const result=await host.withNativeHost(()=>executeNativeValue(target,'native-configured-backtest',{},[],directory,context)) as any;
  assert.ok(isNativeTable(result.canonical_result.orders));assert.equal(result.canonical_result.orders.rowCount,220000);
  const tasks=(await host.harness.commit(tx=>tx.scanTasks({kind:'qz.native.execution'},10),context)).items;
  assert.equal(tasks.length,1);const completed=tasks[0]!;
  assert.equal(completed.state.status,'terminal');
  await host.close();host=await openResearch(options);
  const table=result.canonical_result.orders;
  const rows=await readNativeTable(table,219999,1,async key=>(await host.harness.snapshot(NativeEvidencePart,table.conversationId,key,context))!.data);
  assert.equal((rows[0] as any).quantity,'0.12345678901234567890');
  await host.harness.waitForTask(completed.id,context);assert.equal((await readFile(marker,'utf8')).trim().split('\n').length,1);
  await assert.rejects(host.withNativeHost(()=>executeNativeValue(target,'native-configured-backtest',{crash:true},[],directory,context)),/unknown/);
  const failed=(await host.harness.commit(tx=>tx.scanTasks({kind:'qz.native.execution'},10),context)).items.find(t=>t.id!==completed.id)!;
  const original=(failed.input as any).requestId;
  const before=await readFile(marker,'utf8');
  await host.close();host=await openResearch(options);
  await host.harness.waitForTask(failed.id,context);
  const retained=await host.harness.snapshot(NativeRemoteUncertainty,failed.conversationId,String(failed.id),context);
  assert.equal(retained!.requests[0]!.requestId,original);assert.equal(retained!.requests[0]!.status,'UNKNOWN');
  assert.equal((await remoteNativeExecutionStatus(target,original)).status,'UNKNOWN');
  assert.equal(await readFile(marker,'utf8'),before,'Reopening or reading the old outcome never spawns a replacement.');
 }finally{await host.close();await rm(directory,{recursive:true,force:true});}
});
